//! A real Go SSH/SFTP daemon with real files and OpenSSH keys. No mocks.
#![allow(dead_code)] // Shared by transport, application and GUI integration suites.
use drift_core::{
    config::{Auth, Host},
    remote::ConnectOptions,
};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
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

#[derive(Clone, Copy, Debug)]
pub struct CertificateOptions<'a> {
    pub host_key: &'a str,
    pub ca_key: &'a str,
    pub host_certificate: bool,
    pub principals: Option<&'a str>,
    pub validity: &'a str,
    pub critical: Option<&'a str>,
    pub raw_fallback: bool,
    pub tamper_signature: bool,
}
impl Default for CertificateOptions<'_> {
    fn default() -> Self {
        Self {
            host_key: "ed25519",
            ca_key: "ed25519",
            host_certificate: true,
            principals: Some("127.0.0.1"),
            validity: "-1h:+1h",
            critical: None,
            raw_fallback: false,
            tamper_signature: false,
        }
    }
}

pub struct Server {
    pub dir: tempfile::TempDir,
    pub port: u16,
    pub child: Process,
}
impl Server {
    fn generate_key(dir: &Path, name: &str, algorithm: &str) {
        let mut command = Command::new("ssh-keygen");
        command.args(["-q", "-t", algorithm, "-N", ""]);
        match algorithm {
            "rsa" => {
                command.args(["-b", "2048"]);
            }
            "ecdsa" => {
                command.args(["-b", "256"]);
            }
            _ => {}
        }
        let result = command
            .arg("-f")
            .arg(dir.join(name))
            .output()
            .expect("ssh-keygen is required for real protocol tests");
        assert!(
            result.status.success(),
            "generate {algorithm} {name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fn spawn(
        dir: &Path,
        port: u16,
        drop_probes: bool,
        certificate: Option<&CertificateOptions<'_>>,
    ) -> (Process, u16) {
        let binary = SERVER.get_or_init(|| {
            let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .find(|p| p.join("go.mod").is_file())
                .unwrap()
                .to_path_buf();
            let dir = tempfile::tempdir().unwrap().keep();
            let cache = tempfile::tempdir().unwrap();
            let binary = dir.join("sftp-test-server");
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
                "build real SFTP test daemon: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            binary
        });
        let mut command = Command::new(binary);
        command
            .arg(dir.join("host-key"))
            .arg(dir.join("client-key.pub"))
            .arg(dir.join("files"))
            .arg(port.to_string())
            .env("DRIFT_TEST_DROP_PROBES", drop_probes.to_string())
            .env_remove("DRIFT_TEST_HOST_CERT")
            .env_remove("DRIFT_TEST_RAW_HOST_KEY")
            .env_remove("DRIFT_TEST_TAMPER_CERT")
            .stdout(Stdio::piped());
        if let Some(options) = certificate {
            command
                .env("DRIFT_TEST_HOST_CERT", dir.join("host-key-cert.pub"))
                .env("DRIFT_TEST_RAW_HOST_KEY", options.raw_fallback.to_string())
                .env(
                    "DRIFT_TEST_TAMPER_CERT",
                    options.tamper_signature.to_string(),
                );
        }
        let mut child = Process(command.spawn().unwrap());
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port = line
            .trim()
            .parse()
            .expect("server did not return listening port");
        (child, port)
    }
    pub fn new(drop_probes: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("files")).unwrap();
        for name in ["host-key", "client-key"] {
            Self::generate_key(dir.path(), name, "ed25519");
        }
        let (child, port) = Self::spawn(dir.path(), 0, drop_probes, None);
        Self { dir, port, child }
    }
    pub fn certified(options: CertificateOptions<'_>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("files")).unwrap();
        Self::generate_key(dir.path(), "host-key", options.host_key);
        Self::generate_key(dir.path(), "ca-key", options.ca_key);
        Self::generate_key(dir.path(), "client-key", "ed25519");
        let mut command = Command::new("ssh-keygen");
        command
            .args(["-q", "-s"])
            .arg(dir.path().join("ca-key"))
            .args(["-I", "drift-test-host", "-V", options.validity]);
        if options.host_certificate {
            command.arg("-h");
        }
        if let Some(principals) = options.principals {
            command.args(["-n", principals]);
        }
        if let Some(critical) = options.critical {
            command.args(["-O", critical]);
        }
        let result = command
            .arg(dir.path().join("host-key.pub"))
            .output()
            .expect("ssh-keygen is required for real certificate tests");
        assert!(
            result.status.success(),
            "sign real SSH certificate: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let (child, port) = Self::spawn(dir.path(), 0, false, Some(&options));
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
            tls: None,
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
        Self::generate_key(self.dir.path(), "host-key", "ed25519");
        let (child, port) = Self::spawn(self.dir.path(), self.port, false, None);
        self.child = child;
        assert_eq!(port, self.port);
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

pub mod ftp;
