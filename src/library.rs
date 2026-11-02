//! Synchronous, transactional library scanning. Call scans on a worker thread.
//!
//! Track identity is (canonical source root, relative path), not file content.
//! Audio is opened read-only; only the native database and cover cache are written.

use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    picture::PictureType,
    prelude::Accessor,
    probe::Probe,
};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use walkdir::WalkDir;

const MAX_COVER_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub sources: Vec<Source>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub id: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub id: String,
    pub source_id: String,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub artwork: Option<PathBuf>,
    pub present: bool,
}

pub struct Library {
    connection: Connection,
    covers: PathBuf,
}

impl Library {
    pub fn open(directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory).context("Create native library directory")?;
        let directory = directory.canonicalize()?;
        let covers = directory.join("covers");
        fs::create_dir_all(&covers).context("Create cover cache")?;
        let connection = Connection::open(directory.join("library.sqlite3"))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS sources (
                 id TEXT PRIMARY KEY NOT NULL,
                 root BLOB NOT NULL UNIQUE
             );
             CREATE TABLE IF NOT EXISTS tracks (
                 id TEXT PRIMARY KEY NOT NULL,
                 source_id TEXT NOT NULL REFERENCES sources(id),
                 relative_path BLOB NOT NULL,
                 title TEXT NOT NULL,
                 artist TEXT NOT NULL,
                 album TEXT NOT NULL,
                 duration REAL NOT NULL CHECK(duration >= 0),
                 artwork BLOB,
                 present INTEGER NOT NULL CHECK(present IN (0, 1)),
                 UNIQUE(source_id, relative_path)
             );",
        )?;
        let mut library = Self { connection, covers };
        library.prune_covers()?;
        Ok(library)
    }

    pub fn load(&self) -> Result<Snapshot> {
        // One read transaction keeps both collections consistent with other handles.
        let transaction = self.connection.unchecked_transaction()?;
        let sources = {
            let mut statement =
                transaction.prepare("SELECT id, root FROM sources ORDER BY root")?;
            statement
                .query_map([], |row| {
                    Ok(Source {
                        id: row.get(0)?,
                        root: decode_path(row.get(1)?),
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let tracks = {
            let mut statement = transaction.prepare(
                "SELECT t.id, t.source_id, s.root, t.relative_path, t.title, t.artist,
                        t.album, t.duration, t.artwork, t.present
                 FROM tracks t JOIN sources s ON s.id = t.source_id
                 ORDER BY s.root, t.relative_path",
            )?;
            statement
                .query_map([], |row| {
                    let root = decode_path(row.get(2)?);
                    let relative = decode_path(row.get(3)?);
                    Ok(Track {
                        id: row.get(0)?,
                        source_id: row.get(1)?,
                        path: root.join(relative),
                        title: row.get(4)?,
                        artist: row.get(5)?,
                        album: row.get(6)?,
                        duration: row.get(7)?,
                        artwork: row.get::<_, Option<Vec<u8>>>(8)?.map(decode_path),
                        present: row.get(9)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        transaction.commit()?;
        Ok(Snapshot { sources, tracks })
    }

    pub fn add_folder(&mut self, root: &Path) -> Result<Snapshot> {
        let root = root.canonicalize().context("Access music folder")?;
        let id = self
            .connection
            .query_row(
                "SELECT id FROM sources WHERE root = ?1",
                [encode_path(&root)],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        self.scan(Source { id, root })
    }

    pub fn rescan(&mut self, source_id: &str) -> Result<Snapshot> {
        let root = self
            .connection
            .query_row(
                "SELECT root FROM sources WHERE id = ?1",
                [source_id],
                |row| Ok(decode_path(row.get(0)?)),
            )
            .optional()?
            .context("Music folder not found")?;
        self.scan(Source {
            id: source_id.to_owned(),
            root,
        })
    }

    fn scan(&mut self, source: Source) -> Result<Snapshot> {
        // Finish traversal before touching either persistent state or the cover cache.
        // A failed walk must never mark an unvisited subtree as missing.
        let metadata = fs::symlink_metadata(&source.root).context("Access music folder")?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            bail!("Music source is not a directory (or has become a symlink)");
        }
        let mut paths = Vec::new();
        for entry in WalkDir::new(&source.root)
            .follow_links(false)
            .follow_root_links(false)
        {
            let entry = entry.context("Music folder could not be fully traversed")?;
            if entry.file_type().is_file() && is_audio(entry.path()) {
                paths.push(entry.into_path());
            }
        }
        paths.sort();
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO sources(id, root) VALUES(?1, ?2) ON CONFLICT(root) DO NOTHING",
            params![source.id, encode_path(&source.root)],
        )?;
        // Another handle may have registered this root during traversal.
        let source_id: String = transaction.query_row(
            "SELECT id FROM sources WHERE root = ?1",
            [encode_path(&source.root)],
            |row| row.get(0),
        )?;
        transaction.execute(
            "UPDATE tracks SET present = 0 WHERE source_id = ?1",
            [&source_id],
        )?;
        for path in paths {
            let relative = encode_path(path.strip_prefix(&source.root)?);
            // Parse failures are observations too: retain known metadata and identity,
            // but do not invent a track for an unreadable, previously unknown file.
            let Ok(observed) = read_track(&path) else {
                transaction.execute(
                    "UPDATE tracks SET present = 1 WHERE source_id = ?1 AND relative_path = ?2",
                    params![source_id, relative],
                )?;
                continue;
            };
            let artwork = observed
                .cover
                .as_deref()
                .and_then(|bytes| store_cover(&self.covers, bytes).ok().flatten())
                .map(|path| encode_path(&path));
            transaction.execute(
                "INSERT INTO tracks(id, source_id, relative_path, title, artist, album, duration, artwork, present)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1)
                 ON CONFLICT(source_id, relative_path) DO UPDATE SET
                   title = excluded.title, artist = excluded.artist, album = excluded.album,
                   duration = excluded.duration, artwork = excluded.artwork, present = 1",
                params![Uuid::new_v4().to_string(), source_id, relative, observed.title,
                        observed.artist, observed.album, observed.duration, artwork],
            )?;
        }
        transaction.commit()?;
        self.prune_covers()?;
        self.load()
    }

    fn prune_covers(&mut self) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let referenced = {
            let mut statement =
                transaction.prepare("SELECT artwork FROM tracks WHERE artwork IS NOT NULL")?;
            statement
                .query_map([], |row| Ok(decode_path(row.get(0)?)))?
                .collect::<rusqlite::Result<HashSet<_>>>()?
        };
        let mut retained = Vec::new();
        for entry in fs::read_dir(&self.covers)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".tmp") {
                fs::remove_file(path)?;
                continue;
            }
            if !is_digest(&name) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file()
                || !referenced.contains(&path)
                || metadata.len() > MAX_COVER_BYTES
            {
                if !metadata.is_dir() {
                    fs::remove_file(&path)?;
                }
                transaction.execute(
                    "UPDATE tracks SET artwork = NULL WHERE artwork = ?1",
                    [encode_path(&path)],
                )?;
            } else {
                retained.push((metadata.modified().ok(), metadata.len(), path));
            }
        }
        // Usually only unreferenced covers need pruning. If an existing cache is over
        // budget, evict oldest covers and clear references rather than return dead paths.
        retained.sort();
        let mut total: u64 = retained.iter().map(|entry| entry.1).sum();
        for (_, size, path) in retained {
            if total <= MAX_CACHE_BYTES {
                break;
            }
            fs::remove_file(&path)?;
            transaction.execute(
                "UPDATE tracks SET artwork = NULL WHERE artwork = ?1",
                [encode_path(&path)],
            )?;
            total -= size;
        }
        for path in referenced {
            if !path.is_file() {
                transaction.execute(
                    "UPDATE tracks SET artwork = NULL WHERE artwork = ?1",
                    [encode_path(&path)],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp3" | "m4a" | "flac" | "wav" | "aiff" | "aif" | "ogg" | "opus"
            )
        })
}

struct Observation {
    title: String,
    artist: String,
    album: String,
    duration: f64,
    cover: Option<Vec<u8>>,
}

fn read_track(path: &Path) -> Result<Observation> {
    if !fs::symlink_metadata(path)?.is_file() {
        bail!("Not a regular audio file");
    }
    let file = Probe::open(path)?.read()?;
    let tag = file.primary_tag().or_else(|| file.first_tag());
    let pictures = || file.tags().iter().flat_map(|tag| tag.pictures());
    let picture = pictures()
        .find(|picture| picture.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures().next());
    let cover = picture
        .filter(|picture| {
            !picture.data().is_empty()
                && picture.data().len() as u64 <= MAX_COVER_BYTES
                && picture.mime_type().is_some_and(|mime| {
                    matches!(
                        mime.as_str(),
                        "image/png"
                            | "image/jpeg"
                            | "image/webp"
                            | "image/gif"
                            | "image/bmp"
                            | "image/tiff"
                            | "image/ico"
                            | "image/x-portable-anymap"
                            | "image/svg+xml"
                    )
                })
        })
        .map(|picture| picture.data().to_vec());
    Ok(Observation {
        title: tag
            .and_then(|tag| tag.title())
            .map(|text| text.into_owned())
            .unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            }),
        artist: tag
            .and_then(|tag| tag.artist())
            .map(|text| text.into_owned())
            .unwrap_or_else(|| "Unknown artist".into()),
        album: tag
            .and_then(|tag| tag.album())
            .map(|text| text.into_owned())
            .unwrap_or_else(|| "Unknown album".into()),
        duration: file.properties().duration().as_secs_f64(),
        cover,
    })
}

fn is_digest(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn store_cover(directory: &Path, bytes: &[u8]) -> Result<Option<PathBuf>> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_COVER_BYTES {
        return Ok(None);
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    let path = directory.join(digest);
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
        // Do not trust partial/corrupt files left by external changes.
        if fs::read(&path)? == bytes {
            return Ok(Some(path));
        }
        return Ok(None);
    }
    let mut size = 0u64;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_file() {
            size = size.saturating_add(metadata.len());
        }
    }
    // A fully referenced cache cannot grow without bound; omit a new cover until
    // obsolete references have been pruned, rather than evict another source's art.
    if size.saturating_add(bytes.len() as u64) > MAX_CACHE_BYTES {
        return Ok(None);
    }
    let temporary = directory.join(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)?;
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result?;
    Ok(Some(path))
}

// Native path bytes avoid collisions and allow non-UTF-8 music filenames. This
// database is native-platform data, not a cross-platform interchange format.
#[cfg(unix)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
fn decode_path(bytes: Vec<u8>) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    std::ffi::OsString::from_vec(bytes).into()
}

#[cfg(windows)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(windows)]
fn decode_path(bytes: Vec<u8>) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    let wide: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    std::ffi::OsString::from_wide(&wide).into()
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
