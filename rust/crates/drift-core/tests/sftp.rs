mod support;
use drift_core::remote::{self, ConnectionState};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::{Command, Stdio},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn real_password_keys_host_trust_listing_limits_and_connection_loss() {
    let mut server = support::Server::new(false);
    let root = server.dir.path().join("files");
    fs::create_dir(root.join("folder")).unwrap();
    fs::write(root.join("text.txt"), "remote text").unwrap();
    fs::write(root.join("big.txt"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
    symlink("text.txt", root.join("link")).unwrap();
    let cancel = CancellationToken::new();
    let client = remote::connect(server.host(), server.options(), cancel.clone())
        .await
        .unwrap();
    cancel.cancel(); // The monitor is independent of the completed connect request.
    let entries = client.read_dir(root.to_str().unwrap()).await.unwrap();
    assert_eq!(entries[0].name, "folder");
    assert!(entries[0].directory);
    assert!(entries.iter().find(|e| e.name == "link").unwrap().symlink);
    assert_eq!(
        client
            .read_limited(root.join("text.txt").to_str().unwrap(), 1024 * 1024)
            .await
            .unwrap(),
        b"remote text"
    );
    assert!(
        client
            .read_limited(root.join("big.txt").to_str().unwrap(), 1024 * 1024)
            .await
            .is_err()
    );
    assert!(
        client
            .read_limited(root.join("link").to_str().unwrap(), 1024 * 1024)
            .await
            .is_err()
    );
    assert!(
        client
            .read_dir(root.join("missing").to_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(
        fs::metadata(&server.options().known_hosts)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    client.shutdown().await;
    assert_eq!(*client.state().borrow(), ConnectionState::Closed);
    assert!(
        Command::new("ssh-keygen")
            .args(["-q", "-H", "-f"])
            .arg(server.options().known_hosts)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    for encrypted in [false, true] {
        let client = remote::connect(
            server.key_host(encrypted),
            server.options(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        client.shutdown().await;
    }
    let fifo = server.dir.path().join("key-fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let mut host = server.key_host(false);
    host.auth.key_file = fifo.to_string_lossy().into_owned();
    let error = remote::connect(host, server.options(), CancellationToken::new())
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(!error.contains("timed out"));
    assert!(error.contains("key file"));
    let mut wrong = server.host();
    wrong.auth.password = "wrong".into();
    assert!(
        remote::connect(wrong, server.options(), CancellationToken::new())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("rejected")
    );
    let client = remote::connect(server.host(), server.options(), CancellationToken::new())
        .await
        .unwrap();
    let mut state = client.state();
    server.stop();
    tokio::time::timeout(Duration::from_secs(5), async {
        while *state.borrow_and_update() == ConnectionState::Connected {
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(matches!(&*state.borrow(), ConnectionState::Failed(_)));
    server.change_key();
    assert!(
        remote::connect(server.host(), server.options(), CancellationToken::new())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("changed")
    );
}

#[tokio::test]
async fn real_agent_and_stalled_agent_timeout_release_the_socket() {
    let server = support::Server::new(false);
    let socket = server.dir.path().join("agent.sock");
    let mut agent = support::Process(
        Command::new("ssh-agent")
            .args(["-D", "-a"])
            .arg(&socket)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        Command::new("ssh-add")
            .arg(server.dir.path().join("client-key"))
            .env("SSH_AUTH_SOCK", &socket)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    let mut host = server.host();
    host.auth = drift_core::config::Auth {
        kind: "agent".into(),
        ..Default::default()
    };
    let mut options = server.options();
    options.agent_socket = Some(socket);
    let client = remote::connect(host.clone(), options, CancellationToken::new())
        .await
        .unwrap();
    client.shutdown().await;
    agent.kill().unwrap();
    agent.wait().unwrap();
    let socket = server.dir.path().join("stalled.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let accept = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut bytes)
            .await
            .unwrap();
        bytes
    });
    let mut options = server.options();
    options.agent_socket = Some(socket);
    options.timeout = Duration::from_secs(2);
    assert!(
        remote::connect(host, options, CancellationToken::new())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("timed out")
    );
    assert!(
        !tokio::time::timeout(Duration::from_secs(3), accept)
            .await
            .unwrap()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn keepalive_is_optional_and_missing_replies_close_the_transport() {
    let server = support::Server::new(false);
    let client = remote::connect(server.host(), server.options(), CancellationToken::new())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1300)).await;
    assert!(!server.dir.path().join("probes").exists());
    client.shutdown().await;
    let mut host = server.host();
    host.keep_alive_interval = Some(1);
    let client = remote::connect(host, server.options(), CancellationToken::new())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !server.dir.path().join("probes").exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(*client.state().borrow(), ConnectionState::Connected);
    client.shutdown().await;
    let server = support::Server::new(true);
    let mut host = server.host();
    host.keep_alive_interval = Some(1);
    let client = remote::connect(host, server.options(), CancellationToken::new())
        .await
        .unwrap();
    let mut state = client.state();
    tokio::time::timeout(Duration::from_secs(25), async {
        while *state.borrow_and_update() == ConnectionState::Connected {
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(
        matches!(&*state.borrow(),ConnectionState::Failed(message) if message.contains("keep-alive"))
    );
}

#[tokio::test]
async fn openssh_patterns_revocation_and_corrupt_files_never_silently_change_trust() {
    let server = support::Server::new(false);
    let options = server.options();
    fs::create_dir_all(options.known_hosts.parent().unwrap()).unwrap();
    let key = fs::read_to_string(server.dir.path().join("host-key.pub")).unwrap();
    let patterns = format!(
        "[127.0.0.?]:{},![127.0.0.2]:{} {}",
        server.port, server.port, key
    );
    fs::write(&options.known_hosts, &patterns).unwrap();
    let client = remote::connect(server.host(), options.clone(), CancellationToken::new())
        .await
        .unwrap();
    client.shutdown().await;
    assert_eq!(fs::read_to_string(&options.known_hosts).unwrap(), patterns);
    let revoked = format!("@revoked * {key}");
    fs::write(&options.known_hosts, &revoked).unwrap();
    assert!(
        remote::connect(server.host(), options.clone(), CancellationToken::new())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("revoked")
    );
    assert_eq!(fs::read_to_string(&options.known_hosts).unwrap(), revoked);
    fs::write(&options.known_hosts, "malformed known_hosts\n").unwrap();
    assert!(
        remote::connect(server.host(), options.clone(), CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(&options.known_hosts).unwrap(),
        "malformed known_hosts\n"
    );
}
