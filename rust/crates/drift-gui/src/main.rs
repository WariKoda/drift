mod actions;
mod browser;
mod certificates;
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
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut dashboard = false;
    let mut no_dashboard = false;
    let mut directory = None;
    let mut positional = false;
    for argument in args {
        if !positional && argument == "--" {
            positional = true;
            continue;
        }
        if !positional && argument == "--dashboard" {
            dashboard = true;
            continue;
        }
        if !positional && argument == "--no-dashboard" {
            no_dashboard = true;
            continue;
        }
        if !positional && (argument == "--help" || argument == "-h") {
            println!(
                "Usage: drift-gui [--dashboard | --no-dashboard] [directory]\n\nOutside projects, restore the last opened project or show the dashboard.\nAn explicit directory is opened directly. --no-dashboard takes precedence."
            );
            return;
        }
        if (!positional && argument.as_encoded_bytes().starts_with(b"-")) || directory.is_some() {
            eprintln!("drift-gui: unexpected argument {:?}; use --help", argument);
            std::process::exit(1);
        }
        directory = Some(std::path::PathBuf::from(argument));
    }
    let explicit_directory = directory.is_some();
    let start = directory.map_or_else(std::env::current_dir, std::path::absolute);
    let (store, service, start) = match (
        drift_core::config::config_dir(),
        drift_app::browser::BrowserService::new(),
        start,
    ) {
        (Ok(dir), Ok(service), Ok(start)) => (drift_core::store::Store::new(dir), service, start),
        (Err(error), _, _) => {
            eprintln!("drift-gui: cannot resolve configuration directory: {error}");
            std::process::exit(1);
        }
        (_, Err(error), _) => {
            eprintln!("drift-gui: cannot start background runtime: {error}");
            std::process::exit(1);
        }
        (_, _, Err(error)) => {
            eprintln!("drift-gui: cannot resolve starting directory: {error}");
            std::process::exit(1);
        }
    };
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
                        Shell::new(
                            window,
                            cx,
                            store,
                            service,
                            remote,
                            drift_app::projects::StartOptions {
                                directory: start,
                                dashboard,
                                no_dashboard,
                                explicit_directory,
                            },
                        )
                    })
                },
            ) {
                // Startup has not entered a TUI or opened a logging session.
                eprintln!("drift-gui: cannot open window: {error}");
                cx.quit();
            }
        });
}
