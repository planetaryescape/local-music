# local-music

A local-file-first macOS music player, rewritten in Rust with [Rune](https://github.com/anomalyco/rune), Lofty, and SQLite. No Electron, webview, JavaScript runtime, or server.

## Run on macOS

Install Xcode Command Line Tools and [Rustup](https://rustup.rs), then:

```sh
cargo run --locked
```

The pinned Rust toolchain is installed by Rustup. Rune is pinned to a Git revision in `Cargo.toml` and `Cargo.lock`. This is an experimental development application, not a signed or packaged distribution. **Playback requires macOS**: Rune's current audio backend uses AVPlayer; Linux and Windows playback are not supported.

## Library and playback

- Add folders through the native folder chooser. Index MP3, M4A, FLAC, WAV, AIFF, OGG, and Opus files.
- Browse Library, Albums, and Tracks. Open an album or search by title, artist, album, or path.
- Select a track to play its current browsing context, or choose **Play album**. Browsing and searching do not replace the captured queue. Missing files stay saved but cannot enter a new queue.
- Play/pause, previous/next, queue selection, automatic advance, and seeking are supported. Drag the position bar, or focus it and use ←/→ to seek five seconds.
- Press ⌘K to search; 1/2/3 switch lenses when not editing text; Space toggles playback. Escape returns to Library when no control owns keyboard focus.
- Saved metadata appears before background startup rescans. Folder buttons rescan only that source. Audio and embedded tags are never changed. Symlinks are not followed. Missing tracks return with their original identity.
- Artwork is content-addressed and bounded: 8 MiB per cover, 128 MiB on disk, and at most 96 retained decoded-image handles. Tracks, album rows, and queues are virtualized. Light/dark appearance follows macOS.

The rewrite uses a **new native database**, separate from Electron's data. Add your music folders again; old application data is neither migrated nor deleted. The default directory comes from `directories::ProjectDirs("", "local-music", "local-music-native")`; set `LOCAL_MUSIC_DATA` to use another directory.

Not included: playlist editing, file moves, persisted listening sessions, system media-key integration, signed releases, or native screen-reader integration. Rune does not yet expose a platform accessibility bridge; keyboard control labels are not a claim of VoiceOver support.

## Verify

Run on a Mac (Amp orbs can start a Mac satellite):

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
```

Tests use real WAV files, tags, cover bytes, SQLite, and Rune's input/layout runtime. They cover transactional scans, stable identity, missing/unreadable files, source isolation, cache bounds, contextual queues, search, and virtualized rendering with 5,000 tracks. The invalid-UTF-8 filename filesystem test runs on Linux only: macOS APFS rejects those names.

Rune's `--snap`, `--tree`, and scripted headless runs are useful for layout checks, but **headless audio uses a virtual clock and produces no sound**. Verify playback in the native window with real files, not just those tests. `.agents/setup` fetches dependencies in a Linux orb; native build and playback checks run on a Mac satellite.

## Optional tracing

Without `OTEL_EXPORTER_OTLP_ENDPOINT`, no telemetry is exported. To opt in, configure the launching process with your collector endpoint, `OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf`, and `OTEL_EXPORTER_OTLP_HEADERS` through your secret manager. `OTEL_RESOURCE_ATTRIBUTES` may supply the deployment environment. Never commit bearer tokens.

Library load, add-folder, and rescan spans retain the service name `local-music-desktop`. They record duration and failure status, not local paths, music metadata, exception details, or automatically detected host/process resources. Exports run on an SDK worker and flush on normal shutdown. The Planetaryescape public collector is `https://otel.planetaryescape.co.za` and requires a bearer token.

## Code

`src/main.rs` starts the native shell. `app.rs` owns state, the library worker, and playback; `ui.rs` renders Rune controls; `library.rs` owns scanning, SQLite, and artwork; `session.rs` captures listening context; `telemetry.rs` owns optional export. Historical `.specs` describe the earlier application and are not the native implementation contract.
