use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn options_match_go_path_precedence_and_truthy_environment() {
    let config = Path::new("/config/drift");
    for value in [
        None,
        Some(""),
        Some("0"),
        Some("f"),
        Some("F"),
        Some("false"),
        Some("FALSE"),
        Some("False"),
    ] {
        assert_eq!(
            Options::resolve(None, false, None, value.map(Into::into), config),
            None
        );
    }
    for value in [
        "1",
        "t",
        "T",
        "true",
        "TRUE",
        "True",
        "yes",
        "false ",
        "debug.log",
    ] {
        assert_eq!(
            Options::resolve(None, false, None, Some(value.into()), config),
            Some(Options {
                path: config.join("drift.log"),
                debug: true,
            })
        );
    }
    assert_eq!(
        Options::resolve(
            Some("flag.log".into()),
            false,
            Some("env.log".into()),
            Some("true".into()),
            config
        ),
        Some(Options {
            path: "flag.log".into(),
            debug: true,
        })
    );
    assert_eq!(
        Options::resolve(
            Some("".into()),
            false,
            Some("env.log".into()),
            Some("false".into()),
            config
        ),
        Some(Options {
            path: "env.log".into(),
            debug: false,
        })
    );
    assert_eq!(
        Options::resolve(None, true, Some("".into()), Some("false".into()), config),
        Some(Options {
            path: config.join("drift.log"),
            debug: true,
        })
    );
    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(b"log-\xff".to_vec());
    assert_eq!(
        Options::resolve(None, false, Some(path.clone()), None, config)
            .unwrap()
            .path,
        PathBuf::from(path)
    );
}

#[test]
fn appending_private_files_drains_concurrent_records_and_filters_debug() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("nested/diagnostics.log");
    let logger = Logger::open(Options {
        path: path.clone(),
        debug: false,
    })
    .unwrap();
    logger.debug("hidden debug", &[]);
    logger.info("start", &[("path", "remote\n\t\"filename")]);
    let workers: Vec<_> = (0..4)
        .map(|worker| {
            let logger = logger.clone();
            std::thread::spawn(move || {
                for operation in 0..50 {
                    logger.info(
                        "concurrent",
                        &[
                            ("worker", &worker.to_string()),
                            ("operation", &operation.to_string()),
                        ],
                    );
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    logger.finish().unwrap();
    assert!(logger.failures().borrow().is_none());
    let first = fs::read_to_string(&path).unwrap();
    assert_eq!(first.lines().count(), 201);
    assert!(!first.contains("hidden debug"));
    assert!(first.contains("path=\"remote\\n\\t\\\"filename\""));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let logger = Logger::open(Options {
        path: path.clone(),
        debug: true,
    })
    .unwrap();
    logger.debug("visible debug", &[]);
    logger.finish().unwrap();
    let next = fs::read_to_string(&path).unwrap();
    assert!(next.starts_with(&first));
    assert!(next.contains("level=DEBUG msg=\"visible debug\""));
    logger.info("after shutdown", &[]);
    logger.finish().unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), next);
}

#[test]
fn raw_errors_and_malformed_configuration_are_not_written_to_diagnostics() {
    let root = tempfile::tempdir().unwrap();
    let store = drift_core::store::Store::new(root.path().join("config"));
    fs::create_dir_all(store.dir()).unwrap();
    fs::write(
        store.dir().join("config.toml"),
        "[[hosts]]\nname='host'\nhostname='example'\nport='CONFIG_SECRET'\n",
    )
    .unwrap();
    let error = store.runtime(None).err().unwrap();
    let path = root.path().join("diagnostics.log");
    let logger = Logger::open(Options {
        path: path.clone(),
        debug: true,
    })
    .unwrap();
    logger.failure("configuration failed", &error, &[]);
    logger.failure(
        "untrusted reply",
        &Error::Invalid("SERVER_SECRET".into()),
        &[],
    );
    logger.failure(
        "untrusted connection",
        &Error::Connection("AUTH_SECRET".into()),
        &[],
    );
    let missing = fs::read(root.path().join("missing")).unwrap_err();
    logger.failure("file failed", &Error::Io(missing), &[]);
    logger.finish().unwrap();
    let log = fs::read_to_string(path).unwrap();
    for secret in ["CONFIG_SECRET", "SERVER_SECRET", "AUTH_SECRET"] {
        assert!(!log.contains(secret));
    }
    for kind in ["config_decode", "invalid", "connection", "io"] {
        assert!(log.contains(&format!("error_kind={kind:?}")));
    }
    assert!(log.contains("io_kind=\"NotFound\""));
    assert!(log.contains("os_error="));
}

#[test]
fn disabled_logging_opens_nothing_and_open_failures_are_returned() {
    let root = tempfile::tempdir().unwrap();
    let logger = Logger::default();
    logger.info("disabled", &[]);
    logger.failure("disabled failure", &Error::Busy, &[]);
    logger.finish().unwrap();
    assert!(logger.failures().borrow().is_none());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    fs::write(root.path().join("blocked"), "not a directory").unwrap();
    assert!(
        Logger::open(Options {
            path: root.path().join("blocked/log"),
            debug: false
        })
        .is_err()
    );
    let warning = Logger::disabled(Some("Could not open log".into()));
    assert_eq!(
        warning.failures().borrow().as_deref(),
        Some("Could not open log")
    );
    let fifo = root.path().join("fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Logger::open(Options {
            path: fifo,
            debug: false
        })
        .is_err()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn real_write_failure_is_retained_for_late_gui_subscribers_and_shutdown() {
    let logger = Logger::open(Options {
        path: "/dev/full".into(),
        debug: false,
    })
    .unwrap();
    logger.info("write fails", &[]);
    assert!(logger.finish().is_err());
    let warning = logger.failures().borrow().clone().unwrap();
    assert!(warning.contains("Could not write log file /dev/full"));
    logger.error("no retries", &[]);
    assert_eq!(logger.failures().borrow().as_ref(), Some(&warning));
}
