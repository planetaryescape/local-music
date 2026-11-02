mod app;
mod library;
mod session;
mod telemetry;
mod ui;

use rune::{App, AppSpec};

fn main() {
    let spec = AppSpec {
        min_size: Some((780.0, 540.0)),
        setup: Some(app::setup),
        poll: Some(app::poll),
        on_key: Some(app::on_key),
        ..AppSpec::new("local-music", (1320.0, 860.0), app::State::new, ui::view)
    };
    App::new(spec).run();
}
