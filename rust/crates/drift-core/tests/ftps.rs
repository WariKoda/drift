mod support;
use drift_core::{
    error::Error,
    remote::{self, ConnectionState},
    store::Store,
    tlstrust::{Challenge, Manager, Problem},
};
use fs2::FileExt;
use std::{fs, os::unix::fs::PermissionsExt};
use support::ftp::Server;
use tokio_util::sync::CancellationToken;
fn manager(server: &Server, trusted: bool) -> Manager {
    let roots = if trusted {
        server.roots()
    } else {
        rustls::RootCertStore::empty()
    };
    Manager::with_roots(Store::new(server.dir.path().join("config")), roots)
}
async fn connect(
    server: &Server,
    manager: &Manager,
    required: Option<Challenge>,
) -> drift_core::error::Result<std::sync::Arc<dyn remote::RemoteClient>> {
    let mut options = server.options();
    let mut policy = manager.policy()?;
    if let Some(challenge) = required {
        policy = policy.require(challenge)
    };
    options.tls = Some(policy);
    remote::connect(server.host(), options, CancellationToken::new()).await
}
async fn challenge(server: &Server, manager: &Manager) -> Challenge {
    match connect(server, manager, None).await {
        Err(Error::Certificate { challenge, .. }) => *challenge,
        Err(error) => panic!("expected challenge: {error}"),
        Ok(client) => {
            client.shutdown().await;
            panic!("unexpected connection")
        }
    }
}
#[tokio::test]
async fn ftps_challenges_then_session_trust_protects_control_and_data_without_persisting() {
    for mode in ["valid", "self-signed"] {
        let server = Server::tls(mode);
        fs::write(server.dir.path().join("files/file"), b"secret").unwrap();
        let manager = manager(&server, false);
        let inspected = challenge(&server, &manager).await;
        assert_eq!(inspected.problems, vec![Problem::UnknownAuthority]);
        assert!(inspected.names.contains(&"127.0.0.1".into()));
        assert_eq!(server.commands("PASS"), 0);
        manager.grant(&inspected, false).unwrap();
        let client = connect(&server, &manager, Some(inspected)).await.unwrap();
        assert_eq!(server.commands("USER"), 4);
        assert_eq!(client.read_limited("/file", 100).await.unwrap(), b"secret");
        assert_eq!(client.read_dir("/").await.unwrap().len(), 1);
        assert!(server.commands("TLS") >= 6);
        assert!(
            !server
                .dir
                .path()
                .join("config/trusted-certificates.toml")
                .exists()
        );
        client.shutdown().await;
        let mut other_name = server.host();
        other_name.hostname = "localhost".into();
        let mut options = server.options();
        options.tls = Some(manager.policy().unwrap());
        assert!(
            matches!(remote::connect(other_name,options,CancellationToken::new()).await,Err(Error::Certificate{challenge,..}) if challenge.endpoint.hostname=="localhost")
        );
        assert_eq!(
            challenge(&server, &self::manager(&server, false))
                .await
                .problems,
            vec![Problem::UnknownAuthority]
        );
    }
}
#[tokio::test]
async fn ftps_native_ca_validation_and_retry_pinning_detect_changed_valid_certificates() {
    let server = Server::tls("valid");
    let manager = manager(&server, true);
    let client = connect(&server, &manager, None).await.unwrap();
    client.shutdown().await;
    let untrusted = self::manager(&server, false);
    let inspected = challenge(&server, &untrusted).await;
    let mut wrong_endpoint = inspected.clone();
    wrong_endpoint.endpoint.port = server.port.wrapping_add(1);
    assert!(matches!(
        connect(&server, &manager, Some(wrong_endpoint)).await,
        Err(Error::Invalid(_))
    ));
    server.flag("rotate-cert", "");
    match connect(&server, &manager, Some(inspected.clone())).await {
        Err(Error::Certificate { challenge, .. }) => {
            assert_eq!(challenge.problems, vec![Problem::CertificateChanged]);
            assert_eq!(challenge.previous_fingerprint, Some(inspected.fingerprint));
        }
        _ => panic!("changed trusted-CA certificate was accepted on pinned retry"),
    }
    let client = connect(&server, &manager, None).await.unwrap();
    client.shutdown().await;
}
#[tokio::test]
async fn ftps_exception_problem_sets_are_exact_and_invalid_chains_cannot_be_approved() {
    for (mode, problem) in [
        ("expired", Problem::Expired),
        ("future", Problem::NotYetValid),
        ("wrong-name", Problem::HostnameMismatch),
    ] {
        let server = Server::tls(mode);
        let manager = manager(&server, false);
        let inspected = challenge(&server, &manager).await;
        assert!(inspected.problems.contains(&Problem::UnknownAuthority));
        assert!(inspected.problems.contains(&problem));
        let mut incomplete = inspected.clone();
        incomplete.problems = vec![Problem::UnknownAuthority];
        manager.grant(&incomplete, false).unwrap();
        assert_eq!(
            challenge(&server, &manager).await.problems,
            inspected.problems
        );
        manager.grant(&inspected, false).unwrap();
        let client = connect(&server, &manager, Some(inspected)).await.unwrap();
        client.shutdown().await;
    }
    for mode in ["bad-usage", "bad-signature"] {
        let server = Server::tls(mode);
        for trusted in [false, true] {
            let manager = manager(&server, trusted);
            assert!(
                matches!(
                    connect(&server, &manager, None).await,
                    Err(Error::Connection(_))
                ),
                "invalid structural chain must not produce an approvable challenge: {mode}"
            );
        }
        assert_eq!(server.commands("PASS"), 0);
    }
}
#[tokio::test]
async fn ftps_permanent_trust_reloads_and_conflicting_confirmation_grants_nothing() {
    let server = Server::tls("valid");
    let first = manager(&server, false);
    let second = manager(&server, false);
    let original = challenge(&server, &first).await;
    let stale = challenge(&server, &second).await;
    // An actual failed lock acquisition must not grant session fallback.
    fs::create_dir_all(server.dir.path().join("config")).unwrap();
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(server.dir.path().join("config/write.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    assert!(matches!(first.grant(&original, true), Err(Error::Busy)));
    lock.unlock().unwrap();
    assert_eq!(challenge(&server, &first).await.problems, original.problems);
    first.grant(&original, true).unwrap();
    assert!(matches!(
        second.grant(&stale, true),
        Err(Error::Conflict(_))
    ));
    let path = server.dir.path().join("config/trusted-certificates.toml");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let reloaded = manager(&server, false);
    let client = connect(&server, &reloaded, None).await.unwrap();
    client.shutdown().await;
    server.flag("rotate-cert", "");
    let changed = challenge(&server, &second).await;
    assert!(changed.problems.contains(&Problem::CertificateChanged));
    assert_ne!(changed.fingerprint, original.fingerprint);
    second.grant(&changed, true).unwrap();
    let client = connect(&server, &manager(&server, false), Some(changed))
        .await
        .unwrap();
    client.shutdown().await;
}
#[tokio::test]
async fn ftps_data_certificate_is_pinned_and_session_stops_without_retrying() {
    let server = Server::tls("valid");
    fs::write(server.dir.path().join("files/file"), b"payload").unwrap();
    let manager = manager(&server, true);
    let client = connect(&server, &manager, None).await.unwrap();
    server.flag("rotate-data", "");
    server.flag("hold-tls-failure", "");
    assert!(matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.read_limited("/file", 100)
        )
        .await
        .unwrap(),
        Err(Error::Certificate { .. })
    ));
    assert!(matches!(
        *client.state().borrow(),
        ConnectionState::Certificate(_)
    ));
    assert!(client.stat("/file").await.is_err());
    assert_eq!(server.commands("RETR"), 1);
    client.shutdown().await;
}
