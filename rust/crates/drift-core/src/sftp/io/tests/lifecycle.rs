//! Real allocated handles whose CLOSE cannot be queued under local SDK limits.
use super::super::Handle;
use crate::{
    error::Error,
    remote::ConnectOptions,
    sftp::{SocketGuard, Verifier, known_hosts},
};
use russh::client;
use russh_sftp::{
    client::{Config, RawSftpSession, error::Error as SftpError, rawsession::Limits},
    protocol::{FileAttributes, OpenFlags},
};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

// The integration fixture imports the library by its external crate name, so
// unit tests cannot include it. Keep this private runner independent of that
// import; it launches the very same Go daemon with real OpenSSH keys and files.
struct Daemon {
    dir: tempfile::TempDir,
    child: Child,
    port: u16,
}
impl Daemon {
    fn new() -> Self {
        static BINARY: OnceLock<PathBuf> = OnceLock::new();
        let binary = BINARY.get_or_init(|| {
            let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .find(|path| path.join("go.mod").is_file())
                .unwrap()
                .to_path_buf();
            let directory = tempfile::tempdir().unwrap().keep();
            let cache = tempfile::tempdir().unwrap();
            let binary = directory.join("sftp-terminal-server");
            let result = Command::new("go")
                .args(["build", "-o"])
                .arg(&binary)
                .arg("./testdata/sftp-server")
                .current_dir(repository)
                .env("GOCACHE", cache.path())
                .output()
                .expect("Go is required for real protocol tests");
            assert!(
                result.status.success(),
                "build SFTP daemon: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            binary
        });
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("files")).unwrap();
        fs::write(dir.path().join("files/source"), b"actual file contents").unwrap();
        fs::write(dir.path().join("record-transport-close"), b"armed\n").unwrap();
        let key = dir.path().join("host-key");
        assert!(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&key)
                .status()
                .unwrap()
                .success()
        );
        let child = Command::new(binary)
            .arg(&key)
            .arg(dir.path().join("host-key.pub"))
            .arg(dir.path().join("files"))
            .arg("0")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        // Install the process guard before parsing startup output.
        let mut daemon = Self {
            dir,
            child,
            port: 0,
        };
        let mut line = String::new();
        BufReader::new(daemon.child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        daemon.port = line.trim().parse().expect("daemon did not return a port");
        daemon
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn unqueueable_close_is_terminal(mut daemon: Daemon, explicit: bool) {
    let options = ConnectOptions {
        known_hosts: daemon.dir.path().join("ssh/known_hosts"),
        agent_socket: None,
        timeout: Duration::from_secs(5),
        tls: None,
    };
    let stream = tokio::net::TcpStream::connect(("127.0.0.1", daemon.port))
        .await
        .unwrap()
        .into_std()
        .unwrap();
    let guard = SocketGuard(stream.try_clone().unwrap());
    let mut ssh = client::connect_stream(
        Arc::new(client::Config::default()),
        tokio::net::TcpStream::from_std(stream).unwrap(),
        Verifier {
            options: options.clone(),
            names: vec![known_hosts::endpoint("127.0.0.1", daemon.port)],
        },
    )
    .await
    .unwrap();
    assert!(
        ssh.authenticate_password("testuser", "test-password")
            .await
            .unwrap()
            .success()
    );
    assert!(
        options.known_hosts.is_file(),
        "production verifier recorded real TOFU"
    );
    let channel = ssh.channel_open_session().await.unwrap();
    channel.request_subsystem(true, "sftp").await.unwrap();
    let config = Config::default();
    let mut raw = RawSftpSession::new_with_config(channel.into_stream(), config.clone());
    raw.init().await.unwrap();
    let path = daemon.dir.path().join("files/source");
    let value = raw
        .open(
            path.to_str().unwrap(),
            OpenFlags::READ,
            FileAttributes::empty(),
        )
        .await
        .unwrap()
        .handle;
    assert!(!value.is_empty());
    assert_eq!(raw.read(value.clone(), 0, 4).await.unwrap().data, b"actu");

    // Real HANDLE fits the installed receive cap, but its framed CLOSE is one
    // byte larger than the installed outgoing cap. The daemon need not implement
    // limits@openssh.com: set_limits is the SDK's public negotiation boundary.
    let close_frame = 13 + value.len() as u64;
    assert!(close_frame <= u64::from(config.max_packet_len));
    raw.set_limits(Limits {
        packet_len: Some(close_frame - 1),
        ..Default::default()
    });
    assert!(matches!(
        raw.close(value.clone()).await.unwrap_err(),
        SftpError::Limited(_)
    ));
    ssh.send_ping().await.unwrap();
    assert!(
        !ssh.is_closed(),
        "local packet rejection must not itself kill SSH"
    );

    let raw = Arc::new(raw);
    let stop = CancellationToken::new();
    let monitor_stop = stop.clone();
    let monitor_raw = raw.clone();
    let monitor = tokio::spawn(async move {
        monitor_stop.cancelled().await;
        drop(guard);
        monitor_raw.close_session().unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), ssh).await;
    });
    let mut handle = Handle::new(raw, value, stop.clone());
    if explicit {
        let error = handle.close().await.unwrap_err();
        assert!(matches!(error, Error::Connection(_)), "{error}");
        assert!(error.to_string().contains("Limit exceeded"), "{error}");
        assert!(
            stop.is_cancelled(),
            "unqueueable explicit CLOSE must be terminal"
        );
        handle.close().await.unwrap();
        drop(handle);
    } else {
        drop(handle);
        assert!(
            stop.is_cancelled(),
            "unqueueable Drop CLOSE must be terminal"
        );
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        monitor.await.unwrap();
        loop {
            if fs::read_to_string(daemon.dir.path().join("ssh-connections"))
                .ok()
                .as_deref()
                == Some("closed\n")
                && fs::read_to_string(daemon.dir.path().join("sftp-resources"))
                    .ok()
                    .as_deref()
                    == Some("released\n")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Handle cancellation must allow transport teardown and allocation release");
    let log = fs::read_to_string(daemon.dir.path().join("sftp-operations")).unwrap();
    assert_eq!(
        log.lines()
            .filter(|line| line.starts_with("open\t"))
            .count(),
        1
    );
    assert_eq!(
        log.lines()
            .filter(|line| line.starts_with("read\t"))
            .count(),
        1
    );
    assert!(
        !log.lines().any(|line| line.starts_with("close\t")),
        "CLOSE never queued: {log}"
    );
    assert_eq!(
        fs::read_to_string(daemon.dir.path().join("sftp-resources")).unwrap(),
        "released\n"
    );
    assert_eq!(
        fs::read_to_string(daemon.dir.path().join("ssh-connections")).unwrap(),
        "closed\n"
    );
    assert_eq!(fs::read(path).unwrap(), b"actual file contents");
    assert!(
        daemon.child.try_wait().unwrap().is_none(),
        "only transport closed, not daemon"
    );
}

#[tokio::test]
async fn explicit_close_limited_before_queue_cancels_the_handle_owner() {
    let daemon = Daemon::new();
    tokio::time::timeout(
        Duration::from_secs(15),
        unqueueable_close_is_terminal(daemon, true),
    )
    .await
    .expect("real handle lifecycle test hung");
}

#[tokio::test]
async fn drop_close_limited_before_queue_cancels_the_handle_owner() {
    let daemon = Daemon::new();
    tokio::time::timeout(
        Duration::from_secs(15),
        unqueueable_close_is_terminal(daemon, false),
    )
    .await
    .expect("real handle lifecycle test hung");
}
