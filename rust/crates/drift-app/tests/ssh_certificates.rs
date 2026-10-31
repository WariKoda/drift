#[path = "../../drift-core/tests/support/mod.rs"]
mod support;
use drift_app::{
    browser::{BrowserService, OperationId},
    remote::RemoteService,
};
use drift_core::{error::Error, remote::ConnectionState, store::Store};
use std::fs;

fn id(operation: u64) -> OperationId {
    OperationId {
        project: 1,
        operation,
    }
}
fn authority(server: &support::Server) -> String {
    format!(
        "@cert-authority [127.0.0.1]:{} {}",
        server.port,
        fs::read_to_string(server.dir.path().join("ca-key.pub")).unwrap()
    )
}
#[tokio::test]
async fn certified_sessions_and_host_tests_preserve_trust_drafts_and_active_connections() {
    let server = support::Server::certified(Default::default());
    fs::write(
        server.dir.path().join("files/text.txt"),
        "certificate-backed text",
    )
    .unwrap();
    let options = server.options();
    fs::create_dir_all(options.known_hosts.parent().unwrap()).unwrap();
    let trusted = authority(&server);
    fs::write(&options.known_hosts, &trusted).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let store = Store::new(directory.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let config = fs::read(store.dir().join("config.toml")).unwrap();
    let service = RemoteService::with_options(BrowserService::new().unwrap(), options.clone());
    let active = service
        .connect(server.host(), id(1))
        .task
        .await
        .unwrap()
        .unwrap();
    let text = service
        .preview(
            active.session.clone(),
            format!("{}/text.txt", active.path),
            id(2),
        )
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(text, "certificate-backed text");
    let test = service
        .test_host(store.clone(), None, server.host(), None, id(3))
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(test, active.path);
    assert_eq!(active.session.state(), ConnectionState::Connected);
    let cancelled = service.test_host(store.clone(), None, server.host(), None, id(4));
    cancelled.cancel.cancel();
    assert!(cancelled.task.await.unwrap().is_err());
    assert_eq!(fs::read_to_string(&options.known_hosts).unwrap(), trusted);
    assert!(fs::read(store.dir().join("config.toml")).unwrap() == config);
    let revoked = format!(
        "{trusted}@revoked * {}",
        fs::read_to_string(server.dir.path().join("ca-key.pub")).unwrap()
    );
    fs::write(&options.known_hosts, &revoked).unwrap();
    let error = service
        .test_host(store.clone(), None, server.host(), None, id(5))
        .task
        .await
        .unwrap()
        .err()
        .unwrap();
    assert!(!matches!(error, Error::Certificate { .. }));
    assert!(error.to_string().contains("revoked"));
    assert_eq!(active.session.state(), ConnectionState::Connected);
    assert_eq!(fs::read_to_string(&options.known_hosts).unwrap(), revoked);
    assert!(fs::read(store.dir().join("config.toml")).unwrap() == config);
    active.session.shutdown().await;
}

#[tokio::test]
async fn untrusted_certificate_is_an_error_not_a_tls_approval_or_a_tofu_key() {
    let server = support::Server::certified(Default::default());
    let options = server.options();
    let service = RemoteService::with_options(BrowserService::new().unwrap(), options.clone());
    let error = service
        .connect(server.host(), id(1))
        .task
        .await
        .unwrap()
        .err()
        .unwrap();
    assert!(!matches!(error, Error::Certificate { .. }));
    assert!(!options.known_hosts.exists());
    fs::create_dir_all(options.known_hosts.parent().unwrap()).unwrap();
    let trust = authority(&server);
    fs::write(&options.known_hosts, &trust).unwrap();
    let active = service
        .connect(server.host(), id(2))
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.session.state(), ConnectionState::Connected);
    assert_eq!(fs::read_to_string(&options.known_hosts).unwrap(), trust);
    active.session.shutdown().await;
}
