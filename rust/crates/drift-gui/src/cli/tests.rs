use super::*;
fn parse_args(args: &[&str]) -> Result<Command> {
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
        parse(vec![directory.clone()]).unwrap(),
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
        vec!["--log", "file"],
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
