use drift_core::{
    config::Host,
    error::Error,
    project::{expand_path, git_root},
    store::Store,
};
use fs2::FileExt;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn edits_and_archive_keep_identity_settings_and_unrelated_registry_changes() {
    let config = tempfile::tempdir().unwrap();
    let tree = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let first = store.register(" Shop ", tree.path().join("shop")).unwrap();
    assert_eq!(first.name, "Shop");
    store
        .save_host(
            Some(&first.slug),
            None,
            Host {
                name: "prod".into(),
                hostname: "prod.example".into(),
                ..Host::default()
            },
        )
        .unwrap();
    let settings = fs::read(config.path().join("projects/shop.toml")).unwrap();
    let other = store.register("Other", tree.path().join("other")).unwrap();
    let moved = tree.path().join("moved");
    let registry = store
        .edit_project(&first, " Renamed ", moved.to_str().unwrap())
        .unwrap();
    let edited = registry.find("shop").unwrap();
    assert_eq!(edited.name, "Renamed");
    assert_eq!(edited.path, moved);
    assert!(
        edited.created_at == first.created_at
            && edited.opened_at == first.opened_at
            && edited.updated_at != first.updated_at
    );
    assert!(registry.find(&other.slug) == Some(&other));
    assert_eq!(
        fs::read(config.path().join("projects/shop.toml")).unwrap(),
        settings
    );
    let registry = store.archive_project(edited).unwrap();
    let archived = registry.find("shop").unwrap();
    assert!(archived.archived && registry.active().iter().all(|p| p.slug != "shop"));
    assert_eq!(registry.all().len(), 2);
    let restored = store.archive_project(archived).unwrap();
    assert!(!restored.find("shop").unwrap().archived);
    assert_eq!(
        fs::read(config.path().join("projects/shop.toml")).unwrap(),
        settings
    );
    assert!(tree.path().read_dir().unwrap().next().is_none());
}

#[test]
fn stale_project_edits_and_removals_never_change_settings_or_registry() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let first = store.register("Shop", "/work/shop".into()).unwrap();
    store
        .save_host(
            Some("shop"),
            None,
            Host {
                name: "prod".into(),
                hostname: "prod.example".into(),
                ..Host::default()
            },
        )
        .unwrap();
    store.mark_opened("shop").unwrap();
    let registry = fs::read(dir.path().join("projects.toml")).unwrap();
    let settings = fs::read(dir.path().join("projects/shop.toml")).unwrap();
    assert!(matches!(
        store.edit_project(&first, "Old", "/work/shop"),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        store.archive_project(&first),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        store.remove_project(&first),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        fs::read(dir.path().join("projects.toml")).unwrap(),
        registry
    );
    assert_eq!(
        fs::read(dir.path().join("projects/shop.toml")).unwrap(),
        settings
    );
    let current = store.registry().unwrap().find("shop").unwrap().clone();
    store.register("Other", "/work/other".into()).unwrap();
    assert!(
        store
            .edit_project(&current, "Shop", "/work/other/../other")
            .is_err()
    );
    let lock = fs::File::open(dir.path().join("write.lock")).unwrap();
    lock.lock_exclusive().unwrap();
    assert!(matches!(store.remove_project(&current), Err(Error::Busy)));
    FileExt::unlock(&lock).unwrap();
}

#[test]
fn deletion_removes_only_settings_and_registry_and_works_without_settings() {
    for settings in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("keep.txt"), "local data").unwrap();
        let store = Store::new(dir.path().into());
        let project = store.register("Shop", root.path().into()).unwrap();
        if settings {
            store
                .save_host(
                    Some("shop"),
                    None,
                    Host {
                        name: "prod".into(),
                        hostname: "prod.example".into(),
                        ..Host::default()
                    },
                )
                .unwrap();
        }
        let other = store.register("Other", "/work/other".into()).unwrap();
        let result = store.remove_project(&project).unwrap();
        assert!(result.warning.is_none());
        assert!(result.registry.find("shop").is_none());
        assert!(result.registry.find("other") == Some(&other));
        assert!(!dir.path().join("projects/shop.toml").exists());
        assert_eq!(
            fs::read_to_string(root.path().join("keep.txt")).unwrap(),
            "local data"
        );
        assert_eq!(
            fs::metadata(dir.path().join("projects.toml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn failed_registry_commit_restores_the_hidden_project_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let project = store.register("Shop", "/work/shop".into()).unwrap();
    store
        .save_host(
            Some("shop"),
            None,
            Host {
                name: "prod".into(),
                hostname: "prod.example".into(),
                ..Host::default()
            },
        )
        .unwrap();
    let settings = fs::read(dir.path().join("projects/shop.toml")).unwrap();
    let registry = fs::read(dir.path().join("projects.toml")).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let result = store.remove_project(&project);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(
        fs::read(dir.path().join("projects/shop.toml")).unwrap(),
        settings
    );
    assert_eq!(
        fs::read(dir.path().join("projects.toml")).unwrap(),
        registry
    );
    assert_eq!(
        fs::read_dir(dir.path().join("projects")).unwrap().count(),
        1
    );
}

#[test]
fn lexical_paths_and_git_worktree_roots_match_registration_rules() {
    let tree = tempfile::tempdir().unwrap();
    let nested = tree.path().join("a/b");
    fs::create_dir_all(&nested).unwrap();
    fs::write(tree.path().join(".git"), "gitdir: elsewhere").unwrap();
    assert_eq!(git_root(&nested).unwrap().unwrap(), tree.path());
    assert_eq!(
        expand_path(&format!(" {}/a/../missing/./ ", tree.path().display())).unwrap(),
        tree.path().join("missing")
    );
    assert_eq!(
        expand_path("/../work").unwrap(),
        std::path::Path::new("/work")
    );
    assert!(expand_path("  ").is_err());
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let outer = store.register("Outer", tree.path().into()).unwrap();
    let mut raw = outer.clone();
    raw.path = tree
        .path()
        .join("long-component-in-a-manually-edited-path/..");
    store.save_project(Some(&outer), raw).unwrap();
    let inner = store.register("Inner", tree.path().join("a")).unwrap();
    assert_eq!(
        store
            .registry()
            .unwrap()
            .find_by_path(&nested)
            .unwrap()
            .slug,
        inner.slug
    );
}
