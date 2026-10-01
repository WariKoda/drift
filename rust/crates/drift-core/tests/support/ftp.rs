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
    tls: bool,
}
impl Server {
    pub fn new(limit: usize) -> Self {
        Self::start(limit, None)
    }
    pub fn tls(mode: &str) -> Self {
        Self::tls_with_limit(mode, 4)
    }
    pub fn tls_with_limit(mode: &str, limit: usize) -> Self {
        Self::start(limit, Some(mode))
    }
    fn start(limit: usize, mode: Option<&str>) -> Self {
        let binary = BINARY.get_or_init(|| {
            let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .find(|path| path.join("go.mod").is_file())
                .unwrap()
                .to_owned();
            let temp = tempfile::tempdir().unwrap().keep();
            let cache = tempfile::tempdir().unwrap();
            let binary = temp.join("ftp-server");
            let result = Command::new("go")
                .args(["build", "-o"])
                .arg(&binary)
                .arg("./testdata/ftp-server")
                .current_dir(repo)
                .env("GOCACHE", cache.path())
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
        let mut command = Command::new(binary);
        command
            .arg(dir.path().join("files"))
            .arg(limit.to_string())
            .arg(dir.path());
        if let Some(mode) = mode {
            command.arg(mode);
        }
        let mut child = Process(command.stdout(Stdio::piped()).spawn().unwrap());
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port = line
            .trim()
            .parse()
            .expect("FTP daemon did not return a port");
        Self {
            dir,
            port,
            child,
            tls: mode.is_some(),
        }
    }
    pub fn host(&self) -> Host {
        Host {
            name: "Test FTP".into(),
            hostname: "127.0.0.1".into(),
            port: self.port,
            user: "testuser".into(),
            protocol: if self.tls { "ftps" } else { "ftp" }.into(),
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
    pub fn roots(&self) -> rustls::RootCertStore {
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(
                fs::read(self.dir.path().join("root.der")).unwrap(),
            ))
            .unwrap();
        roots
    }
    pub fn options(&self) -> ConnectOptions {
        ConnectOptions {
            known_hosts: self.dir.path().join("unused_known_hosts"),
            agent_socket: None,
            timeout: Duration::from_secs(5),
            tls: None,
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
    /// Observe actual server-side socket cleanup before a capacity-limited
    /// fixture is reused. Client shutdown does not await the peer's scheduler.
    pub async fn wait_idle(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.commands("OPEN") != self.commands("CLOSED") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("FTP server did not release closed sessions");
    }
}
