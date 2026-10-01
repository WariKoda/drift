//! Real filesystem-backed FTP server, shared by core/application/GUI suites.
use super::Process;
use drift_core::{
    config::{Auth, Host},
    remote::ConnectOptions,
};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    sync::OnceLock,
    time::Duration,
};
static BINARY: OnceLock<PathBuf> = OnceLock::new();
pub struct Server {
    pub dir: tempfile::TempDir,
    pub port: u16,
    pub child: Process,
}
impl Server {
    pub fn new(limit: usize) -> Self {
        let binary = BINARY.get_or_init(|| {
            let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .find(|path| path.join("go.mod").is_file())
                .unwrap()
                .to_owned();
            let temp = tempfile::tempdir().unwrap().keep();
            let binary = temp.join("ftp-server");
            let result = Command::new("go")
                .args(["build", "-o"])
                .arg(&binary)
                .arg("./testdata/ftp-server")
                .current_dir(repo)
                .env("GOCACHE", temp.join("cache"))
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "build real FTP server: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            binary
        });
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("files")).unwrap();
        let mut child = Process(
            Command::new(binary)
                .arg(dir.path().join("files"))
                .arg(limit.to_string())
                .arg(dir.path())
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
            .expect("FTP daemon did not return a port");
        Self { dir, port, child }
    }
    pub fn host(&self) -> Host {
        Host {
            name: "Test FTP".into(),
            hostname: "127.0.0.1".into(),
            port: self.port,
            user: "testuser".into(),
            protocol: "ftp".into(),
            root_path: "/".into(),
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
            known_hosts: self.dir.path().join("unused_known_hosts"),
            agent_socket: None,
            timeout: Duration::from_secs(5),
        }
    }
    pub fn flag(&self, name: &str, value: &str) {
        fs::write(self.dir.path().join(name), value).unwrap();
    }
    pub fn commands(&self, name: &str) -> usize {
        fs::read_to_string(self.dir.path().join("commands"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.split_whitespace().next() == Some(name))
            .count()
    }
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
