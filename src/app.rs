use std::{cell::RefCell, collections::BTreeMap, path::PathBuf, sync::mpsc, time::Duration};

use rune::{Image, ImageCache, Key, KeyEvent, Player, PlayerState, TextEdit, Tx, Waker};

use crate::{
    library::{Library, Snapshot, Track},
    session::Session,
    telemetry::{LibraryOperation, Telemetry},
};

#[derive(Clone, PartialEq, Eq)]
pub enum View {
    Library,
    Albums,
    Tracks,
    Album(String, String),
}

enum Command {
    Load,
    Add(PathBuf),
    Rescan(String),
}

enum Reply {
    Saved(Snapshot),
    Warning(String),
    Complete(Result<Snapshot, String>),
}

pub struct State {
    pub snapshot: Snapshot,
    pub search: TextEdit,
    pub view: View,
    pub visible: Vec<usize>,
    pub session: Session,
    pub player: Option<Player>,
    pub images: Option<ImageCache>,
    artwork: RefCell<BTreeMap<(PathBuf, u32), (u64, Image)>>,
    pub error: Option<String>,
    pub busy: bool,
    pub queue_open: bool,
    pub progress: f64,
    pub duration: f64,
    pub playing: bool,
    pub revision: u64,
    last_search: String,
    started: bool,
    command: Option<mpsc::Sender<Command>>,
    worker: Option<std::thread::JoinHandle<()>>,
    replies: Option<mpsc::Receiver<Reply>>,
    waker: Option<Waker>,
}

impl State {
    pub fn new() -> Self {
        Self {
            snapshot: Snapshot {
                sources: Vec::new(),
                tracks: Vec::new(),
            },
            search: TextEdit::default(),
            view: View::Library,
            visible: Vec::new(),
            session: Session::default(),
            player: None,
            images: None,
            artwork: RefCell::default(),
            error: None,
            busy: true,
            queue_open: false,
            progress: 0.0,
            duration: 0.0,
            playing: false,
            revision: 0,
            last_search: String::new(),
            started: false,
            command: None,
            worker: None,
            replies: None,
            waker: None,
        }
    }

    pub fn show(&mut self, view: View) {
        self.view = view;
        self.rebuild();
    }

    pub fn artwork(&self, path: &std::path::Path, size: f32) -> Option<Image> {
        let cache = self.images.as_ref()?;
        let key = (path.to_path_buf(), size.ceil() as u32);
        let mut images = self.artwork.borrow_mut();
        let generation = rune::commit_generation();
        if !images.contains_key(&key) {
            // Retain async image handles, but never the entire library's decoded artwork.
            if images.len() >= 96 {
                let oldest = images
                    .iter()
                    .min_by_key(|(_, (used, _))| *used)
                    .map(|(key, _)| key.clone());
                if let Some(oldest) = oldest {
                    images.remove(&oldest);
                }
            }
            images.insert(key.clone(), (generation, cache.file(path, size, 2.0)));
        }
        let (used, image) = images.get_mut(&key)?;
        *used = generation;
        Some(image.clone())
    }

    fn rebuild(&mut self) {
        let query = self.search.text.trim().to_lowercase();
        self.visible = self
            .snapshot
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(index, track)| {
                let in_album = match &self.view {
                    View::Album(artist, album) => &track.artist == artist && &track.album == album,
                    _ => true,
                };
                let matches = query.is_empty()
                    || [
                        track.title.as_str(),
                        track.artist.as_str(),
                        track.album.as_str(),
                        track.path.to_string_lossy().as_ref(),
                    ]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&query));
                ((in_album || !query.is_empty()) && matches).then_some(index)
            })
            .collect();
        self.last_search.clone_from(&self.search.text);
        self.revision += 1;
    }

    pub fn start_track(&mut self, id: &str) {
        let tracks = self
            .visible
            .iter()
            .map(|&index| self.snapshot.tracks[index].clone())
            .collect();
        let label = match &self.view {
            _ if !self.search.text.is_empty() => format!("Search “{}”", self.search.text),
            View::Album(_, title) => title.clone(),
            _ => "All tracks".into(),
        };
        if self.session.start(label, tracks, id) {
            self.load_current();
        }
    }

    pub fn play_album(&mut self, artist: &str, album: &str) {
        let tracks: Vec<Track> = self
            .snapshot
            .tracks
            .iter()
            .filter(|track| track.artist == artist && track.album == album)
            .cloned()
            .collect();
        let id = tracks
            .iter()
            .find(|track| track.present)
            .map(|track| track.id.clone());
        if let Some(id) = id
            && self.session.start(album.into(), tracks, &id)
        {
            self.load_current();
        }
    }

    fn load_current(&mut self) {
        // A replaced AVPlayer must be paused before dropping our handle: Rune's backend may
        // still retain the old clip while its asynchronous load completes.
        if let Some(player) = self.player.take() {
            player.pause();
        }
        let Some(track) = self.session.current() else {
            return;
        };
        let player = Player::audio(track.path.clone());
        if let Some(waker) = &self.waker {
            player.set_waker(waker.clone());
        }
        player.play();
        self.duration = track.duration;
        self.progress = 0.0;
        self.playing = true;
        self.error = None;
        self.player = Some(player);
        self.revision += 1;
    }

    pub fn step(&mut self, delta: isize) {
        if self.session.step(delta) {
            self.load_current();
        }
    }

    pub fn select_queue(&mut self, id: &str) {
        if let Some(index) = self.session.tracks.iter().position(|track| track.id == id)
            && self.session.select(index)
        {
            self.load_current();
        }
    }

    pub fn toggle(&mut self) {
        if let Some(player) = &self.player {
            if player.is_playing() {
                player.pause();
            } else {
                player.play();
            }
            self.playing = player.is_playing();
            // Poll runs before input handlers. Resume needs another frame to restart its timer.
            if let Some(waker) = &self.waker {
                waker();
            }
        }
    }

    pub fn seek(&mut self, fraction: f64) {
        if let Some(player) = &self.player {
            player.seek(self.duration * fraction.clamp(0.0, 1.0));
            self.progress = player.time();
        }
    }

    pub fn add(&mut self, path: PathBuf) {
        self.send(Command::Add(path));
    }
    pub fn rescan(&mut self, id: String) {
        self.send(Command::Rescan(id));
    }

    fn send(&mut self, command: Command) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        if self
            .command
            .as_ref()
            .is_none_or(|sender| sender.send(command).is_err())
        {
            self.busy = false;
            self.error = Some("The library worker is not available.".into());
        }
    }
}

fn data_directory() -> anyhow::Result<PathBuf> {
    if let Some(path) = std::env::var_os("LOCAL_MUSIC_DATA") {
        return Ok(path.into());
    }
    directories::ProjectDirs::from("", "local-music", "local-music-native")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("Could not locate the application data directory."))
}

pub fn setup(state: &mut State, waker: Waker) {
    state.images = Some(ImageCache::new(waker.clone()));
    state.waker = Some(waker.clone());
    let (commands, receiver) = mpsc::channel();
    let (sender, replies) = mpsc::channel();
    state.command = Some(commands);
    state.replies = Some(replies);
    state.worker = Some(std::thread::spawn(move || {
        let telemetry = Telemetry::init()
            .map_err(|_| {
                eprintln!("Library telemetry could not be initialized; continuing without export.")
            })
            .ok();
        let mut library: Option<Library> = None;
        for command in receiver {
            let operation = match &command {
                Command::Load => LibraryOperation::Load,
                Command::Add(_) => LibraryOperation::AddFolder,
                Command::Rescan(_) => LibraryOperation::Rescan,
            };
            let run = || -> anyhow::Result<Snapshot> {
                if library.is_none() {
                    library = Some(Library::open(&data_directory()?)?);
                }
                let library = library
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("Library is closed."))?;
                match command {
                    Command::Load => {
                        let snapshot = library.load()?;
                        // Publish the saved library before rescanning it.
                        if sender.send(Reply::Saved(snapshot.clone())).is_err() {
                            return library.load();
                        }
                        waker();
                        for source in snapshot.sources {
                            let result = if let Some(telemetry) = &telemetry {
                                telemetry.trace_library_operation(LibraryOperation::Rescan, || {
                                    library.rescan(&source.id)
                                })
                            } else {
                                library.rescan(&source.id)
                            };
                            // An offline source must not prevent the remaining folders refreshing.
                            if let Err(error) = result {
                                let _ = sender.send(Reply::Warning(error.to_string()));
                                waker();
                            }
                        }
                        library.load()
                    }
                    Command::Add(path) => library.add_folder(&path),
                    Command::Rescan(id) => library.rescan(&id),
                }
            };
            let result = if let Some(telemetry) = &telemetry {
                telemetry.trace_library_operation(operation, run)
            } else {
                run()
            };
            if sender
                .send(Reply::Complete(result.map_err(|error| error.to_string())))
                .is_err()
            {
                break;
            }
            waker();
        }
        if let Some(telemetry) = telemetry {
            let _ = telemetry.shutdown();
        }
    }));
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(player) = &self.player {
            player.pause();
        }
        self.command.take();
        self.replies.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn poll(state: &mut State, seconds: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    if !state.started {
        if seconds < 0.05 {
            tx.wake_after(Duration::from_millis(50));
            return false;
        }
        state.started = true;
        if state
            .command
            .as_ref()
            .is_none_or(|command| command.send(Command::Load).is_err())
        {
            state.busy = false;
            state.error = Some("The library worker is not available.".into());
            changed = true;
        }
    }
    while let Some(reply) = state
        .replies
        .as_ref()
        .and_then(|replies| replies.try_recv().ok())
    {
        let result = match reply {
            Reply::Saved(snapshot) => Ok(snapshot),
            Reply::Warning(error) => {
                state.error = Some(error);
                changed = true;
                continue;
            }
            Reply::Complete(result) => {
                state.busy = false;
                result
            }
        };
        match result {
            Ok(snapshot) => {
                state.snapshot = snapshot;
                state.rebuild();
            }
            Err(error) => state.error = Some(error),
        }
        tx.instant();
        changed = true;
    }
    if state.search.text != state.last_search {
        state.rebuild();
        changed = true;
    }
    if let Some(player) = &state.player {
        match player.state() {
            PlayerState::Ended if state.playing => {
                state.playing = false;
                state.progress = state.duration;
                state.step(1);
                changed = true;
            }
            PlayerState::Ended => {}
            PlayerState::Failed(message) => {
                if state.error.as_ref() != Some(&message) {
                    state.error = Some(message);
                    changed = true;
                }
                state.playing = false;
            }
            _ => {
                let progress = player.time();
                let playing = player.is_playing();
                // Compare with the last published time, not the last 20 Hz backend tick.
                // Otherwise every sub-100 ms sample prevents the progress bar repainting.
                if (progress - state.progress).abs() >= 0.1 || playing != state.playing {
                    changed = true;
                    state.progress = progress;
                }
                state.playing = playing;
                state.duration = player.duration().unwrap_or(state.duration);
                if playing {
                    tx.wake_after(Duration::from_millis(100));
                }
            }
        }
    }
    changed
}

pub fn on_key(state: &mut State, event: &KeyEvent, tx: &mut Tx) -> bool {
    if event.is_command('k') {
        state.search.focused = true;
        return true;
    }
    if tx.focused() == Some("search") {
        return false;
    }
    match &event.key {
        Key::Text(key) if key == " " => {
            if !tx.key_repeat() {
                state.toggle();
            }
        }
        Key::Text(key) if key == "1" => state.show(View::Library),
        Key::Text(key) if key == "2" => state.show(View::Albums),
        Key::Text(key) if key == "3" => state.show(View::Tracks),
        Key::Escape => {
            state.queue_open = false;
            state.show(View::Library);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_repaints_with_frequent_backend_ticks() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tone.wav");
        let mut wav = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 8000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..16000 {
            wav.write_sample(0i16).unwrap();
        }
        wav.finalize().unwrap();
        let mut state = State::new();
        state.started = true;
        state.busy = false;
        let mut runtime = rune::Runtime::new(
            state,
            |state, _| rune::text(format!("{:.1}", state.progress)),
            (200.0, 100.0),
            rune::Env::default(),
        );
        runtime.frame(0.0);
        runtime.update(|state, _| {
            let player = Player::audio(path);
            player.play();
            state.player = Some(player);
            state.playing = true;
        });
        let mut repaints = 0;
        for _ in 0..12 {
            runtime.frame(0.05);
            repaints += usize::from(runtime.update_if(|state, tx| poll(state, 1.0, tx)));
        }
        runtime.frame(0.0);
        assert!(
            repaints >= 3,
            "progress must publish across 20 Hz callbacks"
        );
        assert!(runtime.state.progress >= 0.4);
        assert!(
            runtime
                .elements()
                .iter()
                .any(|el| el.text.as_deref().is_some_and(|text| text != "0.0"))
        );
    }
}
