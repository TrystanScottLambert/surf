//! Surf: browse, filter and download WAVES data products stored in R2.

mod app;
mod assets;
mod screens;
mod services;
mod theme;
mod widgets;

fn main() -> iced::Result {
    let _log_guard = init_logging();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting surf");

    iced::application(app::App::new, app::App::update, app::App::view)
        .title(app::App::title)
        .theme(app::App::theme)
        .window(iced::window::Settings {
            icon: assets::window_icon(),
            ..Default::default()
        })
        .window_size((1280.0, 820.0))
        .centered()
        .run()
}

/// Log to `<data dir>/logs/surf.log` (rotated daily) and to stderr.
fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,surf=info,surf_gui=info,surf_storage=info,surf_query=info")
    });
    let stderr = fmt::layer().with_writer(std::io::stderr);

    let (file, guard) = match services::log_dir() {
        Some(dir) => {
            let appender = tracing_appender::rolling::daily(dir, "surf.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            (
                Some(fmt::layer().with_ansi(false).with_writer(writer)),
                Some(guard),
            )
        }
        None => (None, None),
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(stderr)
        .with(file)
        .init();
    guard
}
