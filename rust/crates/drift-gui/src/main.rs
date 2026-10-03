mod actions;
mod browser;
mod certificates;
mod cli;
mod comparison;
mod diff;
mod hosts;
mod preview;
mod projects;
mod remote;
#[cfg(test)]
#[path = "../../drift-core/tests/support/mod.rs"]
mod sftp_test_support;
mod shell;
mod toolbar;

use gpui_kit::{AppContext, Bounds, QuitMode, WindowBounds, WindowOptions, px, size};
use shell::Shell;

fn main() {
    if let Err(error) = run() {
        eprintln!("drift-gui: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let command = cli::parse(std::env::args_os().skip(1).collect())?;
    let mut stdout = std::io::stdout().lock();
    match &command {
        cli::Command::Help(help) => {
            writeln!(stdout, "{help}")?;
            return Ok(());
        }
        cli::Command::Version => {
            writeln!(stdout, "drift-gui {}", env!("CARGO_PKG_VERSION"))?;
            return Ok(());
        }
        _ => {}
    }
    let store = drift_core::store::Store::new(drift_core::config::config_dir()?);
    let start = match command {
        cli::Command::Project(command) => {
            let response = drift_app::cli::run(&store, command)?;
            stdout.write_all(response.output.as_bytes())?;
            if let Some(warning) = response.warning {
                writeln!(std::io::stderr().lock(), "drift-gui: warning: {warning}")?;
            }
            let Some(start) = response.start else {
                return Ok(());
            };
            start
        }
        cli::Command::Start {
            directory,
            dashboard,
            no_dashboard,
        } => {
            let explicit_directory = directory.is_some();
            drift_app::projects::StartOptions {
                directory: directory.map_or_else(std::env::current_dir, std::path::absolute)?,
                dashboard,
                no_dashboard,
                explicit_directory,
            }
        }
        _ => unreachable!("handled before configuration lookup"),
    };
    drop(stdout);
    let service = drift_app::browser::BrowserService::new()?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx| {
            gpui_kit::init(cx);
            actions::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
            if let Err(error) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        let remote = drift_app::remote::RemoteService::new(service.clone());
                        Shell::new(window, cx, store, service, remote, start)
                    })
                },
            ) {
                // Startup has not entered a TUI or opened a logging session.
                eprintln!("drift-gui: cannot open window: {error}");
                cx.quit();
            }
        });
    Ok(())
}
