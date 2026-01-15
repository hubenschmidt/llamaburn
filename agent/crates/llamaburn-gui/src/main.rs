mod app;
mod panels;

use app::LlamaBurnApp;
use eframe::egui;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn main() -> eframe::Result<()> {
    // Set up file logging to /tmp/llamaburn.log
    let file_appender = tracing_appender::rolling::never("/tmp", "llamaburn.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);

    // Initialize tracing with both stdout and file output
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("llamaburn_services=debug,llamaburn_gui=info")
        }))
        .with(fmt::layer().with_writer(std::io::stdout))
        .with(fmt::layer().with_writer(file_writer).with_ansi(false))
        .init();

    tracing::info!("LlamaBurn GUI starting");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("LlamaBurn"),
        ..Default::default()
    };

    eframe::run_native(
        "LlamaBurn",
        options,
        Box::new(|cc| Ok(Box::new(LlamaBurnApp::new(cc)))),
    )
}
