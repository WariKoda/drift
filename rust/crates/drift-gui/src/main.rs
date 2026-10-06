mod actions;
mod browser;
mod browser_menu;
mod certificates;
mod cli;
mod comparison;
mod diff;
#[cfg(test)]
mod finder_test_support;
mod focus_reveal;
mod form_input;
mod hosts;
mod pane_split;
mod preferences;
mod preview;
mod projects;
mod remote;
#[cfg(test)]
#[path = "../../drift-core/tests/support/mod.rs"]
mod sftp_test_support;
mod shell;
mod toolbar;

use gpui_kit::{AppContext, QuitMode, WindowOptions};
use shell::Shell;

fn main() {
    if let Err(error) = run() {
        eprintln!("drift-gui: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let invocation = cli::parse(std::env::args_os().skip(1).collect())?;
    let command = invocation.command;
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
    drop(stdout);
    use drift_app::logging::{Logger, Options};
    let config_dir = drift_core::config::config_dir()?;
    let logger = match Options::resolve(
        invocation.logging.path,
        invocation.logging.debug,
        std::env::var_os("DRIFT_LOG"),
        std::env::var_os("DRIFT_DEBUG"),
        &config_dir,
    ) {
        Some(options) => {
            let path = options.path.clone();
            match Logger::open(options) {
                Ok(logger) => logger,
                Err(error) => {
                    let warning = format!(
                        "Could not open log file {}: {error}; logging disabled",
                        path.display()
                    );
                    eprintln!("drift-gui: warning: {warning}");
                    Logger::disabled(Some(warning))
                }
            }
        }
        None => Logger::default(),
    };
    use drift_app::cli::ProjectCommand;
    let mode = match &command {
        cli::Command::Project(command) => match command {
            ProjectCommand::List => "projects list",
            ProjectCommand::Add { .. } => "projects add",
            ProjectCommand::Edit { .. } => "projects edit",
            ProjectCommand::Archive(_) => "projects archive",
            ProjectCommand::Remove(_) => "projects remove",
            ProjectCommand::Open(_) => "open",
        },
        cli::Command::Start { .. } => "gui",
        _ => unreachable!(),
    };
    logger.info(
        "drift-gui start",
        &[("version", env!("CARGO_PKG_VERSION")), ("command", mode)],
    );
    logger.debug("command dispatch", &[("command", mode)]);
    let result = run_command(
        command,
        drift_core::store::Store::new(config_dir),
        logger.clone(),
    );
    if result.is_err() {
        logger.error("command failed", &[("command", mode)]);
    }
    logger.info(
        "drift-gui exit",
        &[("outcome", if result.is_ok() { "success" } else { "failed" })],
    );
    if let Err(error) = logger.finish() {
        let warning = logger
            .failures()
            .borrow()
            .clone()
            .unwrap_or_else(|| format!("Could not finish log: {error}"));
        eprintln!("drift-gui: warning: {warning}");
    }
    result
}
fn run_command(
    command: cli::Command,
    store: drift_core::store::Store,
    logger: drift_app::logging::Logger,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    let start = match command {
        cli::Command::Project(command) => {
            let response = drift_app::cli::run(&store, command).inspect_err(|error| {
                logger.failure("project command failed", error, &[]);
            })?;
            stdout.write_all(response.output.as_bytes())?;
            if let Some(warning) = response.warning {
                logger.error("project command partial completion", &[]);
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
    let (preferences, preferences_warning) = match store.gui_preferences() {
        Ok(preferences) => (preferences, None),
        Err(error) => {
            logger.failure("GUI preferences load failed", &error, &[]);
            (Default::default(), Some("Could not load GUI preferences; using session defaults. Repair gui.toml before saving preferences.".into()))
        }
    };
    let preferences_writer =
        drift_app::gui_preferences::PreferencesWriter::open(store.clone(), logger.clone())?;
    let shutdown_preferences = preferences_writer.clone();
    let service = drift_app::browser::BrowserService::new()?.with_logger(logger.clone());
    let background = service.clone();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx| {
            gpui_kit::init(cx);
            actions::bind_keys(cx);
            let bounds = preferences::initial_window_bounds(preferences.window, cx);
            if let Err(error) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(bounds),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        let remote = drift_app::remote::RemoteService::new(service.clone());
                        Shell::new(window, cx, store, service, remote, start).with_preferences(
                            preferences,
                            preferences_writer,
                            preferences_warning,
                            window,
                            cx,
                        )
                    })
                },
            ) {
                logger.error("window open failed", &[]);
                eprintln!("drift-gui: cannot open window: {error}");
                cx.quit();
            }
        });
    background.wait_for_shutdown();
    shutdown_preferences.finish()?;
    Ok(())
}
