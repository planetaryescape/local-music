use std::collections::BTreeMap;

use rune::{
    Color, Cx, El, FileDialogOptions, GrowthSpec, ScrollSpec, TextStyle, col, el, row, text,
    text_input,
};

use crate::{
    app::{State, View},
    library::Track,
};

#[derive(Clone, Copy)]
struct Palette {
    bg: Color,
    panel: Color,
    ink: Color,
    muted: Color,
    line: Color,
    accent: Color,
}

fn palette(dark: bool) -> Palette {
    let color = |value| rune::color::hex(value).unwrap_or(Color::BLACK);
    if dark {
        Palette {
            bg: color("#121412"),
            panel: color("#20231f"),
            ink: color("#f0eee7"),
            muted: color("#aaa9a2"),
            line: color("#333731"),
            accent: color("#ff7957"),
        }
    } else {
        Palette {
            bg: color("#ebe9e2"),
            panel: color("#f9f8f4"),
            ink: color("#171918"),
            muted: color("#656964"),
            line: color("#d2d1cb"),
            accent: color("#ad3f22"),
        }
    }
}

fn label(value: impl Into<String>, size: f32, color: Color) -> El {
    text(value)
        .font(TextStyle::new(size, 400.0))
        .color(color)
        .truncate()
}

fn button(key: &str, title: &str, p: Palette, action: impl Fn(&mut State) + 'static) -> El {
    row()
        .key(key)
        .h(32.0)
        .px(12.0)
        .center()
        .radius(6.0)
        .fill(p.panel)
        .hover(p.line)
        .focusable(true)
        .label(title)
        .focus_ring(2.0, 2.0, p.accent)
        .child(label(title, 12.0, p.ink))
        .on_press(action)
}

fn cover(state: &State, track: &Track, size: f32, p: Palette) -> El {
    let mut art = el().size(size, size).radius(4.0).fill(p.line).center();
    if let Some(image) = track
        .artwork
        .as_ref()
        .and_then(|path| state.artwork(path, size))
    {
        art = art.pixels(image.pixels());
    } else {
        art = art.child(label("◎", size * 0.38, p.muted));
    }
    art
}

fn add_folder(p: Palette) -> El {
    row()
        .key("add-folder")
        .h(34.0)
        .px(14.0)
        .center()
        .radius(6.0)
        .fill(p.ink)
        .hover(p.accent)
        .focusable(true)
        .label("Add folder")
        .focus_ring(2.0, 2.0, p.accent)
        .child(label("+  Add folder", 12.0, p.panel))
        .on_press_tx(|_: &mut State, tx| {
            tx.open_folder(
                FileDialogOptions {
                    title: "Choose a music folder".into(),
                    ..Default::default()
                },
                |state: &mut State, result, _| match result {
                    Ok(Some(path)) => state.add(path),
                    Ok(None) => {}
                    Err(_) => state.error = Some("Could not open the folder chooser.".into()),
                },
            );
        })
}

fn header(state: &State, p: Palette) -> El {
    let mut navigation = row().gap(4.0);
    for (title, key, view) in [
        ("Library  1", "library", View::Library),
        ("Albums  2", "albums", View::Albums),
        ("Tracks  3", "tracks", View::Tracks),
    ] {
        let active = state.view == view;
        navigation = navigation.child(
            button(key, title, p, move |state| state.show(view.clone())).fill(if active {
                p.line
            } else {
                p.panel
            }),
        );
    }
    row()
        .key("header")
        .w_full()
        .h(68.0)
        .px(24.0)
        .gap(20.0)
        .center_y()
        .fill(p.panel)
        .child(label("◎  local-music", 16.0, p.ink).w(145.0))
        .child(navigation)
        .child(
            text_input(&state.search, TextStyle::new(13.0, 400.0))
                .key("search")
                .label("Search your library")
                .grow(1.0)
                .min_w(100.0)
                .h(34.0)
                .px(12.0)
                .py(8.0)
                .single_line()
                .fill(p.bg)
                .radius(6.0)
                .placeholder("Search your music   ⌘K", p.muted)
                .on_edit(|state: &mut State| &mut state.search),
        )
        .child(add_folder(p))
}

fn album_groups(state: &State) -> Vec<(&Track, usize)> {
    let mut albums = BTreeMap::new();
    for &index in &state.visible {
        let track = &state.snapshot.tracks[index];
        let entry = albums
            .entry((&track.artist, &track.album))
            .or_insert((track, 0));
        entry.1 += 1;
        if entry.0.artwork.is_none() && track.artwork.is_some() {
            entry.0 = track;
        }
    }
    albums.into_values().collect()
}

fn album_card(state: &State, track: &Track, count: usize, size: f32, p: Palette) -> El {
    let artist = track.artist.clone();
    let album = track.album.clone();
    col()
        .key(format!("album-{}-{}", artist, album))
        .w(size)
        .gap(6.0)
        .focusable(true)
        .label(format!("Open {} by {}", album, artist))
        .focus_ring(2.0, 2.0, p.accent)
        .child(cover(state, track, size, p))
        .child(label(&album, 13.0, p.ink))
        .child(label(&artist, 12.0, p.muted))
        .child(label(format!("{count} tracks"), 10.0, p.muted))
        .on_press(move |state: &mut State| {
            state.search.text.clear();
            state.show(View::Album(artist.clone(), album.clone()));
        })
}

fn album_shelf(state: &State, p: Palette, width: f32) -> El {
    let mut shelf = row().gap(20.0);
    let count = ((width - 56.0) / 168.0).floor().max(1.0) as usize;
    for (track, count) in album_groups(state).into_iter().take(count.min(6)) {
        shelf = shelf.child(album_card(state, track, count, 148.0, p));
    }
    col()
        .gap(14.0)
        .child(label("Albums in your library", 18.0, p.ink))
        .child(shelf)
}

fn track_row(state: &State, index: usize, p: Palette, wide: bool) -> El {
    let track = &state.snapshot.tracks[state.visible[index]];
    let id = track.id.clone();
    let active = state
        .session
        .current()
        .is_some_and(|current| current.id == id);
    let title = if track.present {
        track.title.clone()
    } else {
        format!("{} · File missing", track.title)
    };
    let mut item = row()
        .h(58.0)
        .px(10.0)
        .gap(12.0)
        .center_y()
        .fill(if active {
            p.accent.alpha(0.12)
        } else {
            Color::TRANSPARENT
        })
        .hover(p.line.alpha(0.5))
        .child(
            label(
                if active && state.playing {
                    "Ⅱ".into()
                } else {
                    format!("{:02}", index + 1)
                },
                11.0,
                p.muted,
            )
            .w(30.0),
        )
        .child(cover(state, track, 40.0, p))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(3.0)
                .child(label(title, 13.0, if active { p.accent } else { p.ink }))
                .child(label(&track.artist, 11.0, p.muted)),
        );
    if wide {
        item = item.child(label(&track.album, 12.0, p.muted).w(180.0));
    }
    item = item.child(label(time(track.duration), 11.0, p.muted).w(48.0));
    if track.present {
        item = item
            .focusable(true)
            .label(format!("Play {} by {}", track.title, track.artist))
            .focus_ring(2.0, -2.0, p.accent)
            .on_press(move |state: &mut State| state.start_track(&id));
    }
    item
}

fn tracks(state: &State, p: Palette, wide: bool) -> El {
    if state.visible.is_empty() {
        return col()
            .grow(1.0)
            .center()
            .gap(12.0)
            .child(label(
                if state.snapshot.sources.is_empty() {
                    "Your library starts here"
                } else {
                    "No matching tracks"
                },
                22.0,
                p.ink,
            ))
            .child(label(
                if state.snapshot.sources.is_empty() {
                    "Add a folder. Your music stays exactly where it is."
                } else {
                    "Try another artist, album, title or file path."
                },
                13.0,
                p.muted,
            ))
            .child(add_folder(p));
    }
    let mut headings = row()
        .h(28.0)
        .px(10.0)
        .gap(12.0)
        .child(label("#", 10.0, p.muted).w(30.0))
        .child(label("TITLE / ARTIST", 10.0, p.muted).grow(1.0));
    if wide {
        headings = headings.child(label("ALBUM", 10.0, p.muted).w(180.0));
    }
    headings = headings.child(label("TIME", 10.0, p.muted).w(48.0));
    col()
        .grow(1.0)
        .min_h(0.0)
        .gap(10.0)
        .child(row().center_y().child(label(
            format!("Tracks · {}", state.visible.len()),
            18.0,
            p.ink,
        )))
        .child(headings)
        .child(
            col()
                .key("track-scroll")
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .grow(1.0)
                .min_h(0.0)
                .child(
                    col()
                        .key("track-rows")
                        .w_full()
                        .growth(GrowthSpec::default())
                        .virtual_rows(
                            state.visible.len(),
                            58.0,
                            |state: &State, index| {
                                state.snapshot.tracks[state.visible[index]]
                                    .id
                                    .as_str()
                                    .into()
                            },
                            move |state: &State, index| track_row(state, index, p, wide),
                        )
                        .virtual_rows_version(state.revision),
                ),
        )
}

fn album_grid(state: &State, p: Palette, width: f32) -> El {
    let columns = ((width - 56.0) / 180.0).floor().max(1.0) as usize;
    let rows = album_groups(state).len().div_ceil(columns);
    col()
        .key("album-scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .key("album-rows")
                .w_full()
                .growth(GrowthSpec::default())
                .virtual_rows(
                    rows,
                    244.0,
                    |_: &State, index| format!("album-row-{index}").into(),
                    move |state: &State, index| {
                        let mut line = row().h(244.0).gap(20.0);
                        for (track, count) in album_groups(state)
                            .into_iter()
                            .skip(index * columns)
                            .take(columns)
                        {
                            line = line.child(album_card(state, track, count, 160.0, p));
                        }
                        line
                    },
                )
                .virtual_rows_version(state.revision),
        )
}

fn queue(state: &State, p: Palette) -> El {
    let items = col()
        .key("queue-rows")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            state.session.tracks.len(),
            36.0,
            |state: &State, index| format!("queue-{}", state.session.tracks[index].id).into(),
            move |state: &State, index| {
                let id = state.session.tracks[index].id.clone();
                button(
                    &format!("queue-{index}"),
                    &state.session.tracks[index].title,
                    p,
                    move |state| state.select_queue(&id),
                )
                .fill(if state.session.index == Some(index) {
                    p.accent.alpha(0.15)
                } else {
                    p.panel
                })
            },
        )
        .virtual_rows_version(state.revision);
    col()
        .key("queue-panel")
        .w(260.0)
        .h_full()
        .padding(16.0)
        .gap(12.0)
        .fill(p.panel)
        .child(
            row()
                .child(label("Listening queue", 16.0, p.ink).grow(1.0))
                .child(button("close-queue", "Close", p, |state| {
                    state.queue_open = false
                })),
        )
        .child(label(&state.session.label, 11.0, p.muted))
        .child(
            col()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(items),
        )
}

fn player(state: &State, p: Palette) -> El {
    let Some(track) = state.session.current() else {
        return row()
            .h(82.0)
            .px(28.0)
            .center_y()
            .fill(p.panel)
            .child(label("◎   Nothing playing", 14.0, p.ink))
            .child(label("     Choose a track to begin", 12.0, p.muted));
    };
    let index = state.session.index.unwrap_or(0);
    let previous = button("previous", "⏮", p, |state| state.step(-1)).opacity(if index > 0 {
        1.0
    } else {
        0.35
    });
    let next = button("next", "⏭", p, |state| state.step(1)).opacity(
        if index + 1 < state.session.tracks.len() {
            1.0
        } else {
            0.35
        },
    );
    let controls = row()
        .center()
        .gap(8.0)
        .child(previous)
        .child(button(
            "play-pause",
            if state.playing { "Pause" } else { "Play" },
            p,
            State::toggle,
        ))
        .child(next);
    let fraction = (state.progress / state.duration.max(1.0)).clamp(0.0, 1.0);
    let scrub = el()
        .key("seek")
        .role("slider")
        .label("Playback position")
        .h(14.0)
        .grow(1.0)
        .center_y()
        .child(
            el().h(4.0).w_full().fill(p.line).radius(2.0).child(
                el().h(4.0)
                    .w_frac(fraction as f32)
                    .fill(p.accent)
                    .radius(2.0),
            ),
        )
        .focusable(true)
        .focus_ring(2.0, 2.0, p.accent)
        .on_key(|state: &mut State, event, _| {
            let delta = match event.key {
                rune::Key::Left => -5.0,
                rune::Key::Right => 5.0,
                _ => return false,
            };
            state.seek((state.progress + delta) / state.duration.max(1.0));
            true
        })
        .on_drag(|state: &mut State, drag, _| {
            if !drag.cancelled() {
                state.seek(f64::from(drag.x / drag.w.max(1.0)));
            }
        });
    row()
        .key("player")
        .h(82.0)
        .w_full()
        .px(24.0)
        .gap(24.0)
        .center_y()
        .fill(p.panel)
        .child(
            row()
                .w(280.0)
                .gap(12.0)
                .center_y()
                .child(cover(state, track, 44.0, p))
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(3.0)
                        .child(label(&track.title, 13.0, p.ink))
                        .child(label(
                            format!("{} · {}", track.artist, track.album),
                            11.0,
                            p.muted,
                        )),
                ),
        )
        .child(
            col().grow(1.0).min_w(120.0).gap(2.0).child(controls).child(
                row()
                    .gap(8.0)
                    .center_y()
                    .child(label(time(state.progress), 10.0, p.muted).w(40.0))
                    .child(scrub)
                    .child(label(time(state.duration), 10.0, p.muted).w(40.0)),
            ),
        )
        .child(button(
            "queue",
            &format!("Queue · {}", state.session.tracks.len()),
            p,
            |state| state.queue_open = !state.queue_open,
        ))
}

fn time(seconds: f64) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.0) as u64
    } else {
        0
    };
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub fn view(state: &State, cx: &mut Cx<State>) -> El {
    let p = palette(cx.env().system_dark);
    let [width, height] = cx.layout_size("app").unwrap_or([1320.0, 860.0]);
    let content_width = width - if state.queue_open { 260.0 } else { 0.0 };
    let title = if !state.search.text.is_empty() {
        format!("Results for “{}”", state.search.text)
    } else {
        match &state.view {
            View::Library => "Library".into(),
            View::Albums => "Albums".into(),
            View::Tracks => "All tracks".into(),
            View::Album(_, title) => title.clone(),
        }
    };
    let mut content = col()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .padding(28.0)
        .gap(22.0)
        .child(
            col()
                .gap(6.0)
                .child(label("YOUR COLLECTION", 10.0, p.accent))
                .child(label(title, 36.0, p.ink))
                .child(label(
                    state.error.clone().unwrap_or_else(|| {
                        if state.busy {
                            "Scanning music…".into()
                        } else {
                            format!(
                                "{} tracks · {} albums · {} folders",
                                state.visible.len(),
                                album_groups(state).len(),
                                state.snapshot.sources.len()
                            )
                        }
                    }),
                    12.0,
                    if state.error.is_some() {
                        p.accent
                    } else {
                        p.muted
                    },
                )),
        );
    if state.view == View::Library
        && state.search.text.is_empty()
        && !state.visible.is_empty()
        && height >= 760.0
    {
        content = content.child(album_shelf(state, p, content_width));
    }
    if let View::Album(artist, album) = &state.view
        && state.search.text.is_empty()
    {
        let (artist, album) = (artist.clone(), album.clone());
        content = content.child(
            row()
                .gap(10.0)
                .child(button("back", "← Albums", p, |state| {
                    state.show(View::Albums)
                }))
                .child(button("play-album", "▶ Play album", p, move |state| {
                    state.play_album(&artist, &album)
                })),
        );
    }
    if state.view == View::Albums && state.search.text.is_empty() {
        content = content.child(album_grid(state, p, content_width));
    } else {
        content = content.child(tracks(state, p, content_width >= 650.0));
    }
    let mut sources = row().gap(8.0);
    for source in &state.snapshot.sources {
        let id = source.id.clone();
        let name = source
            .root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        sources = sources.child(button(
            &format!("source-{id}"),
            &format!("↻ {name}"),
            p,
            move |state| state.rescan(id.clone()),
        ));
    }
    if !state.snapshot.sources.is_empty() {
        content = content.child(row().w_full().h(34.0).scroll_x().child(sources));
    }
    let mut body = row().grow(1.0).min_h(0.0).child(content);
    if state.queue_open {
        body = body.child(queue(state, p));
    }
    col()
        .key("app")
        .w_full()
        .h_full()
        .fill(p.bg)
        .on_key(crate::app::on_key)
        .child(header(state, p))
        .child(body)
        .child(player(state, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn fixture() -> (tempfile::TempDir, State) {
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
        for _ in 0..8000 {
            wav.write_sample(0i16).unwrap();
        }
        wav.finalize().unwrap();
        let mut state = State::new();
        state.busy = false;
        state.snapshot.tracks = (0..5000)
            .map(|index| Track {
                id: format!("track-{index}"),
                source_id: "source".into(),
                path: path.clone(),
                title: format!("Track {index}"),
                artist: "Artist".into(),
                album: if index < 2 {
                    "First album"
                } else {
                    "Other album"
                }
                .into(),
                duration: 1.0,
                artwork: None,
                present: index != 1,
            })
            .collect();
        state.show(View::Tracks);
        (directory, state)
    }

    fn tap(runtime: &mut Runtime<State>, key: &str) {
        let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("Missing {key}"));
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.1);
    }

    #[test]
    fn native_rows_play_and_browsing_does_not_replace_the_queue() {
        let (_directory, state) = fixture();
        let mut runtime = Runtime::new(state, view, (1320.0, 860.0), Env::default());
        runtime.frame(0.0);
        let rows: Vec<_> = runtime
            .elements()
            .into_iter()
            .filter(|el| {
                el.label
                    .as_deref()
                    .is_some_and(|label| label.starts_with("Play Track"))
            })
            .collect();
        assert!(!rows.is_empty(), "Playable rows must be rendered");
        assert!(rows.len() < 100, "Only viewport rows should be built");
        assert!(
            rows[0].visible[3] > 20.0,
            "First row must be visible: {:?}",
            rows[0].rect
        );
        tap(&mut runtime, "track-0");
        assert_eq!(runtime.state.session.current().unwrap().id, "track-0");
        assert_eq!(runtime.state.session.tracks.len(), 4999);
        runtime.key(rune::KeyEvent {
            key: rune::Key::Text(" ".into()),
            mods: Default::default(),
        });
        runtime.frame(0.05);
        assert!(
            !runtime.state.player.as_ref().unwrap().is_playing(),
            "Space pauses even when a track row has focus"
        );
        assert!(
            runtime
                .elements()
                .iter()
                .any(|el| el.label.as_deref() == Some("Play"))
        );
        runtime.key_up(rune::KeyEvent {
            key: rune::Key::Text(" ".into()),
            mods: Default::default(),
        });
        runtime.frame(0.0);
        runtime.key(rune::KeyEvent {
            key: rune::Key::Text(" ".into()),
            mods: Default::default(),
        });
        runtime.frame(0.05);
        assert!(runtime.state.player.as_ref().unwrap().is_playing());
        assert!(
            runtime
                .elements()
                .iter()
                .any(|el| el.label.as_deref() == Some("Pause")),
            "Resume must repaint without another pointer or keyboard event"
        );
        tap(&mut runtime, "albums");
        assert_eq!(runtime.state.session.tracks.len(), 4999);
        tap(&mut runtime, "queue");
        assert!(runtime.find("queue-panel").is_some());
        tap(&mut runtime, "queue-track-2");
        assert_eq!(runtime.state.session.current().unwrap().id, "track-2");
    }

    #[test]
    fn album_playback_is_scoped_but_search_is_global_and_small_windows_keep_tracks_visible() {
        let (_directory, mut state) = fixture();
        state.show(View::Album("Artist".into(), "First album".into()));
        state.play_album("Artist", "First album");
        assert_eq!(state.session.tracks.len(), 1);
        state.search.text = "Track 42".into();
        state.show(View::Library);
        assert_eq!(state.visible.len(), 111);
        assert_eq!(state.session.tracks.len(), 1);
        let mut runtime = Runtime::new(state, view, (780.0, 540.0), Env::default());
        runtime.frame(0.0);
        assert!(runtime.find("track-42").is_some());
        let row = runtime
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("track-42"))
            .unwrap();
        assert!(row.visible[2] > 200.0 && row.visible[3] > 20.0, "{:?}", row);
    }
}
