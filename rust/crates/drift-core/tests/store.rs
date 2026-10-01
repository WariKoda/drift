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
