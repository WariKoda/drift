use super::*;
fn parse_args(args: &[&str]) -> Result<Command> {
    invocation(args).map(|parsed| parsed.command)
}
fn invocation(args: &[&str]) -> Result<Invocation> {
    parse(args.iter().map(OsString::from).collect())
}
#[test]
fn launch_options_and_literal_directories_keep_startup_behavior() {
    assert_eq!(
        parse_args(&[]).unwrap(),
        Command::Start {
            directory: None,
            dashboard: false,
            no_dashboard: false
        }
    );
    assert_eq!(
        parse_args(&["path", "--dashboard", "--no-dashboard"]).unwrap(),
        Command::Start {
            directory: Some("path".into()),
            dashboard: true,
            no_dashboard: true
        }
    );
    assert_eq!(
        parse_args(&["dash"]).unwrap(),
        Command::Start {
            directory: None,
            dashboard: true,
            no_dashboard: false
        }
    );
    for directory in ["projects", "dash", "version", "--help", "-project"] {
        assert_eq!(
            parse_args(&["--", directory]).unwrap(),
            Command::Start {
                directory: Some(directory.into()),
                dashboard: false,
                no_dashboard: false
            }
        );
    }
    use std::os::unix::ffi::OsStringExt;
    let directory = OsString::from_vec(b"path-\xff".to_vec());
    assert_eq!(
        parse(vec![directory.clone()]).unwrap().command,
        Command::Start {
            directory: Some(directory.into()),
            dashboard: false,
            no_dashboard: false
        }
    );
}
#[test]
fn project_arguments_flags_and_help_are_scoped_to_their_commands() {
    assert_eq!(
        parse_args(&["projects", "list"]).unwrap(),
        Command::Project(ProjectCommand::List)
    );
    assert_eq!(
        parse_args(&["projects", "add", "Shop"]).unwrap(),
        Command::Project(ProjectCommand::Add {
            name: "Shop".into(),
            path: ".".into()
        })
    );
    assert_eq!(
        parse_args(&["projects", "add", "--", "-Shop", "~/src"]).unwrap(),
        Command::Project(ProjectCommand::Add {
            name: "-Shop".into(),
            path: "~/src".into()
        })
    );
    assert_eq!(
        parse_args(&[
            "projects",
            "edit",
            "--name=New Shop",
            "shop",
            "--path",
            "../src"
        ])
        .unwrap(),
        Command::Project(ProjectCommand::Edit {
            slug: "shop".into(),
            name: Some("New Shop".into()),
            path: Some("../src".into())
        })
    );
    assert_eq!(
        parse_args(&["open", "Shop"]).unwrap(),
        Command::Project(ProjectCommand::Open("Shop".into()))
    );
    assert_eq!(
        parse_args(&["projects", "archive", "shop"]).unwrap(),
        Command::Project(ProjectCommand::Archive("shop".into()))
    );
    assert_eq!(
        parse_args(&["projects", "remove", "shop"]).unwrap(),
        Command::Project(ProjectCommand::Remove("shop".into()))
    );
    assert_eq!(parse_args(&["version"]).unwrap(), Command::Version);
    for args in [
        vec!["--help"],
        vec!["projects", "--help"],
        vec!["projects", "add", "--help"],
        vec!["open", "-h"],
    ] {
        assert!(matches!(parse_args(&args).unwrap(), Command::Help(_)));
    }
}
#[test]
fn invalid_commands_fail_before_any_store_or_window_is_opened() {
    for args in [
        vec!["--unknown"],
        vec!["--dashboard=yes"],
        vec!["a", "b"],
        vec!["projects", "unknown"],
        vec!["projects", "list", "extra"],
        vec!["projects", "add"],
        vec!["projects", "add", "a", "b", "c"],
        vec!["projects", "edit"],
        vec!["projects", "edit", "shop", "--path"],
        vec!["projects", "remove", "shop", "--name", "new"],
        vec!["open"],
        vec!["open", "a", "b"],
        vec!["dash", "directory"],
        vec!["version", "extra"],
        vec!["--log"],
        vec!["--debug", "projects", "remove", "shop", "--name", "new"],
        vec!["--log=run.log", "open", "shop", "--path", "new"],
        vec!["dash", "--debug", "--dashboard"],
        vec!["version", "--log=run.log", "--no-dashboard"],
        vec!["--debug", "--path", "--log"],
    ] {
        assert!(parse_args(&args).is_err(), "{args:?}");
    }
    use std::os::unix::ffi::OsStringExt;
    assert!(
        parse(vec![
            "projects".into(),
            "add".into(),
            OsString::from_vec(vec![0xff])
        ])
        .is_err()
    );
}
#[test]
fn logging_flags_are_global_before_and_after_commands_and_positionals() {
    let cases: &[(&[&str], &[&str])] = &[
        (&["--log", "run.log", "--debug"], &[]),
        (&["--log=run.log", "--debug", "src"], &["src"]),
        (&["src", "--log", "run.log", "--debug"], &["src"]),
        (&["--debug", "--log=run.log", "dash"], &["dash"]),
        (&["dash", "--log=run.log", "--debug"], &["dash"]),
        (&["--log=run.log", "version", "--debug"], &["version"]),
        (
            &["--debug", "open", "--log", "run.log", "shop"],
            &["open", "shop"],
        ),
        (
            &["open", "shop", "--debug", "--log=run.log"],
            &["open", "shop"],
        ),
        (
            &["--log=run.log", "projects", "--debug", "list"],
            &["projects", "list"],
        ),
        (
            &[
                "projects", "add", "--debug", "Shop", "--log", "run.log", "~/src",
            ],
            &["projects", "add", "Shop", "~/src"],
        ),
        (
            &[
                "projects",
                "--debug",
                "edit",
                "--log=run.log",
                "shop",
                "--name",
                "New",
            ],
            &["projects", "edit", "shop", "--name", "New"],
        ),
        (
            &["projects", "archive", "shop", "--log=run.log", "--debug"],
            &["projects", "archive", "shop"],
        ),
        (
            &["projects", "--log=run.log", "remove", "--debug", "shop"],
            &["projects", "remove", "shop"],
        ),
    ];
    for (args, command_args) in cases {
        assert_eq!(
            invocation(args).unwrap(),
            Invocation {
                command: parse_args(command_args).unwrap(),
                logging: LogFlags {
                    path: Some("run.log".into()),
                    debug: true,
                },
            },
            "{args:?}"
        );
        assert_eq!(
            invocation(command_args).unwrap().logging,
            LogFlags::default()
        );
    }
}
#[test]
fn terminator_stops_globals_and_preserves_literal_command_directories() {
    for directory in ["projects", "open", "dash", "version", "--log", "--debug"] {
        for (prefix, logging) in [
            (vec![], LogFlags::default()),
            (
                vec!["--log=run.log", "--debug"],
                LogFlags {
                    path: Some("run.log".into()),
                    debug: true,
                },
            ),
        ] {
            let mut args = prefix;
            args.extend(["--", directory]);
            assert_eq!(
                invocation(&args).unwrap(),
                Invocation {
                    command: Command::Start {
                        directory: Some(directory.into()),
                        dashboard: false,
                        no_dashboard: false,
                    },
                    logging,
                }
            );
        }
    }
    let opened = invocation(&["open", "--", "--debug"]).unwrap();
    assert_eq!(
        opened.command,
        Command::Project(ProjectCommand::Open("--debug".into()))
    );
    assert_eq!(opened.logging, LogFlags::default());
    let added = invocation(&["projects", "add", "--", "--log", "--debug"]).unwrap();
    assert_eq!(
        added.command,
        Command::Project(ProjectCommand::Add {
            name: "--log".into(),
            path: "--debug".into(),
        })
    );
    assert_eq!(added.logging, LogFlags::default());
    for args in [
        vec!["open", "shop", "--", "--debug"],
        vec!["version", "--", "--log=run.log"],
        vec!["projects", "list", "--", "--debug"],
    ] {
        assert!(invocation(&args).is_err(), "{args:?}");
    }
}
#[test]
fn command_option_values_are_not_stolen_by_global_flags() {
    for value in ["--log", "--debug", "--log=run.log", "--debug=invalid", "--"] {
        assert_eq!(
            invocation(&[
                "projects",
                "edit",
                "shop",
                "--name",
                value,
                "--path",
                value,
                "--log=actual.log",
                "--debug"
            ])
            .unwrap(),
            Invocation {
                command: Command::Project(ProjectCommand::Edit {
                    slug: "shop".into(),
                    name: Some(value.into()),
                    path: Some(value.into()),
                }),
                logging: LogFlags {
                    path: Some("actual.log".into()),
                    debug: true,
                },
            }
        );
        let parsed = invocation(&[
            "projects",
            "edit",
            "shop",
            &format!("--name={value}"),
            &format!("--path={value}"),
        ])
        .unwrap();
        assert_eq!(
            parsed.command,
            Command::Project(ProjectCommand::Edit {
                slug: "shop".into(),
                name: Some(value.into()),
                path: Some(value.into()),
            })
        );
        assert_eq!(parsed.logging, LogFlags::default());
    }
}
#[test]
fn logging_values_and_repeated_flags_match_go() {
    for value in [
        "1", "t", "T", "TRUE", "true", "True", "0", "f", "F", "FALSE", "false", "False",
    ] {
        assert_eq!(
            invocation(&[&format!("--debug={value}")]).unwrap().logging,
            LogFlags {
                path: None,
                debug: ["1", "t", "T", "TRUE", "true", "True"].contains(&value),
            }
        );
    }
    for (args, debug) in [
        (vec!["--debug=false", "--debug"], true),
        (vec!["--debug", "--debug=false"], false),
        (vec!["--debug=0", "--debug=True", "--debug=F"], false),
    ] {
        assert_eq!(invocation(&args).unwrap().logging.debug, debug);
    }
    let repeated = invocation(&[
        "--log",
        "first.log",
        "--debug",
        "projects",
        "--log=second.log",
        "list",
        "--debug=false",
        "--log",
        "",
    ])
    .unwrap();
    assert_eq!(repeated.command, Command::Project(ProjectCommand::List));
    assert_eq!(
        repeated.logging,
        LogFlags {
            path: Some("".into()),
            debug: false
        }
    );
    for args in [vec!["--log", ""], vec!["--log="]] {
        assert_eq!(invocation(&args).unwrap().logging.path, Some("".into()));
    }
    for value in ["--debug", "--"] {
        let parsed = invocation(&["--log", value]).unwrap();
        assert_eq!(parsed.command, parse_args(&[]).unwrap());
        assert_eq!(
            parsed.logging,
            LogFlags {
                path: Some(value.into()),
                debug: false
            }
        );
    }
    let parsed = invocation(&["--debug", "false"]).unwrap();
    assert_eq!(parsed.command, parse_args(&["false"]).unwrap());
    assert!(parsed.logging.debug);
}
#[test]
fn invalid_boolean_values_and_missing_log_paths_are_rejected() {
    for value in [
        "", "yes", "no", "on", "off", "2", "TRUE ", " true", "TrUe", "Falsey",
    ] {
        for command in ["version", "--help", "dash"] {
            assert!(invocation(&[command, &format!("--debug={value}")]).is_err());
        }
    }
    for args in [
        vec!["--log"],
        vec!["projects", "list", "--log"],
        vec!["version", "--log"],
        vec!["--help", "--log"],
    ] {
        assert!(invocation(&args).is_err(), "{args:?}");
    }
    use std::os::unix::ffi::OsStringExt;
    assert!(parse(vec![OsString::from_vec(b"--debug=\xff".to_vec())]).is_err());
}
#[test]
fn log_paths_preserve_non_utf8_bytes_in_both_flag_forms() {
    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(b"log-\xff".to_vec());
    for args in [
        vec!["--log".into(), path.clone(), "version".into()],
        vec![
            "version".into(),
            OsString::from_vec(b"--log=log-\xff".to_vec()),
        ],
    ] {
        assert_eq!(
            parse(args).unwrap(),
            Invocation {
                command: Command::Version,
                logging: LogFlags {
                    path: Some(PathBuf::from(path.clone())),
                    debug: false,
                },
            }
        );
    }
}
#[test]
fn help_and_version_accept_logging_flags_without_changing_commands() {
    for (args, command) in [
        (
            vec!["--debug", "--help", "--log=run.log"],
            Command::Help(HELP),
        ),
        (
            vec!["--log=run.log", "open", "-h", "--debug"],
            Command::Help(HELP),
        ),
        (
            vec!["projects", "--debug", "--help", "--log=run.log"],
            Command::Help(PROJECT_HELP),
        ),
        (
            vec!["--debug", "projects", "add", "--log=run.log", "--help"],
            Command::Help(PROJECT_HELP),
        ),
        (
            vec!["--debug", "version", "--log=run.log"],
            Command::Version,
        ),
        (
            vec!["--log=run.log", "version", "--debug"],
            Command::Version,
        ),
    ] {
        assert_eq!(
            invocation(&args).unwrap(),
            Invocation {
                command,
                logging: LogFlags {
                    path: Some("run.log".into()),
                    debug: true
                },
            }
        );
    }
    for help in [HELP, PROJECT_HELP] {
        for control in ["--log", "--debug", "DRIFT_LOG", "DRIFT_DEBUG"] {
            assert!(help.contains(control));
        }
    }
}
