use drift_core::{
    config::{GlobalConfig, Host, ProjectConfig, RuntimeConfig},
    error::Error,
    store::Store,
};
use fs2::FileExt;
use std::{fs, os::unix::fs::PermissionsExt};
#[test]
fn writes_use_flock_and_stale_records_conflict_without_losing_unrelated_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let a = Host {
        name: "a".into(),
        hostname: "a.example".into(),
        ..Host::default()
    };
    store.save_host(None, None, a.clone()).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.path().join("write.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    assert!(matches!(store.host_catalog(None), Err(Error::Busy)));
    assert!(matches!(
        store.save_host(
            None,
            None,
            Host {
                name: "b".into(),
                ..Host::default()
            }
        ),
        Err(Error::Busy)
    ));
    FileExt::unlock(&lock).unwrap();
    store
        .save_host(
            None,
            None,
            Host {
                name: "b".into(),
                ..Host::default()
            },
        )
        .unwrap();
    let mut edited = a.clone();
    edited.hostname = "new.example".into();
    store.save_host(None, Some(&a), edited).unwrap();
    assert!(matches!(
        store.save_host(None, Some(&a), a.clone()),
        Err(Error::Conflict(_))
    ));
    assert_eq!(store.global().unwrap().hosts.len(), 2);
    assert_eq!(
        fs::metadata(dir.path().join("config.toml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}
#[test]
fn stored_links_and_optional_zero_remain_raw_after_runtime_resolution() {
    let global: GlobalConfig = toml::from_str("[defaults]\nport = 2222\nuser = 'default'\n[[hosts]]\nname = 'server'\nhostname = 'example'\nkeep_alive_interval = 0\n").unwrap();
    let project: ProjectConfig =
        toml::from_str("[[hosts]]\nname = 'prod'\nserver = 'server'\nroot_path = '/srv'\n")
            .unwrap();
    let runtime = RuntimeConfig::resolve(&global, Some(&project)).unwrap();
    assert_eq!(runtime.hosts[0].port, 2222);
    assert_eq!(runtime.hosts[0].keep_alive_seconds(), 0);
    let encoded = toml::to_string(&project).unwrap();
    assert!(!encoded.contains("hostname"));
    assert!(!encoded.contains("2222"));
    assert_eq!(project.hosts[0].keep_alive_interval, None);
    assert!(
        toml::to_string(&global)
            .unwrap()
            .contains("keep_alive_interval = 0")
    );
}

#[test]
fn host_catalog_keeps_raw_defaults_and_delete_guards_links_and_stale_records() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    fs::write(
        dir.path().join("config.toml"),
        "[defaults]\nport = 2222\nuser = 'deploy'\n",
    )
    .unwrap();
    let server = Host {
        name: "shared".into(),
        hostname: "example".into(),
        root_path: "/srv".into(),
        ..Host::default()
    };
    store.save_host(None, None, server.clone()).unwrap();
    let link = Host {
        name: "prod".into(),
        server: "shared".into(),
        root_path: "/project".into(),
        ..Host::default()
    };
    store.save_host(Some("one"), None, link.clone()).unwrap();
    let catalog = store.host_catalog(Some("one")).unwrap();
    assert!(catalog.hosts[0] == link);
    assert_eq!(catalog.runtime.hosts[0].port, 2222);
    assert_eq!(catalog.runtime.hosts[0].user, "deploy");
    assert!(catalog.hosts[0].hostname.is_empty());
    assert!(catalog.servers[0] == server);
    assert!(store.delete_host(None, &server).is_err());
    let renamed = Host {
        name: "other".into(),
        ..server.clone()
    };
    assert!(store.save_host(None, Some(&server), renamed).is_err());
    let changed_link = Host {
        root_path: "/changed".into(),
        ..link.clone()
    };
    store
        .save_host(Some("one"), Some(&link), changed_link.clone())
        .unwrap();
    assert!(matches!(
        store.delete_host(Some("one"), &link),
        Err(Error::Conflict(_))
    ));
    store.delete_host(Some("one"), &changed_link).unwrap();
    store.delete_host(None, &server).unwrap();
    assert!(store.host_catalog(None).unwrap().hosts.is_empty());
    assert_eq!(store.global().unwrap().defaults.port, 2222);
}
