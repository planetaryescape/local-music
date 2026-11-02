use super::*;
use lofty::{
    config::WriteOptions,
    picture::{MimeType, Picture},
    tag::{Tag, TagExt, TagType},
};

struct Disk(PathBuf);

impl Disk {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("local-music-library-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn directory(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn open(&self) -> Library {
        Library::open(&self.0.join("native-data")).unwrap()
    }
}

impl Drop for Disk {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// A genuine one-second mono PCM WAV, with optional real ID3 metadata written by
// Lofty. Production scanning must leave these fixture bytes exactly unchanged.
fn wav(path: &Path, title: Option<&str>, cover: Option<Vec<u8>>) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&16036u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&8000u32.to_le_bytes());
    bytes.extend_from_slice(&16000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&16000u32.to_le_bytes());
    bytes.resize(16044, 0);
    fs::write(path, bytes).unwrap();
    if title.is_some() || cover.is_some() {
        let mut tag = Tag::new(TagType::Id3v2);
        if let Some(title) = title {
            tag.set_title(title.to_owned());
            tag.set_artist("A real artist".into());
            tag.set_album("A real album".into());
        }
        if let Some(bytes) = cover {
            tag.push_picture(Picture::new_unchecked(
                PictureType::CoverFront,
                Some(MimeType::Png),
                None,
                bytes,
            ));
        }
        tag.save_to_path(path, WriteOptions::default()).unwrap();
    }
}

fn png() -> Vec<u8> {
    // A valid one-pixel PNG, not a mocked image or metadata reader.
    vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15, 0, 1, 5,
        1, 1, 39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ]
}

#[test]
fn wav_metadata_artwork_and_reopen_preserve_audio_and_identity() {
    let disk = Disk::new();
    let music = disk.directory("music");
    let path = music.join("tagged.WAV");
    wav(&path, Some("An actual title"), Some(png()));
    let original = fs::read(&path).unwrap();
    let mut library = disk.open();
    assert!(library.load().unwrap().tracks.is_empty());
    let snapshot = library.add_folder(&music).unwrap();
    assert_eq!(snapshot.sources.len(), 1);
    assert_eq!(snapshot.sources[0].root, music.canonicalize().unwrap());
    assert_eq!(snapshot.tracks.len(), 1);
    let track = &snapshot.tracks[0];
    assert_eq!(track.title, "An actual title");
    assert_eq!(track.artist, "A real artist");
    assert_eq!(track.album, "A real album");
    assert_eq!(track.path, path.canonicalize().unwrap());
    assert_eq!(track.source_id, snapshot.sources[0].id);
    assert!((track.duration - 1.0).abs() < 0.01);
    assert!(track.present);
    assert_eq!(Uuid::parse_str(&track.id).unwrap().get_version_num(), 4);
    assert_eq!(fs::read(track.artwork.as_ref().unwrap()).unwrap(), png());
    assert_eq!(library.rescan(&snapshot.sources[0].id).unwrap(), snapshot);
    assert_eq!(fs::read(&path).unwrap(), original);
    drop(library);
    let mut reopened = disk.open();
    assert_eq!(reopened.load().unwrap(), snapshot);
    assert_eq!(reopened.add_folder(&music.join(".")).unwrap(), snapshot);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn missing_tracks_stay_saved_and_return_with_the_same_id() {
    let disk = Disk::new();
    let music = disk.directory("music");
    let path = music.join("untagged.wav");
    wav(&path, None, None);
    let mut library = disk.open();
    let before = library.add_folder(&music).unwrap();
    assert_eq!(before.tracks[0].title, "untagged");
    assert_eq!(before.tracks[0].artist, "Unknown artist");
    assert_eq!(before.tracks[0].album, "Unknown album");
    fs::remove_file(&path).unwrap();
    let missing = library.rescan(&before.sources[0].id).unwrap();
    assert_eq!(missing.tracks.len(), 1);
    assert!(!missing.tracks[0].present);
    assert_eq!(missing.tracks[0].id, before.tracks[0].id);
    drop(library);
    let mut library = disk.open();
    assert_eq!(library.load().unwrap(), missing);
    wav(&path, Some("Updated title"), None);
    let returned = library.rescan(&before.sources[0].id).unwrap();
    assert!(returned.tracks[0].present);
    assert_eq!(returned.tracks[0].id, before.tracks[0].id);
    assert_eq!(returned.tracks[0].title, "Updated title");
}

#[test]
fn unreadable_known_tracks_retain_metadata_and_unknown_tracks_are_not_invented() {
    let disk = Disk::new();
    let music = disk.directory("music");
    let path = music.join("known.wav");
    wav(&path, Some("Keep this metadata"), Some(png()));
    let mut library = disk.open();
    let before = library.add_folder(&music).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(!library.rescan(&before.sources[0].id).unwrap().tracks[0].present);
    fs::write(&path, b"not a readable WAV stream").unwrap();
    fs::write(music.join("new.wav"), b"also unreadable").unwrap();
    fs::write(music.join("not-a-track.txt"), b"ignored").unwrap();
    assert_eq!(library.rescan(&before.sources[0].id).unwrap(), before);
}

#[test]
fn rescan_reconciles_only_its_own_source() {
    let disk = Disk::new();
    let first = disk.directory("first");
    let second = disk.directory("second");
    wav(&first.join("same.wav"), None, None);
    wav(&second.join("same.wav"), None, None);
    let mut library = disk.open();
    let initial = library.add_folder(&first).unwrap();
    let both = library.add_folder(&second).unwrap();
    assert_ne!(both.tracks[0].id, both.tracks[1].id);
    fs::remove_file(first.join("same.wav")).unwrap();
    fs::remove_file(second.join("same.wav")).unwrap();
    let after = library.rescan(&initial.sources[0].id).unwrap();
    let first_track = after
        .tracks
        .iter()
        .find(|track| track.source_id == initial.sources[0].id)
        .unwrap();
    let second_track = after
        .tracks
        .iter()
        .find(|track| track.source_id != initial.sources[0].id)
        .unwrap();
    assert!(!first_track.present);
    assert_eq!(
        second_track,
        both.tracks
            .iter()
            .find(|track| track.id == second_track.id)
            .unwrap()
    );
}

#[test]
fn inaccessible_or_invalid_roots_do_not_change_persistent_snapshot() {
    let disk = Disk::new();
    let music = disk.directory("music");
    wav(&music.join("track.wav"), None, None);
    let mut library = disk.open();
    let before = library.add_folder(&music).unwrap();
    fs::rename(&music, disk.0.join("moved")).unwrap();
    assert!(library.rescan(&before.sources[0].id).is_err());
    assert_eq!(library.load().unwrap(), before);
    fs::write(&music, b"now a file, not a folder").unwrap();
    assert!(library.rescan(&before.sources[0].id).is_err());
    assert!(library.add_folder(&music).is_err());
    assert!(library.add_folder(&disk.0.join("absent")).is_err());
    assert!(library.rescan("not-a-source").is_err());
    assert_eq!(library.load().unwrap(), before);
    drop(library);
    assert_eq!(disk.open().load().unwrap(), before);
}

#[cfg(unix)]
#[test]
fn directory_traversal_failure_is_atomic_including_new_source_registration() {
    use std::os::unix::fs::PermissionsExt;
    let disk = Disk::new();
    let music = disk.directory("music");
    let nested = disk.directory("music/nested");
    wav(&music.join("top.wav"), None, None);
    wav(&nested.join("nested.wav"), None, None);
    let mut library = disk.open();
    let before = library.add_folder(&music).unwrap();
    fs::remove_file(music.join("top.wav")).unwrap();
    wav(&music.join("new.wav"), None, None);
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o0)).unwrap();
    let denied = fs::read_dir(&nested).is_err();
    let rescan = library.rescan(&before.sources[0].id);
    // Restore before assertions so fixture cleanup also works on test failure.
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        denied,
        "permission tests must run without root/DAC override privileges"
    );
    assert!(rescan.is_err());
    assert_eq!(library.load().unwrap(), before);

    let other = disk.directory("other");
    let blocked = disk.directory("other/blocked");
    wav(&other.join("uncommitted.wav"), None, None);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o0)).unwrap();
    let result = library.add_folder(&other);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(library.load().unwrap(), before);
    drop(library);
    assert_eq!(disk.open().load().unwrap(), before);
}

#[cfg(unix)]
#[test]
fn permission_denied_audio_is_a_present_observation_not_a_missing_track() {
    use std::os::unix::fs::PermissionsExt;
    let disk = Disk::new();
    let music = disk.directory("music");
    let path = music.join("track.wav");
    wav(&path, Some("Keep me"), None);
    let mut library = disk.open();
    let before = library.add_folder(&music).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
    let denied = fs::File::open(&path).is_err();
    let result = library.rescan(&before.sources[0].id);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        denied,
        "permission tests must run without root/DAC override privileges"
    );
    assert_eq!(result.unwrap(), before);
}

#[cfg(unix)]
#[test]
fn symlink_files_directories_and_cycles_are_not_followed() {
    use std::os::unix::fs::symlink;
    let disk = Disk::new();
    let music = disk.directory("music");
    let outside = disk.directory("outside");
    wav(&outside.join("outside.wav"), None, None);
    wav(&music.join("real.wav"), None, None);
    symlink(outside.join("outside.wav"), music.join("linked.wav")).unwrap();
    symlink(&outside, music.join("linked-directory")).unwrap();
    symlink(&music, music.join("cycle")).unwrap();
    let mut library = disk.open();
    let snapshot = library.add_folder(&music).unwrap();
    assert_eq!(snapshot.tracks.len(), 1);
    assert_eq!(snapshot.tracks[0].title, "real");
    let alias = disk.0.join("alias");
    symlink(&music, &alias).unwrap();
    assert_eq!(library.add_folder(&alias).unwrap(), snapshot);
    fs::rename(&music, disk.0.join("old-music")).unwrap();
    symlink(&outside, &music).unwrap();
    assert!(library.rescan(&snapshot.sources[0].id).is_err());
    assert_eq!(library.load().unwrap(), snapshot);
}

// APFS rejects invalid UTF-8 filenames, while Linux filesystems preserve bytes.
#[cfg(target_os = "linux")]
#[test]
fn non_unicode_paths_round_trip_without_loss_or_collisions() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let disk = Disk::new();
    let music = disk.directory("music");
    for byte in [0xfe, 0xff] {
        let path = music.join(OsString::from_vec(vec![byte, b'.', b'w', b'a', b'v']));
        wav(&path, None, None);
    }
    let mut library = disk.open();
    let before = library.add_folder(&music).unwrap();
    assert_eq!(before.tracks.len(), 2);
    assert_ne!(before.tracks[0].path, before.tracks[1].path);
    drop(library);
    assert_eq!(disk.open().load().unwrap(), before);
}

#[test]
fn covers_are_content_addressed_deduplicated_and_pruned_when_unreferenced() {
    let disk = Disk::new();
    let music = disk.directory("music");
    wav(&music.join("one.wav"), None, Some(png()));
    wav(&music.join("two.wav"), None, Some(png()));
    let mut library = disk.open();
    let before = library.add_folder(&music).unwrap();
    assert_eq!(before.tracks[0].artwork, before.tracks[1].artwork);
    let cover = before.tracks[0].artwork.as_ref().unwrap();
    assert_eq!(
        cover.file_name().unwrap().to_str().unwrap(),
        format!("{:x}", Sha256::digest(png()))
    );
    assert_eq!(fs::read_dir(&library.covers).unwrap().count(), 1);
    wav(&music.join("one.wav"), None, None);
    let still_referenced = library.rescan(&before.sources[0].id).unwrap();
    assert!(still_referenced.tracks[0].artwork.is_none());
    assert!(cover.is_file());
    wav(&music.join("two.wav"), None, None);
    let no_covers = library.rescan(&before.sources[0].id).unwrap();
    assert!(no_covers.tracks.iter().all(|track| track.artwork.is_none()));
    assert!(!cover.exists());
    assert_eq!(fs::read_dir(&library.covers).unwrap().count(), 0);
}

#[test]
fn cache_admission_is_bounded_and_reopen_prunes_orphans_and_temporaries() {
    let disk = Disk::new();
    let music = disk.directory("music");
    // Genuine embedded covers exercise admission through the caller's interface.
    // Sixteen 8 MiB images fit; the seventeenth must not grow the cache.
    for index in 0..17 {
        let mut cover = png();
        cover.push(index);
        cover.resize(MAX_COVER_BYTES as usize, 0);
        wav(&music.join(format!("{index}.wav")), None, Some(cover));
    }
    let mut library = disk.open();
    let snapshot = library.add_folder(&music).unwrap();
    assert_eq!(snapshot.tracks.len(), 17);
    assert_eq!(
        snapshot
            .tracks
            .iter()
            .filter(|track| track.artwork.is_some())
            .count(),
        16
    );
    assert_eq!(
        fs::read_dir(&library.covers)
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .sum::<u64>(),
        MAX_CACHE_BYTES
    );
    fs::write(library.covers.join("interrupted.tmp"), b"partial write").unwrap();
    let orphan = library
        .covers
        .join(format!("{:x}", Sha256::digest(b"orphan")));
    fs::write(&orphan, b"orphan").unwrap();
    drop(library);
    let reopened = disk.open();
    assert!(!orphan.exists());
    assert!(!reopened.covers.join("interrupted.tmp").exists());
    assert_eq!(fs::read_dir(&reopened.covers).unwrap().count(), 16);
    assert_eq!(reopened.load().unwrap(), snapshot);
}

#[test]
fn over_budget_existing_cache_is_pruned_without_dangling_database_references() {
    let disk = Disk::new();
    let music = disk.directory("music");
    for index in 0..17 {
        let mut cover = png();
        cover.push(index);
        wav(&music.join(format!("{index}.wav")), None, Some(cover));
    }
    let mut library = disk.open();
    let snapshot = library.add_folder(&music).unwrap();
    // Simulate externally expanded files in an existing real cache using sparse
    // on-disk files. Reopening must restore the budget and clear evicted paths.
    for track in &snapshot.tracks {
        fs::OpenOptions::new()
            .write(true)
            .open(track.artwork.as_ref().unwrap())
            .unwrap()
            .set_len(MAX_COVER_BYTES)
            .unwrap();
    }
    drop(library);
    let reopened = disk.open();
    let after = reopened.load().unwrap();
    assert_eq!(after.tracks.len(), 17);
    assert_eq!(
        after
            .tracks
            .iter()
            .filter(|track| track.artwork.is_some())
            .count(),
        16
    );
    assert!(
        after
            .tracks
            .iter()
            .filter_map(|track| track.artwork.as_ref())
            .all(|path| path.is_file())
    );
    assert_eq!(
        fs::read_dir(&reopened.covers)
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .sum::<u64>(),
        MAX_CACHE_BYTES
    );
}

#[test]
fn oversized_embedded_art_is_omitted_without_dropping_the_track() {
    let disk = Disk::new();
    let music = disk.directory("music");
    let mut cover = png();
    cover.resize(MAX_COVER_BYTES as usize + 1, 0);
    wav(&music.join("large.wav"), Some("Still a track"), Some(cover));
    let mut library = disk.open();
    let snapshot = library.add_folder(&music).unwrap();
    assert_eq!(snapshot.tracks.len(), 1);
    assert_eq!(snapshot.tracks[0].title, "Still a track");
    assert!(snapshot.tracks[0].artwork.is_none());
}
