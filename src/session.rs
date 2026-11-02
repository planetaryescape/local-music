use crate::library::Track;

/// A listening context is captured when playback starts, never changed by browsing.
#[derive(Default)]
pub struct Session {
    pub label: String,
    pub tracks: Vec<Track>,
    pub index: Option<usize>,
}

impl Session {
    pub fn start(&mut self, label: String, tracks: Vec<Track>, id: &str) -> bool {
        let tracks: Vec<_> = tracks.into_iter().filter(|track| track.present).collect();
        let Some(index) = tracks.iter().position(|track| track.id == id) else {
            return false;
        };
        *self = Self {
            label,
            tracks,
            index: Some(index),
        };
        true
    }

    pub fn current(&self) -> Option<&Track> {
        self.index.and_then(|index| self.tracks.get(index))
    }

    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.tracks.len() {
            return false;
        }
        self.index = Some(index);
        true
    }

    pub fn step(&mut self, delta: isize) -> bool {
        let Some(index) = self.index.and_then(|index| index.checked_add_signed(delta)) else {
            return false;
        };
        self.select(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn track(id: &str, present: bool) -> Track {
        Track {
            id: id.into(),
            source_id: "source".into(),
            path: PathBuf::from(id),
            title: id.into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration: 20.0,
            artwork: None,
            present,
        }
    }

    #[test]
    fn queue_keeps_context_skips_missing_and_stops_at_both_ends() {
        let mut session = Session::default();
        assert!(session.start(
            "Album".into(),
            vec![track("a", true), track("missing", false), track("b", true)],
            "a"
        ));
        assert!(!session.step(-1));
        assert!(session.step(1));
        assert_eq!(session.current().map(|track| track.id.as_str()), Some("b"));
        assert!(!session.step(1));
        assert_eq!(session.label, "Album");
        assert!(!session.start("Bad".into(), vec![track("missing", false)], "missing"));
        assert_eq!(session.current().map(|track| track.id.as_str()), Some("b"));
    }
}
