//! A real Go SSH/SFTP daemon with real files and OpenSSH keys. No mocks.
#![allow(dead_code)] // Shared by transport, application and GUI integration suites.
use drift_core::{
    config::{Auth, Host},
    remote::ConnectOptions,
};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::OnceLock,
    time::Duration,
};
static SERVER: OnceLock<PathBuf> = OnceLock::new();
// Kill and reap daemon processes even when an assertion panics.
pub struct Process(pub Child);
impl std::ops::Deref for Process {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for Process {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub struct Server {
    pub dir: tempfile::TempDir,
    pub port: u16,
    pub child: Process,
}
impl Server {
    pub fn new(drop_probes: bool) -> Self {
        let binary = SERVER.get_or_init(|| {
            let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .find(|p| p.join("go.mod").is_file())
                .unwrap()
                .to_path_buf();
            let dir = tempfile::tempdir().unwrap().keep();
            let binary = dir.join("sftp-test-server");
            let result = Command::new("go")
                .args(["build", "-o"])
                .arg(&binary)
                .arg("./testdata/sftp-server")
                .current_dir(repository)
                .env("GOCACHE", dir.join("go-cache"))
                .output()
                .expect("Go is required for real protocol tests");
            assert!(
                result.status.success(),
                "build real SFTP test daemon: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            binary
        });
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("files")).unwrap();
        for name in ["host-key", "client-key"] {
            assert!(
                Command::new("ssh-keygen")
                    .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                    .arg(dir.path().join(name))
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let mut child = Process(
            Command::new(binary)
                .arg(dir.path().join("host-key"))
                .arg(dir.path().join("client-key.pub"))
                .arg(dir.path().join("files"))
                .arg("0")
                .env("DRIFT_TEST_DROP_PROBES", drop_probes.to_string())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port = line
            .trim()
            .parse()
            .expect("server did not return listening port");
        Self { dir, port, child }
    }
    pub fn host(&self) -> Host {
        Host {
            name: "Test SFTP".into(),
            hostname: "127.0.0.1".into(),
            port: self.port,
            user: "testuser".into(),
            protocol: "sftp".into(),
            root_path: self.dir.path().join("files").to_string_lossy().into_owned(),
            auth: Auth {
                kind: "password".into(),
                password: "test-password".into(),
                ..Default::default()
            },
            keep_alive_interval: Some(0),
            ..Default::default()
        }
    }
    pub fn options(&self) -> ConnectOptions {
        ConnectOptions {
            known_hosts: self.dir.path().join("ssh/known_hosts"),
            agent_socket: None,
            timeout: Duration::from_secs(5),
        }
    }
    pub fn key_host(&self, encrypted: bool) -> Host {
        let path = self.dir.path().join(if encrypted {
            "encrypted-key"
        } else {
            "client-key"
        });
        if encrypted {
            fs::copy(self.dir.path().join("client-key"), &path).unwrap();
            assert!(
                Command::new("ssh-keygen")
                    .args(["-q", "-p", "-P", "", "-N", "test-passphrase", "-f"])
                    .arg(&path)
                    .stdout(Stdio::null())
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let mut host = self.host();
        host.auth = Auth {
            kind: "keyfile".into(),
            key_file: path.to_string_lossy().into_owned(),
            passphrase: if encrypted {
                "test-passphrase".into()
            } else {
                String::new()
            },
            ..Default::default()
        };
        host
    }
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    pub fn change_key(&mut self) {
        self.stop();
        fs::remove_file(self.dir.path().join("host-key")).unwrap();
        fs::remove_file(self.dir.path().join("host-key.pub")).unwrap();
        assert!(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(self.dir.path().join("host-key"))
                .status()
                .unwrap()
                .success()
        );
        self.child = Process(
            Command::new(SERVER.get().unwrap())
                .arg(self.dir.path().join("host-key"))
                .arg(self.dir.path().join("client-key.pub"))
                .arg(self.dir.path().join("files"))
                .arg(self.port.to_string())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut line = String::new();
        BufReader::new(self.child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line.trim(), self.port.to_string());
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

pub mod ftp;
