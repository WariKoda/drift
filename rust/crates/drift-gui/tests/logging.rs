//! Logging flags are exercised in isolated, real executable processes.
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn invoke(config: &Path, cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_drift-gui"))
        .args(args)
        .current_dir(cwd)
        .env("XDG_CONFIG_HOME", config)
        .env("HOME", cwd)
        .env("DISPLAY", "")
        .env("WAYLAND_DISPLAY", "")
        .env_remove("DRIFT_LOG")
        .env_remove("DRIFT_DEBUG")
        .envs(env.iter().copied())
        .output()
        .unwrap()
}
fn ok(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn disabled_help_and_version_do_not_open_logs_or_configuration() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    for value in ["", "0", "f", "F", "false", "FALSE", "False"] {
        let output = ok(invoke(
            &config,
            root.path(),
            &["projects", "list"],
            &[("DRIFT_DEBUG", value)],
        ));
        assert!(output.stderr.is_empty());
        assert!(!config.exists());
    }
    let inaccessible = root.path().join("blocked");
    fs::write(&inaccessible, "not a directory").unwrap();
    let log = root.path().join("must-not-exist.log");
    for args in [
        vec!["--log", log.to_str().unwrap(), "--help"],
        vec!["projects", "--debug", "--help"],
        vec!["--debug", "version"],
        vec!["open", "--log", log.to_str().unwrap(), "--help"],
    ] {
        assert!(
            ok(invoke(
                &inaccessible,
                root.path(),
                &args,
                &[
                    ("DRIFT_LOG", log.to_str().unwrap()),
                    ("DRIFT_DEBUG", "true")
                ]
            ))
            .stderr
            .is_empty()
        );
        assert!(!log.exists());
    }
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn flags_environment_debug_and_append_match_go_priority() {
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let env_log = root.path().join("env.log");
    let flag_log = root.path().join("flag.log");
    let default = config.path().join("drift/drift.log");
    ok(invoke(
        config.path(),
        root.path(),
        &["projects", "list"],
        &[("DRIFT_LOG", env_log.to_str().unwrap())],
    ));
    let first = fs::read_to_string(&env_log).unwrap();
    assert!(first.contains("level=INFO"));
    assert!(!first.contains("level=DEBUG"));
    assert!(!default.exists());
    ok(invoke(
        config.path(),
        root.path(),
        &[
            "--log",
            flag_log.to_str().unwrap(),
            "projects",
            "list",
            "--debug",
        ],
        &[
            ("DRIFT_LOG", env_log.to_str().unwrap()),
            ("DRIFT_DEBUG", "false"),
        ],
    ));
    assert_eq!(fs::read_to_string(&env_log).unwrap(), first);
    assert!(
        fs::read_to_string(&flag_log)
            .unwrap()
            .contains("level=DEBUG")
    );
    ok(invoke(
        config.path(),
        root.path(),
        &["projects", "list", "--log=", "--debug=false"],
        &[
            ("DRIFT_LOG", env_log.to_str().unwrap()),
            ("DRIFT_DEBUG", "yes"),
        ],
    ));
    let appended = fs::read_to_string(&env_log).unwrap();
    assert!(appended.starts_with(&first));
    assert!(appended.contains("level=DEBUG"));
    ok(invoke(
        config.path(),
        root.path(),
        &["--debug", "projects", "list"],
        &[],
    ));
    assert!(
        fs::read_to_string(&default)
            .unwrap()
            .contains("level=DEBUG")
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn management_and_open_failures_are_logged_without_serialized_config_or_arguments() {
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let log = config.path().join("diagnostics.log");
    let path = log.to_str().unwrap();
    ok(invoke(
        config.path(),
        root.path(),
        &["projects", "add", "SECRET_PROJECT_NAME", "--log", path],
        &[],
    ));
    let registry = config.path().join("drift/projects.toml");
    let before = fs::read(&registry).unwrap();
    let output = invoke(
        config.path(),
        root.path(),
        &["--debug", "open", "absent", "--log", path],
        &[],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(&registry).unwrap(), before);
    fs::write(&registry, "projects = 'CONFIG_SECRET'").unwrap();
    assert_eq!(
        invoke(
            config.path(),
            root.path(),
            &["projects", "list", "--log", path],
            &[]
        )
        .status
        .code(),
        Some(1)
    );
    let text = fs::read_to_string(&log).unwrap();
    assert!(text.contains("project command failed"));
    assert!(text.contains("error_kind=\"config_decode\""));
    assert!(text.contains("command=\"open\""));
    assert!(!text.contains("CONFIG_SECRET"));
    assert!(!text.contains("SECRET_PROJECT_NAME"));
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn log_open_failure_warns_without_changing_the_command_result() {
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let blocked = config.path().join("blocked");
    fs::write(&blocked, "not a directory").unwrap();
    let log = blocked.join("log");
    let output = ok(invoke(
        config.path(),
        root.path(),
        &["projects", "add", "Project", "--log", log.to_str().unwrap()],
        &[],
    ));
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning: Could not open log file"));
    assert!(config.path().join("drift/projects.toml").exists());
    let failed = invoke(
        config.path(),
        root.path(),
        &["open", "absent", "--log", log.to_str().unwrap()],
        &[],
    );
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("warning: Could not open log file"));
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[cfg(target_os = "linux")]
#[test]
fn log_write_failure_warns_and_preserves_committed_mutations() {
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let output = ok(invoke(
        config.path(),
        root.path(),
        &["--log=/dev/full", "projects", "add", "Project"],
        &[],
    ));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("warning: Could not write log file /dev/full")
    );
    let store = drift_core::store::Store::new(config.path().join("drift"));
    assert!(store.registry().unwrap().find("project").is_some());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}
