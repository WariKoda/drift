use drift_core::{
    config::{Auth, Host},
    error::Error,
    pathmap::Mapping,
    store::Store,
};
use fs2::FileExt;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn link_targets_are_sorted_resolved_and_include_users_outside_the_registry() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    fs::write(dir.path().join("config.toml"), "[defaults]\nport=2222\nuser='global'\n[[hosts]]\nname='z'\nhostname='z.example'\n[[hosts]]\nname='a'\nhostname='a.example'\n").unwrap();
    store
        .save_host(
            Some("shop-b"),
            None,
            Host {
                name: "own".into(),
                hostname: "own.example".into(),
                ..Host::default()
            },
        )
        .unwrap();
    store
        .save_host(
            Some("shop-a"),
            None,
            Host {
                name: "linked".into(),
                server: "a".into(),
                root_path: "/a".into(),
                ..Host::default()
            },
        )
        .unwrap();
    let path = dir.path().join("projects/shop-a.toml");
    let mut project = store.project("shop-a").unwrap();
    project.defaults.user = "web".into();
    project.hosts.push(Host {
        name: "stage".into(),
        hostname: "stage.example".into(),
        protocol: "ftps".into(),
        root_path: "/stage".into(),
        ..Host::default()
    });
    fs::write(&path, toml::to_string(&project).unwrap()).unwrap();
    fs::write(dir.path().join("projects/.broken.toml"), "invalid [").unwrap();
    fs::create_dir(dir.path().join("projects/directory.toml")).unwrap();
    let catalog = store.link_targets("shop-b").unwrap();
    assert_eq!(
        catalog
            .targets
            .iter()
            .map(|t| (t.project.as_deref().unwrap_or(""), t.host.name.as_str()))
            .collect::<Vec<_>>(),
        vec![("", "a"), ("", "z"), ("shop-a", "stage")]
    );
    assert_eq!(catalog.targets[0].used_by, vec!["shop-a"]);
    assert_eq!(catalog.targets[0].host.port, 2222);
    assert_eq!(catalog.targets[2].host.port, 21);
    assert_eq!(catalog.targets[2].host.user, "web");
    let server = store
        .select_link_target("shop-b", &catalog.targets[0])
        .unwrap();
    assert_eq!(server.server.name, "a");
    assert!(server.warning.is_none());
    fs::write(dir.path().join("projects/corrupt.toml"), "invalid [").unwrap();
    assert!(store.link_targets("shop-b").is_err());
    assert!(
        store
            .delete_host(None, &store.global().unwrap().hosts[0])
            .is_err()
    );
}

#[test]
fn promotion_materializes_source_defaults_keeps_source_scope_and_avoids_name_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    fs::write(
        dir.path().join("config.toml"),
        "[defaults]\nport=2222\nuser='global'\n",
    )
    .unwrap();
    for name in ["stage", "source-stage", "source-stage-2"] {
        store
            .save_host(
                None,
                None,
                Host {
                    name: name.into(),
                    hostname: "elsewhere".into(),
                    ..Host::default()
                },
            )
            .unwrap();
    }
    let host = Host {
        name: "stage".into(),
        hostname: "stage.example".into(),
        root_path: "/stage".into(),
        keep_alive_interval: Some(0),
        auth: Auth {
            kind: "password".into(),
            password: "test-secret".into(),
            ..Auth::default()
        },
        mappings: vec![Mapping {
            local: "src".into(),
            remote: "app".into(),
        }],
        ..Host::default()
    };
    store.save_host(Some("source"), None, host.clone()).unwrap();
    let mut source = store.project("source").unwrap();
    source.defaults.user = "web".into();
    source.mappings = vec![Mapping {
        local: "fallback".into(),
        remote: "fallback".into(),
    }];
    fs::write(
        dir.path().join("projects/source.toml"),
        toml::to_string(&source).unwrap(),
    )
    .unwrap();
    let target = store
        .link_targets("dest")
        .unwrap()
        .targets
        .into_iter()
        .find(|t| t.project.as_deref() == Some("source"))
        .unwrap();
    // An unrelated host added after opening the picker must survive promotion.
    store
        .save_host(
            Some("source"),
            None,
            Host {
                name: "unrelated".into(),
                hostname: "other".into(),
                ..Host::default()
            },
        )
        .unwrap();
    let promoted = store.select_link_target("dest", &target).unwrap();
    assert!(promoted.warning.is_none());
    assert_eq!(promoted.server.name, "source-stage-3");
    assert_eq!(promoted.server.user, "web");
    assert_eq!(promoted.server.port, 22);
    assert_eq!(promoted.server.keep_alive_interval, Some(0));
    assert!(promoted.server.auth == host.auth);
    assert!(promoted.server.mappings.is_empty());
    let source = store.project("source").unwrap();
    let link = source.hosts.iter().find(|h| h.name == "stage").unwrap();
    assert_eq!(link.server, "source-stage-3");
    assert!(link.hostname.is_empty() && link.user.is_empty() && link.protocol.is_empty());
    assert_eq!(link.keep_alive_interval, None);
    assert!(link.auth == Auth::default());
    assert_eq!(link.mappings, host.mappings);
    assert_eq!(link.root_path, host.root_path);
    assert_eq!(source.hosts.len(), 2);
    assert_eq!(source.mappings[0].local, "fallback");
    assert!(
        store
            .runtime(Some("source"))
            .unwrap()
            .hosts
            .iter()
            .find(|h| h.name == "stage")
            .unwrap()
            == &Host {
                server: "source-stage-3".into(),
                port: 22,
                user: "web".into(),
                ..host
            }
    );
    assert!(!dir.path().join("projects/dest.toml").exists());
    assert!(matches!(
        store.select_link_target("dest", &target),
        Err(Error::Conflict(_))
    ));
    assert!(
        store
            .delete_host(None, &store.global().unwrap().hosts[3])
            .is_err()
    );
    for path in [
        dir.path().join("config.toml"),
        dir.path().join("projects/source.toml"),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn promotion_rejects_stale_hosts_defaults_and_locks_before_any_write() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let host = Host {
        name: "stage".into(),
        hostname: "first.example".into(),
        ..Host::default()
    };
    store.save_host(Some("source"), None, host.clone()).unwrap();
    let target = store.link_targets("dest").unwrap().targets.remove(0);
    assert!(store.select_link_target("source", &target).is_err());
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(dir.path().join("write.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    assert!(matches!(store.link_targets("dest"), Err(Error::Busy)));
    assert!(matches!(
        store.select_link_target("dest", &target),
        Err(Error::Busy)
    ));
    lock.unlock().unwrap();
    let mut changed = host.clone();
    changed.hostname = "second.example".into();
    store
        .save_host(Some("source"), Some(&host), changed)
        .unwrap();
    assert!(matches!(
        store.select_link_target("dest", &target),
        Err(Error::Conflict(_))
    ));
    assert!(!dir.path().join("config.toml").exists());
    let target = store.link_targets("dest").unwrap().targets.remove(0);
    let mut source = store.project("source").unwrap();
    source.defaults.port = 2200;
    fs::write(
        dir.path().join("projects/source.toml"),
        toml::to_string(&source).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        store.select_link_target("dest", &target),
        Err(Error::Conflict(_))
    ));
    assert!(!dir.path().join("config.toml").exists());
}

#[test]
fn failed_source_write_keeps_a_valid_global_copy_and_reports_partial_completion() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let host = Host {
        name: "stage".into(),
        hostname: "stage.example".into(),
        root_path: "/stage".into(),
        ..Host::default()
    };
    store.save_host(Some("source"), None, host.clone()).unwrap();
    let target = store.link_targets("dest").unwrap().targets.remove(0);
    let projects = dir.path().join("projects");
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o500)).unwrap();
    let promoted = store.select_link_target("dest", &target);
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o700)).unwrap();
    let promoted = promoted.unwrap();
    assert!(
        promoted
            .warning
            .as_ref()
            .unwrap()
            .contains("keeps its own copy")
    );
    assert_eq!(promoted.server.name, "stage");
    assert!(store.project("source").unwrap().hosts[0] == host);
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    assert!(store.runtime(Some("source")).is_ok());
    assert!(!dir.path().join("projects/dest.toml").exists());
}
