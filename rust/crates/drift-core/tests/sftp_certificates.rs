mod support;

use drift_core::{
    config::Host,
    remote::{self, ConnectionState},
};
use std::{fs, process::Command};
use support::{CertificateOptions, Server};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

fn public_key(server: &Server, name: &str) -> String {
    fs::read_to_string(server.dir.path().join(name)).unwrap()
}

fn authority(server: &Server, scope: &str) -> String {
    format!(
        "@cert-authority {scope} {}",
        public_key(server, "ca-key.pub")
    )
}

fn endpoint(server: &Server, hostname: &str) -> String {
    format!("[{hostname}]:{}", server.port)
}

fn write_known_hosts(server: &Server, contents: &str) {
    let path = server.options().known_hosts;
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

async fn assert_sftp_round_trip(server: &Server, host: Host) {
    let path = server.options().known_hosts;
    let trusted = fs::read(&path).unwrap();
    let root = server.dir.path().join("files");
    let source = root.join("source.txt");
    let target = root.join("uploaded.txt");
    let payload = b"real certificate-authenticated SFTP payload\n";
    fs::write(&source, payload).unwrap();
    let client = remote::connect(host, server.options(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(*client.state().borrow(), ConnectionState::Connected);
    assert!(!target.exists(), "connecting must not start a transfer");
    let entries = client.read_dir(root.to_str().unwrap()).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "source.txt");
    assert!(entries[0].regular);
    let mut stream = client.open(source.to_str().unwrap()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    stream.close().await.unwrap();
    assert_eq!(bytes, payload);
    // The upload consumes a real SFTP reader, not a fabricated RemoteRead.
    let source_stream = client.open(source.to_str().unwrap()).await.unwrap();
    client
        .upload(target.to_str().unwrap(), source_stream)
        .await
        .unwrap();
    assert_eq!(fs::read(&target).unwrap(), payload);
    assert_eq!(
        client
            .read_limited(target.to_str().unwrap(), 1024)
            .await
            .unwrap(),
        payload
    );
    client.delete(target.to_str().unwrap()).await.unwrap();
    assert!(!target.exists());
    assert_eq!(fs::read(&source).unwrap(), payload);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    client.shutdown().await;
    assert_eq!(*client.state().borrow(), ConnectionState::Closed);
    assert_eq!(
        fs::read(path).unwrap(),
        trusted,
        "certificate trust is read-only"
    );
}

async fn assert_rejected_unchanged(server: &Server, host: Host, case: &str) -> String {
    let path = server.options().known_hosts;
    let before = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("read known_hosts before {case}: {error}"),
    };
    let error = match remote::connect(host, server.options(), CancellationToken::new()).await {
        Ok(client) => {
            client.shutdown().await;
            panic!("{case}: certificate unexpectedly accepted");
        }
        Err(error) => error.to_string(),
    };
    assert!(!error.contains("timed out"), "{case}: {error}");
    match before {
        Some(bytes) => assert_eq!(
            fs::read(path).unwrap(),
            bytes,
            "{case}: failure must not persist a certificate or its leaf key"
        ),
        None => assert!(
            !path.exists(),
            "{case}: unknown certificate must not create known_hosts"
        ),
    }
    error
}

#[tokio::test]
async fn trusted_certificate_scoped_to_nondefault_endpoint_performs_real_sftp() {
    let server = Server::certified(Default::default());
    assert_ne!(server.port, 22);
    write_known_hosts(
        &server,
        &authority(&server, &endpoint(&server, "127.0.0.1")),
    );
    assert_sftp_round_trip(&server, server.host()).await;
}

#[tokio::test]
async fn wildcard_authority_honors_positive_patterns_and_negation() {
    let server = Server::certified(Default::default());
    let allowed = format!("[127.0.0.?]:{},![127.0.0.2]:{}", server.port, server.port);
    write_known_hosts(&server, &authority(&server, &allowed));
    assert_sftp_round_trip(&server, server.host()).await;
    let denied = format!(
        "[127.0.0.?]:{},!{}",
        server.port,
        endpoint(&server, "127.0.0.1")
    );
    write_known_hosts(&server, &authority(&server, &denied));
    assert_rejected_unchanged(&server, server.host(), "negated CA scope").await;
}

#[tokio::test]
async fn hashed_authority_generated_by_openssh_authenticates_real_sftp() {
    let server = Server::certified(Default::default());
    // ssh-keygen -H skips authority rows, so add the marker after real hashing.
    write_known_hosts(
        &server,
        &format!(
            "{} {}",
            endpoint(&server, "127.0.0.1"),
            public_key(&server, "ca-key.pub")
        ),
    );
    let result = Command::new("ssh-keygen")
        .args(["-q", "-H", "-f"])
        .arg(server.options().known_hosts)
        .output()
        .expect("ssh-keygen is required for real certificate tests");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let hashed = fs::read_to_string(server.options().known_hosts).unwrap();
    assert!(hashed.starts_with("|1|"), "{hashed}");
    assert!(!hashed.contains(&endpoint(&server, "127.0.0.1")));
    write_known_hosts(&server, &format!("@cert-authority {hashed}"));
    assert_sftp_round_trip(&server, server.host()).await;
}

#[tokio::test]
async fn authority_and_principal_use_requested_hostname_without_ip_fallback() {
    let server = Server::certified(CertificateOptions {
        principals: Some("localhost"),
        ..Default::default()
    });
    let mut host = server.host();
    host.hostname = "localhost".into();
    write_known_hosts(
        &server,
        &authority(&server, &endpoint(&server, "localhost")),
    );
    assert_sftp_round_trip(&server, host.clone()).await;
    write_known_hosts(
        &server,
        &authority(&server, &endpoint(&server, "127.0.0.1")),
    );
    assert_rejected_unchanged(&server, host, "IP CA scope cannot authorize hostname").await;

    let server = Server::certified(Default::default());
    let mut host = server.host();
    host.hostname = "localhost".into();
    write_known_hosts(
        &server,
        &authority(&server, &endpoint(&server, "localhost")),
    );
    assert_rejected_unchanged(&server, host, "IP principal cannot authorize hostname").await;
}

#[tokio::test]
async fn wrong_port_scope_and_ordinary_pins_cannot_authorize_a_certificate() {
    let server = Server::certified(Default::default());
    let different_port = if server.port == u16::MAX {
        server.port - 1
    } else {
        server.port + 1
    };
    let scope = endpoint(&server, "127.0.0.1");
    let cases = [
        (
            "wrong port",
            authority(&server, &format!("[127.0.0.1]:{different_port}")),
        ),
        ("default port", authority(&server, "127.0.0.1")),
        (
            "wrong hostname",
            authority(&server, &endpoint(&server, "127.0.0.2")),
        ),
        (
            "ordinary CA pin",
            format!("{scope} {}", public_key(&server, "ca-key.pub")),
        ),
        (
            "ordinary leaf pin",
            format!("{scope} {}", public_key(&server, "host-key.pub")),
        ),
    ];
    for (case, contents) in cases {
        write_known_hosts(&server, &contents);
        assert_rejected_unchanged(&server, server.host(), case).await;
    }
    let server = Server::certified(CertificateOptions {
        raw_fallback: true,
        ..Default::default()
    });
    write_known_hosts(
        &server,
        &format!(
            "{} {}",
            endpoint(&server, "127.0.0.1"),
            public_key(&server, "ca-key.pub")
        ),
    );
    assert_rejected_unchanged(&server, server.host(), "bare CA with advertised raw leaf").await;
}

#[tokio::test]
async fn current_start_is_valid_but_expired_future_and_current_end_are_rejected() {
    let server = Server::certified(CertificateOptions {
        validity: "+0s:+1h",
        ..Default::default()
    });
    write_known_hosts(
        &server,
        &authority(&server, &endpoint(&server, "127.0.0.1")),
    );
    assert_sftp_round_trip(&server, server.host()).await;
    for validity in ["-2h:-1h", "+1h:+2h", "-1h:+0s"] {
        let server = Server::certified(CertificateOptions {
            validity,
            ..Default::default()
        });
        write_known_hosts(
            &server,
            &authority(&server, &endpoint(&server, "127.0.0.1")),
        );
        assert_rejected_unchanged(&server, server.host(), validity).await;
    }
}

#[tokio::test]
async fn user_certificate_cannot_be_used_as_a_host_certificate() {
    let server = Server::certified(CertificateOptions {
        host_certificate: false,
        ..Default::default()
    });
    write_known_hosts(
        &server,
        &authority(&server, &endpoint(&server, "127.0.0.1")),
    );
    assert_rejected_unchanged(&server, server.host(), "user certificate").await;
}

#[tokio::test]
async fn empty_principals_allow_any_host_but_nonempty_principals_are_exact() {
    let server = Server::certified(CertificateOptions {
        principals: None,
        ..Default::default()
    });
    write_known_hosts(
        &server,
        &authority(&server, &endpoint(&server, "127.0.0.1")),
    );
    assert_sftp_round_trip(&server, server.host()).await;
    for principal in ["elsewhere.example", "127.0.0.*", "[127.0.0.1]:22"] {
        let server = Server::certified(CertificateOptions {
            principals: Some(principal),
            ..Default::default()
        });
        write_known_hosts(
            &server,
            &authority(&server, &endpoint(&server, "127.0.0.1")),
        );
        assert_rejected_unchanged(&server, server.host(), principal).await;
    }
}

#[tokio::test]
async fn unsupported_critical_options_are_rejected() {
    for critical in [
        "critical:drift-test=required",
        "critical:force-command=/bin/true",
    ] {
        let server = Server::certified(CertificateOptions {
            critical: Some(critical),
            ..Default::default()
        });
        write_known_hosts(
            &server,
            &authority(&server, &endpoint(&server, "127.0.0.1")),
        );
        assert_rejected_unchanged(&server, server.host(), critical).await;
    }
}

#[tokio::test]
async fn forged_ca_signature_is_rejected_despite_valid_leaf_exchange_signature() {
    let server = Server::certified(CertificateOptions {
        tamper_signature: true,
        ..Default::default()
    });
    write_known_hosts(
        &server,
        &authority(&server, &endpoint(&server, "127.0.0.1")),
    );
    assert_rejected_unchanged(&server, server.host(), "tampered CA signature").await;
}

#[tokio::test]
async fn untrusted_signing_ca_cannot_use_another_scoped_authority() {
    let server = Server::certified(Default::default());
    let unrelated = Server::certified(Default::default());
    write_known_hosts(
        &server,
        &authority(&unrelated, &endpoint(&server, "127.0.0.1")),
    );
    assert_rejected_unchanged(&server, server.host(), "different signing CA").await;
}

#[tokio::test]
async fn later_revocations_of_ca_leaf_and_certificate_are_global_regardless_of_scope() {
    let server = Server::certified(Default::default());
    let trusted = authority(&server, &endpoint(&server, "127.0.0.1"));
    for key in ["ca-key.pub", "host-key.pub", "host-key-cert.pub"] {
        write_known_hosts(
            &server,
            &format!(
                "{trusted}@revoked [elsewhere.example]:1 {}",
                public_key(&server, key)
            ),
        );
        let error = assert_rejected_unchanged(&server, server.host(), key).await;
        assert!(error.contains("revoked"), "{key}: {error}");
    }
}

#[tokio::test]
async fn unknown_certificates_never_create_or_append_tofu_trust() {
    let server = Server::certified(Default::default());
    assert!(!server.options().known_hosts.exists());
    assert_rejected_unchanged(&server, server.host(), "absent known_hosts").await;
    write_known_hosts(&server, "");
    assert_rejected_unchanged(&server, server.host(), "empty known_hosts").await;
}

#[tokio::test]
async fn certificates_are_preferred_for_each_leaf_algorithm_independently_of_ca_algorithm() {
    for (host_key, ca_key) in [
        ("ed25519", "ed25519"),
        ("rsa", "ed25519"),
        ("ecdsa", "ed25519"),
        ("ed25519", "rsa"),
        ("ed25519", "ecdsa"),
    ] {
        let server = Server::certified(CertificateOptions {
            host_key,
            ca_key,
            raw_fallback: true,
            ..Default::default()
        });
        write_known_hosts(
            &server,
            &authority(&server, &endpoint(&server, "127.0.0.1")),
        );
        assert_sftp_round_trip(&server, server.host()).await;
    }
}

#[tokio::test]
async fn invalid_certificate_never_downgrades_to_a_coexisting_raw_pin() {
    for options in [
        CertificateOptions {
            principals: Some("elsewhere.example"),
            raw_fallback: true,
            ..Default::default()
        },
        CertificateOptions {
            tamper_signature: true,
            raw_fallback: true,
            ..Default::default()
        },
    ] {
        let server = Server::certified(options);
        let scope = endpoint(&server, "127.0.0.1");
        write_known_hosts(
            &server,
            &format!(
                "{}{scope} {}",
                authority(&server, &scope),
                public_key(&server, "host-key.pub")
            ),
        );
        assert_rejected_unchanged(&server, server.host(), "invalid cert with valid raw pin").await;
    }
}
