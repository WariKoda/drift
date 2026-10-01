mod support;
use drift_core::{
    error::Error,
    remote::{self, ConnectionState},
};
use std::{fs, time::Duration};
use support::ftp::Server;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn real_ftp_lists_streams_metadata_and_limits_logins_without_retrying() {
    for limit in [1, 2, 4] {
        let server = Server::new(limit);
        fs::create_dir(server.dir.path().join("files/folder")).unwrap();
        fs::write(
            server.dir.path().join("files/folder/file name"),
            b"hello\r\n",
        )
        .unwrap();
        let cancel = CancellationToken::new();
        let client = remote::connect(server.host(), server.options(), cancel.clone())
            .await
            .unwrap();
        cancel.cancel(); // Setup cancellation is detached from the session.
        assert_eq!(client.canonicalize("folder").await.unwrap(), "/folder");
        assert!(client.stat("/folder").await.unwrap().directory);
        let metadata = client.stat("/folder/file name").await.unwrap();
        assert!(metadata.regular);
        assert_eq!(metadata.size, 7);
        assert!(metadata.modified.is_some());
        let entries = client.read_dir("/").await.unwrap();
        assert_eq!(entries[0].name, "folder");
        assert!(entries[0].directory);
        let mut stream = client.open("/folder/file name").await.unwrap();
        let mut bytes = vec![];
        stream.read_to_end(&mut bytes).await.unwrap();
        stream.close().await.unwrap();
        assert_eq!(bytes, b"hello\r\n");
        assert_eq!(
            client.read_limited("/folder/file name", 100).await.unwrap(),
            bytes
        );
        assert_eq!(server.commands("USER"), limit);
        assert_eq!(server.commands("REJECT"), usize::from(limit < 4));
        client.shutdown().await;
        assert_eq!(*client.state().borrow(), ConnectionState::Closed);
        assert!(!server.dir.path().join("unused_known_hosts").exists());
    }
}
#[tokio::test]
async fn ftp_550_requires_parent_evidence_and_retains_access_errors() {
    let server = Server::new(4);
    fs::create_dir_all(server.dir.path().join("files/locked/inner")).unwrap();
    fs::write(
        server.dir.path().join("files/locked/inner/exists"),
        b"secret",
    )
    .unwrap();
    let client = remote::connect(server.host(), server.options(), CancellationToken::new())
        .await
        .unwrap();
    for path in ["/absent", "/absent/inner/file"] {
        assert!(
            matches!(client.stat(path).await, Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound)
        );
    }
    server.flag("deny", "MLST /locked/inner/exists");
    assert!(client.stat("/locked/inner/exists").await.unwrap().regular); // SIZE remains accessible, like Go.
    server.flag(
        "deny",
        "MLST /locked/inner/exists\nSIZE /locked/inner/exists\nLIST /locked/inner/exists",
    );
    assert!(
        matches!(client.stat("/locked/inner/exists").await, Err(Error::Invalid(error)) if error.contains("550"))
    );
    server.flag("deny", "LIST /locked");
    assert!(
        matches!(client.read_dir("/locked").await, Err(Error::Invalid(error)) if error.contains("550"))
    );
    assert_eq!(*client.state().borrow(), ConnectionState::Connected);
    client.shutdown().await;
}
#[tokio::test]
async fn ftp_fallback_and_final_reply_failure_are_checked() {
    let server = Server::new(1);
    server.flag("no-epsv", "");
    server.flag("no-mlst", "");
    fs::write(server.dir.path().join("files/file"), b"complete payload").unwrap();
    let client = remote::connect(server.host(), server.options(), CancellationToken::new())
        .await
        .unwrap();
    assert!(client.stat("/file").await.unwrap().modified.is_some());
    assert_eq!(client.read_dir("/").await.unwrap()[0].name, "file");
    server.flag("fail-completion", "RETR /file");
    let mut stream = client.open("/file").await.unwrap();
    let mut bytes = vec![];
    stream.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, b"complete payload");
    assert!(matches!(stream.close().await, Err(Error::Connection(_))));
    assert!(matches!(
        *client.state().borrow(),
        ConnectionState::Failed(_)
    ));
    assert!(server.commands("PASV") > 0);
    client.shutdown().await;
}
#[tokio::test]
async fn ftp_keepalive_skips_busy_streams_and_shutdown_interrupts_io() {
    let server = Server::new(1);
    fs::write(
        server.dir.path().join("files/large"),
        vec![b'x'; 8 * 1024 * 1024],
    )
    .unwrap();
    server.flag("slow-retr", "");
    let mut host = server.host();
    host.keep_alive_interval = Some(1);
    let client = remote::connect(host, server.options(), CancellationToken::new())
        .await
        .unwrap();
    let mut stream = client.open("/large").await.unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(server.commands("NOOP"), 0);
    client.shutdown().await;
    let mut bytes = vec![];
    let _ = tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut bytes))
        .await
        .unwrap();
    assert!(stream.close().await.is_err());
    assert_eq!(*client.state().borrow(), ConnectionState::Closed);
    let server = Server::new(1);
    server.flag("drop", "NOOP");
    let mut host = server.host();
    host.keep_alive_interval = Some(1);
    let client = remote::connect(host, server.options(), CancellationToken::new())
        .await
        .unwrap();
    let mut state = client.state();
    tokio::time::timeout(Duration::from_secs(3), state.changed())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(*state.borrow(), ConnectionState::Failed(_)));
}
#[tokio::test]
async fn ftp_authentication_and_command_injection_fail_without_credentials_in_errors() {
    let server = Server::new(1);
    let mut host = server.host();
    host.auth.password = "do-not-echo-this".into();
    let error = remote::connect(host, server.options(), CancellationToken::new())
        .await
        .err()
        .unwrap();
    assert!(!error.to_string().contains("do-not-echo-this"));
    let server = Server::new(1);
    let client = remote::connect(server.host(), server.options(), CancellationToken::new())
        .await
        .unwrap();
    assert!(client.stat("/file\r\nDELE /other").await.is_err());
    assert_eq!(server.commands("DELE"), 0);
    client.shutdown().await;
}

#[tokio::test]
async fn abandoning_a_real_ftp_stream_invalidates_its_control_connection() {
    let server = Server::new(1);
    fs::write(server.dir.path().join("files/file"), b"payload").unwrap();
    let client = remote::connect(server.host(), server.options(), CancellationToken::new())
        .await
        .unwrap();
    let stream = client.open("/file").await.unwrap();
    drop(stream);
    assert!(matches!(
        *client.state().borrow(),
        ConnectionState::Failed(_)
    ));
    assert!(client.stat("/file").await.is_err());
}
