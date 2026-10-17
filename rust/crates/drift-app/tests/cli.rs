use drift_app::cli::{ProjectCommand as Command, run};
use drift_core::{config::Host, store::Store};
use std::fs;

#[test]
fn cli_mutations_preserve_identity_settings_and_local_files() {
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let moved = tempfile::tempdir().unwrap();
    fs::write(root.path().join("keep"), "local content").unwrap();
    let store = Store::new(config.path().into());
    assert!(
        run(&store, Command::List)
            .unwrap()
            .output
            .starts_with("No projects")
    );
    assert!(!config.path().join("projects.toml").exists());
    run(
        &store,
        Command::Add {
            name: "Shop".into(),
            path: root.path().to_str().unwrap().into(),
        },
    )
    .unwrap();
    store.mark_opened("shop").unwrap();
    store
        .save_host(
            Some("shop"),
            None,
            Host {
                name: "prod".into(),
                hostname: "example.test".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let before = store.registry().unwrap().find("shop").unwrap().clone();
    let settings = fs::read(config.path().join("projects/shop.toml")).unwrap();
    run(
        &store,
        Command::Edit {
            slug: "shop".into(),
            name: Some("New Shop".into()),
            path: Some(moved.path().to_str().unwrap().into()),
        },
    )
    .unwrap();
    let project = store.registry().unwrap().find("shop").unwrap().clone();
    assert_eq!(project.name, "New Shop");
    assert_eq!(project.path, moved.path());
    assert!(project.created_at == before.created_at && project.opened_at == before.opened_at);
    for archived in [true, false] {
        run(&store, Command::Archive("shop".into())).unwrap();
        assert_eq!(
            store.registry().unwrap().find("shop").unwrap().archived,
            archived
        );
        assert_eq!(
            fs::read(config.path().join("projects/shop.toml")).unwrap(),
            settings
        );
    }
    run(&store, Command::Remove("shop".into())).unwrap();
    assert!(store.registry().unwrap().projects.is_empty());
    assert!(!config.path().join("projects/shop.toml").exists());
    assert_eq!(
        fs::read(root.path().join("keep")).unwrap(),
        b"local content"
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(moved.path()).unwrap().count(), 0);
}
#[test]
fn open_matches_names_prefixes_and_substrings_without_marking_or_writing() {
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store
        .register("Shop Production", root.path().into())
        .unwrap();
    let before = fs::read(config.path().join("projects.toml")).unwrap();
    for query in ["shop-production", "SHOP PRODUCTION", "Shop P", "roduction"] {
        let response = run(&store, Command::Open(query.into())).unwrap();
        let start = response.start.unwrap();
        assert_eq!(start.directory, root.path());
        assert!(start.explicit_directory && start.no_dashboard && !start.dashboard);
        assert_eq!(
            fs::read(config.path().join("projects.toml")).unwrap(),
            before
        );
    }
    fs::write(config.path().join("config.toml"), "broken = [").unwrap();
    assert!(run(&store, Command::Open("shop-production".into())).is_err());
    assert_eq!(
        fs::read(config.path().join("projects.toml")).unwrap(),
        before
    );
    fs::remove_file(config.path().join("config.toml")).unwrap();
    let other = tempfile::tempdir().unwrap();
    store.register("Shop Preview", other.path().into()).unwrap();
    assert!(
        run(&store, Command::Open("Shop".into()))
            .err()
            .unwrap()
            .to_string()
            .contains("ambiguous")
    );
}
#[test]
fn list_status_and_errors_do_not_mutate_the_registry() {
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let gone = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.register("Active", root.path().into()).unwrap();
    let archived = store
        .register("Archived", root.path().join("archived"))
        .unwrap();
    fs::create_dir(&archived.path).unwrap();
    store.archive_project(&archived).unwrap();
    store.register("Missing", gone.path().into()).unwrap();
    gone.close().unwrap();
    let before = fs::read(config.path().join("projects.toml")).unwrap();
    let output = run(&store, Command::List).unwrap().output;
    let statuses: Vec<_> = output
        .lines()
        .skip(1)
        .map(|line| line.split('\t').next_back().unwrap())
        .collect();
    assert_eq!(statuses, ["active", "archived", "missing"]);
    for command in [
        Command::Open("missing".into()),
        Command::Open("absent".into()),
        Command::Archive("absent".into()),
        Command::Remove("absent".into()),
        Command::Edit {
            slug: "active".into(),
            name: Some(String::new()),
            path: None,
        },
        Command::Add {
            name: "Duplicate path".into(),
            path: root.path().to_str().unwrap().into(),
        },
    ] {
        assert!(run(&store, command).is_err());
        assert_eq!(
            fs::read(config.path().join("projects.toml")).unwrap(),
            before
        );
    }
}
