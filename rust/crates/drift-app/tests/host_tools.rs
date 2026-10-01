#[path = "../../drift-core/tests/support/mod.rs"]
mod support;
use drift_app::{
    browser::{BrowserService, OperationId},
    remote::RemoteService,
};
use drift_core::{
    config::Host, error::Error, remote::ConnectionState, store::Store, tlstrust::Manager,
};
use std::{fs, sync::Arc, time::Duration};
fn id(operation: u64) -> OperationId {
    OperationId {
        project: 1,
        operation,
    }
}

#[tokio::test]
async fn unsaved_host_tests_resolve_fresh_defaults_and_links_without_changing_active_sessions() {
    let sftp = support::Server::new(false);
    let ftp = support::ftp::Server::new(8);
    let ftps = support::ftp::Server::tls_with_limit("valid", 8);
    for (host, options, roots) in [
        (sftp.host(), sftp.options(), None),
        (ftp.host(), ftp.options(), None),
        (ftps.host(), ftps.options(), Some(ftps.roots())),
    ] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::new(home.path().join("config"));
        fs::create_dir_all(store.dir()).unwrap();
        fs::write(
            store.dir().join("config.toml"),
            format!("[defaults]\nport = {}\nuser = 'testuser'\n", host.port),
        )
        .unwrap();
        let mut server = host.clone();
        server.name = "shared".into();
        server.port = 0;
        server.user.clear();
        store.save_host(None, None, server.clone()).unwrap();
        let config_before = fs::read(store.dir().join("config.toml")).unwrap();
        let service = RemoteService::with_options(BrowserService::new().unwrap(), options);
        let service = if let Some(roots) = roots {
            service.with_trust(Arc::new(Manager::with_roots(store.clone(), roots)))
        } else {
            service
        };
        let active = service
            .connect(host.clone(), id(1))
            .task
            .await
            .unwrap()
            .unwrap();
        let draft = Host {
            name: "unsaved".into(),
            server: "shared".into(),
            root_path: host.root_path.clone(),
            ..Host::default()
        };
        for _ in 0..3 {
            let root = service
                .test_host(
                    store.clone(),
                    Some("project".into()),
                    draft.clone(),
                    None,
                    id(2),
                )
                .task
                .await
                .unwrap()
                .unwrap();
            assert_eq!(root, active.path);
        }
        assert_eq!(active.session.state(), ConnectionState::Connected);
        assert_eq!(
            fs::read(store.dir().join("config.toml")).unwrap(),
            config_before
        );
        assert!(!store.dir().join("projects/project.toml").exists());
        assert!(store.registry().unwrap().projects.is_empty());
        let invalid_root = Host {
            root_path: "/missing-host-test-root".into(),
            ..host.clone()
        };
        assert!(
            service
                .test_host(store.clone(), None, invalid_root, None, id(3))
                .task
                .await
                .unwrap()
                .is_err()
        );
        let wrong_password = Host {
            auth: drift_core::config::Auth {
                password: "incorrect".into(),
                ..host.auth.clone()
            },
            ..host.clone()
        };
        assert!(
            service
                .test_host(store.clone(), None, wrong_password, None, id(4))
                .task
                .await
                .unwrap()
                .is_err()
        );
        let cancelled = service.test_host(store.clone(), None, host.clone(), None, id(5));
        cancelled.cancel.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(10), cancelled.task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert_eq!(active.session.state(), ConnectionState::Connected);
        if host.protocol != "sftp" {
            let fixture = if host.protocol == "ftps" { &ftps } else { &ftp };
            fixture.flag("hold-list", "");
            let before = fixture.commands("LIST");
            let cancelled = service.test_host(store.clone(), None, host.clone(), None, id(8));
            tokio::time::timeout(Duration::from_secs(5), async {
                while fixture.commands("LIST") <= before {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            cancelled.cancel.cancel();
            assert!(
                tokio::time::timeout(Duration::from_secs(2), cancelled.task)
                    .await
                    .unwrap()
                    .unwrap()
                    .is_err()
            );
            fs::remove_file(fixture.dir.path().join("hold-list")).unwrap();
            assert_eq!(active.session.state(), ConnectionState::Connected);
        }
        service
            .list(active.session.clone(), active.path.clone(), id(6))
            .task
            .await
            .unwrap()
            .unwrap();
        active.session.shutdown().await;
    }
}

#[tokio::test]
async fn host_test_challenges_pin_retries_and_reset_affects_only_future_connections() {
    let server = support::ftp::Server::tls_with_limit("valid", 8);
    let home = tempfile::tempdir().unwrap();
    let store = Store::new(home.path().into());
    let manager = Arc::new(Manager::with_roots(
        store.clone(),
        rustls::RootCertStore::empty(),
    ));
    let service = RemoteService::with_options(BrowserService::new().unwrap(), server.options())
        .with_trust(manager.clone());
    let host = server.host();
    let inspected = match service
        .test_host(store.clone(), None, host.clone(), None, id(1))
        .task
        .await
        .unwrap()
    {
        Err(Error::Certificate { challenge, .. }) => *challenge,
        _ => panic!("expected real FTPS challenge"),
    };
    assert_eq!(server.commands("PASS"), 0);
    service
        .trust_certificate(inspected.clone(), true, id(2))
        .task
        .await
        .unwrap()
        .unwrap();
    service
        .test_host(
            store.clone(),
            None,
            host.clone(),
            Some(inspected.clone()),
            id(3),
        )
        .task
        .await
        .unwrap()
        .unwrap();
    let active = service
        .connect(host.clone(), id(4))
        .task
        .await
        .unwrap()
        .unwrap();
    let snapshot = service
        .inspect_host_trust(store.clone(), None, host.clone(), id(5))
        .task
        .await
        .unwrap()
        .unwrap();
    service
        .reset_host_trust(snapshot, id(6))
        .task
        .await
        .unwrap()
        .unwrap();
    assert!(store.trusted_certificates().unwrap().is_empty());
    assert_eq!(active.session.state(), ConnectionState::Connected);
    service
        .list(active.session.clone(), active.path, id(7))
        .task
        .await
        .unwrap()
        .unwrap();
    active.session.shutdown().await;
    assert!(matches!(
        service
            .test_host(store.clone(), None, host.clone(), None, id(8))
            .task
            .await
            .unwrap(),
        Err(Error::Certificate { .. })
    ));
    manager.grant(&inspected, false).unwrap();
    server.flag("rotate-cert", "");
    assert!(matches!(
        service
            .test_host(
                store.clone(),
                None,
                host.clone(),
                Some(inspected.clone()),
                id(9)
            )
            .task
            .await
            .unwrap(),
        Err(Error::Certificate { .. })
    ));
    let before = server.commands("PASS");
    let mut changed_protocol = host;
    changed_protocol.protocol = "ftp".into();
    assert!(
        service
            .test_host(store, None, changed_protocol, Some(inspected), id(10))
            .task
            .await
            .unwrap()
            .is_err()
    );
    assert_eq!(server.commands("PASS"), before);
}
