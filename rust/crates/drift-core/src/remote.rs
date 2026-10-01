//! Remote paths and transport lifetime. No local paths reach these clients.
use crate::{
    config::Host,
    error::{Error, Result},
};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug)]
pub struct RemoteMetadata {
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
    pub directory: bool,
    pub regular: bool,
}
/// Transfer completion is separate from EOF: callers must also check close.
#[async_trait]
pub trait RemoteRead: tokio::io::AsyncRead + Unpin + Send {
    async fn close(self: Box<Self>) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Closed,
    Failed(String),
}
#[derive(Clone, Debug)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub regular: bool,
    pub symlink: bool,
    pub size: u64,
}
#[async_trait]
pub trait RemoteClient: Send + Sync {
    async fn stat(&self, path: &str) -> Result<RemoteMetadata>;
    async fn open(&self, path: &str) -> Result<Box<dyn RemoteRead>>;
    /// Consumes and closes the source, including error paths, before commit.
    async fn upload(&self, path: &str, source: Box<dyn RemoteRead>) -> Result<()>;
    async fn delete(&self, path: &str) -> Result<()>;
    async fn canonicalize(&self, path: &str) -> Result<String>;
    async fn read_dir(&self, path: &str) -> Result<Vec<RemoteEntry>>;
    async fn read_limited(&self, path: &str, limit: usize) -> Result<Vec<u8>>;
    fn state(&self) -> watch::Receiver<ConnectionState>;
    async fn shutdown(&self);
}
#[derive(Clone)]
pub struct ConnectOptions {
    pub known_hosts: PathBuf,
    pub agent_socket: Option<PathBuf>,
    pub timeout: std::time::Duration,
}
impl ConnectOptions {
    pub fn from_environment() -> Result<Self> {
        let home = std::env::var_os("HOME")
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Error::Invalid("HOME is required for SSH known_hosts".into()))?;
        Ok(Self {
            known_hosts: PathBuf::from(home).join(".ssh/known_hosts"),
            agent_socket: std::env::var_os("SSH_AUTH_SOCK")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            timeout: std::time::Duration::from_secs(15),
        })
    }
}
pub async fn connect(
    host: Host,
    options: ConnectOptions,
    cancel: CancellationToken,
) -> Result<std::sync::Arc<dyn RemoteClient>> {
    match host.protocol.as_str() {
        "" | "sftp" => Ok(std::sync::Arc::new(
            crate::sftp::connect(host, options, cancel).await?,
        )),
        "ftp" => Ok(std::sync::Arc::new(
            crate::ftp::connect(host, options, cancel).await?,
        )),
        protocol => Err(Error::Invalid(format!(
            "{protocol} connections are not implemented yet"
        ))),
    }
}
