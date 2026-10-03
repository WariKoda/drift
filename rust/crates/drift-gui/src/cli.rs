//! Argument handling is independent of GPUI initialization and display access.
use drift_app::cli::ProjectCommand;
use drift_core::error::{Error, Result};
use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

pub const HELP: &str = "Usage: drift-gui [--dashboard | --no-dashboard] [directory]
       drift-gui dash
       drift-gui open <name-or-slug>
       drift-gui projects <list|add|edit|archive|remove>
       drift-gui version

Outside projects, restore the last opened project or show the dashboard.
An explicit directory is opened directly. --no-dashboard takes precedence.
Use -- before a directory beginning with '-'; project names matching commands
can be opened with drift-gui ./directory or drift-gui open <name-or-slug>.
Use drift-gui projects --help for management commands.

Global logging flags (before or after commands and arguments):
  --log <path>, --log=<path>  Write logs to a file (empty falls back to DRIFT_LOG).
  --debug[=<bool>]           Enable debug logging; uses the default log file if needed.
  Bool accepts 1,t,T,TRUE,true,True or 0,f,F,FALSE,false,False.
  -- terminates all flag recognition.
Environment: DRIFT_LOG sets the log file; DRIFT_DEBUG enables debug logging.
Logging is off by default. --debug=false does not override a true DRIFT_DEBUG.";
const PROJECT_HELP: &str = "Usage: drift-gui projects list
       drift-gui projects add <name> [path]
       drift-gui projects edit <slug> [--name <name>] [--path <path>]
       drift-gui projects archive <slug>
       drift-gui projects remove <slug>

Add defaults to the current directory; paths support ~ and ~/.
Archive toggles archived/active. Remove deletes registry/settings, keeping local files.
Stores are shared with drift and the GUI; writes use the common transaction lock.
Use -- before positional arguments beginning with '-'.
Global flags: --log <path> or --log=<path>, --debug or --debug=<bool>.
Environment: DRIFT_LOG sets the log file; DRIFT_DEBUG enables debug logging.
Use drift-gui --help for logging details.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Start {
        directory: Option<PathBuf>,
        dashboard: bool,
        no_dashboard: bool,
    },
    Project(ProjectCommand),
    Help(&'static str),
    Version,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Invocation {
    pub command: Command,
    pub logging: LogFlags,
}
#[derive(Default, Debug, PartialEq, Eq)]
pub struct LogFlags {
    pub path: Option<PathBuf>,
    pub debug: bool,
}
#[derive(Default)]
struct Arguments {
    positional: Vec<OsString>,
    name: Option<String>,
    path: Option<String>,
    dashboard: bool,
    no_dashboard: bool,
    help: bool,
}
// Only options allowed in the current command may consume an argument.
fn arguments(args: Vec<OsString>, allowed: &[&str]) -> Result<Arguments> {
    let mut result = Arguments::default();
    let mut positional = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if positional {
            result.positional.push(arg);
            continue;
        }
        if arg == "--" {
            positional = true;
            continue;
        }
        if arg == "--help" || arg == "-h" {
            result.help = true;
            continue;
        }
        let option = arg.to_str().and_then(|s| s.split('=').next());
        if let Some(option) = option.filter(|s| allowed.contains(s)) {
            match option {
                "--dashboard" | "--no-dashboard" => {
                    if arg.to_string_lossy().contains('=') {
                        return Err(Error::Invalid(format!("{option} takes no value")));
                    }
                    if option == "--dashboard" {
                        result.dashboard = true;
                    } else {
                        result.no_dashboard = true;
                    }
                }
                "--name" | "--path" => {
                    let value = if let Some((_, value)) = arg.to_str().unwrap().split_once('=') {
                        value.to_owned()
                    } else {
                        text(args.next().ok_or_else(|| {
                            Error::Invalid(format!("missing value for {option}"))
                        })?)?
                    };
                    if option == "--name" {
                        result.name = Some(value);
                    } else {
                        result.path = Some(value);
                    }
                }
                _ => unreachable!(),
            }
        } else if arg.as_encoded_bytes().starts_with(b"-") {
            return Err(Error::Invalid(format!(
                "unexpected option {arg:?}; use --help"
            )));
        } else {
            result.positional.push(arg);
        }
    }
    Ok(result)
}
fn text(value: OsString) -> Result<String> {
    value
        .into_string()
        .map_err(|_| Error::Invalid("project arguments must be UTF-8".into()))
}
pub fn parse(args: Vec<OsString>) -> Result<Invocation> {
    let mut logging = LogFlags::default();
    let mut command_args = Vec::with_capacity(args.len());
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            command_args.push(arg);
            command_args.extend(args);
            break;
        }
        if arg == "--name" || arg == "--path" {
            // Command option values are literal, even when they look like global flags.
            command_args.push(arg);
            if let Some(value) = args.next() {
                command_args.push(value);
            }
        } else if arg == "--log" {
            logging.path =
                Some(PathBuf::from(args.next().ok_or_else(|| {
                    Error::Invalid("missing value for --log".into())
                })?));
        } else if let Some(value) = arg.as_bytes().strip_prefix(b"--log=") {
            logging.path = Some(PathBuf::from(OsStr::from_bytes(value)));
        } else if arg == "--debug" {
            logging.debug = true;
        } else if let Some(value) = arg.as_encoded_bytes().strip_prefix(b"--debug=") {
            logging.debug = match value {
                b"1" | b"t" | b"T" | b"TRUE" | b"true" | b"True" => true,
                b"0" | b"f" | b"F" | b"FALSE" | b"false" | b"False" => false,
                _ => {
                    return Err(Error::Invalid(format!(
                        "invalid boolean value for --debug: {arg:?}"
                    )));
                }
            };
        } else {
            command_args.push(arg);
        }
    }
    Ok(Invocation {
        command: parse_command(command_args)?,
        logging,
    })
}
fn parse_command(args: Vec<OsString>) -> Result<Command> {
    let first = args.first().and_then(|arg| arg.to_str());
    match first {
        Some("projects") => {
            let subcommand = args.get(1).and_then(|a| a.to_str());
            let options = if subcommand == Some("edit") {
                &["--name", "--path"][..]
            } else {
                &[]
            };
            let mut parsed = arguments(args.into_iter().skip(1).collect(), options)?;
            if parsed.help || parsed.positional.is_empty() {
                return Ok(Command::Help(PROJECT_HELP));
            }
            let subcommand = text(parsed.positional.remove(0))?;
            let count = parsed.positional.len();
            let valid = match subcommand.as_str() {
                "list" => count == 0,
                "add" => (1..=2).contains(&count),
                "edit" | "archive" | "remove" => count == 1,
                _ => false,
            };
            if !valid {
                return Err(Error::Invalid(
                    "invalid project command or argument count; use projects --help".into(),
                ));
            }
            let mut positional = parsed.positional.into_iter();
            let command = match subcommand.as_str() {
                "list" => ProjectCommand::List,
                "add" => ProjectCommand::Add {
                    name: text(positional.next().unwrap())?,
                    path: text(positional.next().unwrap_or_else(|| ".".into()))?,
                },
                "edit" => ProjectCommand::Edit {
                    slug: text(positional.next().unwrap())?,
                    name: parsed.name,
                    path: parsed.path,
                },
                "archive" => ProjectCommand::Archive(text(positional.next().unwrap())?),
                "remove" => ProjectCommand::Remove(text(positional.next().unwrap())?),
                _ => unreachable!(),
            };
            Ok(Command::Project(command))
        }
        Some("open" | "dash" | "version") => {
            let name = first.unwrap().to_owned();
            let parsed = arguments(args.into_iter().skip(1).collect(), &[])?;
            if parsed.help {
                return Ok(Command::Help(HELP));
            }
            if parsed.positional.len() != usize::from(name == "open") {
                return Err(Error::Invalid(format!(
                    "invalid arguments for {name}; use --help"
                )));
            }
            Ok(match name.as_str() {
                "open" => Command::Project(ProjectCommand::Open(text(
                    parsed.positional.into_iter().next().unwrap(),
                )?)),
                "dash" => Command::Start {
                    directory: None,
                    dashboard: true,
                    no_dashboard: false,
                },
                "version" => Command::Version,
                _ => unreachable!(),
            })
        }
        _ => {
            let parsed = arguments(args, &["--dashboard", "--no-dashboard"])?;
            if parsed.help {
                return Ok(Command::Help(HELP));
            }
            if parsed.positional.len() > 1 {
                return Err(Error::Invalid(
                    "expected at most one directory; use --help".into(),
                ));
            }
            Ok(Command::Start {
                directory: parsed.positional.into_iter().next().map(PathBuf::from),
                dashboard: parsed.dashboard,
                no_dashboard: parsed.no_dashboard,
            })
        }
    }
}

#[cfg(test)]
mod tests;
