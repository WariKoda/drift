//! Native SSH/SFTP transport; the monitor outlives the connect operation.
mod io;
mod known_hosts;
mod transfer;
use crate::{
    config::{Host, expand_env},
    error::{Error, Result},
    remote::{
        ConnectOptions, ConnectionState, RemoteClient, RemoteEntry, RemoteMetadata, RemoteRead,
    },
};
use async_trait::async_trait;
use io::{Handle, ReadFile, TransferLimits};
use russh::{
    client,
    keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate, agent::client::AgentClient},
};
use russh_sftp::{
    client::{Config, RawSftpSession, rawsession::Limits},
    protocol::StatusCode,
};
use std::{net::Shutdown, sync::Arc, time::Duration};
use tokio::{io::AsyncReadExt, sync::watch};
use tokio_util::sync::CancellationToken;

struct Verifier {
    options: ConnectOptions,
    names: Vec<String>,
}
impl client::Handler for Verifier {
    type Error = Error;
    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        if key.certificate().is_some() {
            return Err(Error::Invalid(
                "SSH host certificate verification is not implemented yet".into(),
            ));
        }
        let path = self.options.known_hosts.clone();
        let names = self.names.clone();
        let key = key.public_key();
        tokio::task::spawn_blocking(move || known_hosts::verify(&path, &names, &key))
            .await
            .map_err(|e| Error::Invalid(format!("known_hosts task: {e}")))??;
        Ok(true)
    }
}
struct SocketGuard(std::net::TcpStream);
impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}
pub struct SftpClient {
    sftp: Arc<RawSftpSession>,
    posix_rename: bool,
    fsync: bool,
    limits: TransferLimits,
    stop: CancellationToken,
    state: watch::Receiver<ConnectionState>,
}
impl Drop for SftpClient {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
impl From<russh::Error> for Error {
    fn from(e: russh::Error) -> Self {
        Error::Invalid(format!("SSH: {e}"))
    }
}
fn sftp_error(e: russh_sftp::client::error::Error) -> Error {
    use russh_sftp::client::error::Error as SftpError;
    if matches!(
        e,
        SftpError::IO(_)
            | SftpError::Timeout
            | SftpError::UnexpectedPacket
            | SftpError::UnexpectedBehavior(_)
    ) {
        return Error::Connection(e.to_string());
    }
    if let russh_sftp::client::error::Error::Status(status) = &e {
        let kind = match status.status_code {
            russh_sftp::protocol::StatusCode::NoSuchFile => Some(std::io::ErrorKind::NotFound),
            russh_sftp::protocol::StatusCode::PermissionDenied => {
                Some(std::io::ErrorKind::PermissionDenied)
            }
            russh_sftp::protocol::StatusCode::NoConnection
            | russh_sftp::protocol::StatusCode::ConnectionLost
            | russh_sftp::protocol::StatusCode::BadMessage => {
                return Error::Connection(e.to_string());
            }
            _ => None,
        };
        if let Some(kind) = kind {
            return Error::Io(std::io::Error::new(kind, e));
        }
    }
    Error::Invalid(format!("SFTP: {e}"))
}
fn operation_error(stop: &CancellationToken, error: russh_sftp::client::error::Error) -> Error {
    let error = sftp_error(error);
    if matches!(error, Error::Connection(_)) {
        stop.cancel();
    }
    error
}
impl SftpClient {
    fn check_connected(&self) -> Result<()> {
        if self.stop.is_cancelled() || *self.state.borrow() != ConnectionState::Connected {
            self.stop.cancel();
            return Err(Error::Connection("SFTP connection closed".into()));
        }
        Ok(())
    }
    async fn request<T>(
        &self,
        request: impl std::future::Future<
            Output = std::result::Result<T, russh_sftp::client::error::Error>,
        >,
    ) -> Result<T> {
        self.check_connected()?;
        request
            .await
            .map_err(|error| operation_error(&self.stop, error))
    }
}
fn transfer_io_error(e: std::io::Error) -> Error {
    if let Some(error) = e
        .get_ref()
        .and_then(|e| e.downcast_ref::<russh_sftp::client::error::Error>())
    {
        return sftp_error(error.clone());
    }
    if matches!(
        e.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::TimedOut
    ) {
        return Error::Connection(e.to_string());
    }
    Error::Io(e)
}

pub async fn connect(
    host: Host,
    options: ConnectOptions,
    cancel: CancellationToken,
) -> Result<SftpClient> {
    let timeout = options.timeout;
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(Error::Invalid("connection cancelled".into())),
        result = tokio::time::timeout(timeout, establish(host, options)) => result.map_err(|_| Error::Invalid("SSH connection/authentication timed out".into()))?,
    }
}
async fn establish(host: Host, options: ConnectOptions) -> Result<SftpClient> {
    let hostname = host.hostname.trim_matches(['[', ']']);
    if hostname.is_empty() || host.user.is_empty() {
        return Err(Error::Invalid("SSH hostname and user are required".into()));
    }
    let port = if host.port == 0 { 22 } else { host.port };
    let mut addresses: Vec<_> = tokio::net::lookup_host((hostname, port)).await?.collect();
    addresses.sort_by_key(|a| !a.is_ipv4());
    let mut last = None;
    let mut connected = None;
    for address in addresses {
        match tokio::net::TcpStream::connect(address).await {
            Ok(stream) => {
                connected = Some(stream);
                break;
            }
            Err(error) => last = Some(error),
        }
    }
    let stream = connected.ok_or_else(|| {
        last.map(Error::Io)
            .unwrap_or_else(|| Error::Invalid("SSH endpoint has no addresses".into()))
    })?;
    let stream = stream.into_std()?;
    let guard = SocketGuard(stream.try_clone()?);
    let stream = tokio::net::TcpStream::from_std(stream)?;
    let mut config = client::Config::default();
    // Go verifies the requested hostname; an IP entry must not override it.
    let names = vec![known_hosts::endpoint(hostname, port)];
    let path = options.known_hosts.clone();
    let host_names = names.clone();
    let mut preferred =
        tokio::task::spawn_blocking(move || known_hosts::preferred(&path, &host_names))
            .await
            .map_err(|e| Error::Invalid(format!("known_hosts task: {e}")))??;
    if !preferred.is_empty() {
        for algorithm in config.preferred.key.iter() {
            if !preferred.contains(algorithm) {
                preferred.push(algorithm.clone());
            }
        }
        config.preferred.key = std::borrow::Cow::Owned(preferred);
    }
    let mut ssh = client::connect_stream(
        Arc::new(config),
        stream,
        Verifier {
            options: options.clone(),
            names,
        },
    )
    .await?;
    let authenticated = match host.auth.kind.as_str() {
        "password" => ssh
            .authenticate_password(&host.user, expand_env(&host.auth.password))
            .await?
            .success(),
        "keyfile" if !host.auth.key_file.is_empty() => {
            let mut path = expand_env(&host.auth.key_file);
            if let Some(relative) = path.strip_prefix("~/") {
                path = std::env::var("HOME")
                    .map_err(|_| Error::Invalid("HOME is required to expand key path".into()))?
                    + "/"
                    + relative;
            }
            let pass = expand_env(&host.auth.passphrase);
            let key = tokio::task::spawn_blocking(move || {
                use std::os::unix::fs::OpenOptionsExt;
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(path)
                    .map_err(|e| Error::Invalid(format!("read SSH key: {e}")))?;
                if !file.metadata()?.is_file() {
                    return Err(Error::Invalid("SSH key must be a regular file".into()));
                }
                let mut reader = std::io::Read::take(file, 1024 * 1024 + 1);
                let mut text = String::new();
                std::io::Read::read_to_string(&mut reader, &mut text)?;
                if text.len() > 1024 * 1024 {
                    return Err(Error::Invalid("SSH key file exceeds size limit".into()));
                }
                russh::keys::decode_secret_key(&text, (!pass.is_empty()).then_some(pass.as_str()))
                    .map_err(|_| Error::Invalid("cannot decrypt or parse SSH key".into()))
            })
            .await
            .map_err(|e| Error::Invalid(format!("SSH key task: {e}")))?
            .map_err(|_| {
                Error::Invalid(
                    "cannot read or decrypt SSH key file; check path and passphrase".into(),
                )
            })?;
            let hash = ssh.best_supported_rsa_hash().await?.flatten();
            ssh.authenticate_publickey(&host.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await?
                .success()
        }
        "" | "agent" | "keyfile" => {
            let socket = options.agent_socket.ok_or_else(|| {
                Error::Invalid("SSH_AUTH_SOCK not set and no keyfile configured".into())
            })?;
            let mut agent = AgentClient::connect_uds(socket)
                .await
                .map_err(|_| Error::Invalid("cannot connect to SSH agent".into()))?;
            let keys = agent
                .request_identities()
                .await
                .map_err(|_| Error::Invalid("cannot list SSH agent keys".into()))?;
            let hash = ssh.best_supported_rsa_hash().await?.flatten();
            let mut success = false;
            for identity in keys {
                if ssh
                    .authenticate_publickey_with(
                        &host.user,
                        identity.public_key().into_owned(),
                        hash,
                        &mut agent,
                    )
                    .await
                    .map_err(|_| Error::Invalid("SSH agent authentication failed".into()))?
                    .success()
                {
                    success = true;
                    break;
                }
            }
            success
        }
        _ => return Err(Error::Invalid("unknown SSH authentication type".into())),
    };
    if !authenticated {
        return Err(Error::Invalid("SSH authentication rejected".into()));
    }
    let channel = ssh.channel_open_session().await?;
    channel.request_subsystem(true, "sftp").await?;
    let config = Config::default();
    let mut sftp = RawSftpSession::new_with_config(channel.into_stream(), config.clone());
    let version = sftp.init().await.map_err(sftp_error)?;
    let advertised = |name| version.extensions.get(name).is_some_and(|v| v == "1");
    let posix_rename = advertised("posix-rename@openssh.com");
    let fsync = advertised("fsync@openssh.com");
    let mut server_limits = Limits::default();
    if advertised("limits@openssh.com") {
        match sftp.limits().await {
            Ok(limits) => server_limits = limits.into(),
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == StatusCode::OpUnsupported => {}
            Err(error) => return Err(sftp_error(error)),
        }
    }
    let limits = TransferLimits::new(&config, server_limits)?;
    // Raw send checks framed request sizes; the reader adapter also checks
    // response sizes and handle-dependent overhead before issuing file I/O.
    server_limits.packet_len = Some(
        server_limits
            .packet_len
            .unwrap_or(u64::from(config.max_packet_len))
            .min(u64::from(config.max_packet_len)),
    );
    sftp.set_limits(server_limits);
    let sftp = Arc::new(sftp);
    let stop = CancellationToken::new();
    let (state, receiver) = watch::channel(ConnectionState::Connected);
    let interval = host.keep_alive_seconds();
    let monitor_stop = stop.clone();
    let monitor_sftp = sftp.clone();
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(Duration::from_secs(interval.max(1)));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticks.tick().await;
        let (mut terminal, joined) = loop {
            tokio::select! {
                biased;
                _ = monitor_stop.cancelled() => break (ConnectionState::Closed, false),
                result = &mut ssh => break (ConnectionState::Failed(result.err().map_or_else(|| "SSH connection closed".into(), |e| e.to_string())), true),
                _ = ticks.tick(), if interval > 0 => {
                    let result = tokio::select! {
                        _ = monitor_stop.cancelled() => break (ConnectionState::Closed, false),
                        result = tokio::time::timeout(Duration::from_secs(15), ssh.send_ping()) => result,
                    };
                    match result {
                        Ok(Ok(())) if !ssh.is_closed() => {},
                        _ => break (ConnectionState::Failed("SSH keep-alive failed or timed out".into()), false),
                    }
                }
            }
        };
        drop(guard); // Interrupt socket I/O before waiting for subsystem cleanup.
        if let Err(error) = monitor_sftp.close_session() {
            terminal = match terminal {
                ConnectionState::Failed(cause) => {
                    ConnectionState::Failed(format!("{cause}; SFTP close: {error}"))
                }
                _ => ConnectionState::Failed(format!("SFTP close: {error}")),
            };
        }
        if !joined {
            let _ = tokio::time::timeout(Duration::from_secs(2), ssh).await;
        }
        let _ = state.send(terminal);
    });
    Ok(SftpClient {
        sftp,
        posix_rename,
        fsync,
        limits,
        stop,
        state: receiver,
    })
}
#[async_trait]
impl RemoteClient for SftpClient {
    async fn upload(&self, path: &str, source: Box<dyn RemoteRead>) -> Result<()> {
        let result = self.upload_staged(path, source).await;
        if matches!(result, Err(Error::Connection(_))) {
            self.stop.cancel();
        }
        result
    }
    async fn delete(&self, path: &str) -> Result<()> {
        self.request(self.sftp.remove(path)).await.map(|_| ())
    }
    async fn stat(&self, path: &str) -> Result<RemoteMetadata> {
        let metadata = self.request(self.sftp.stat(path)).await?.attrs;
        Ok(RemoteMetadata {
            size: metadata.size.unwrap_or(0),
            modified: metadata
                .mtime
                .map(|seconds| std::time::UNIX_EPOCH + Duration::from_secs(seconds.into())),
            directory: metadata.file_type().is_dir(),
            regular: metadata.file_type().is_file(),
        })
    }
    async fn open(&self, path: &str) -> Result<Box<dyn RemoteRead>> {
        self.check_connected()?;
        Ok(Box::new(
            ReadFile::open(self.sftp.clone(), self.limits, path, self.stop.clone()).await?,
        ))
    }
    async fn canonicalize(&self, path: &str) -> Result<String> {
        self.request(self.sftp.realpath(path))
            .await?
            .files
            .into_iter()
            .next()
            .map(|file| file.filename)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                self.stop.cancel();
                Error::Connection("SFTP realpath returned no filename".into())
            })
    }
    async fn read_dir(&self, path: &str) -> Result<Vec<RemoteEntry>> {
        let value = self.request(self.sftp.opendir(path)).await?.handle;
        let mut handle = Handle::new(self.sftp.clone(), value, self.stop.clone());
        let read: Result<Vec<RemoteEntry>> = async {
            let mut entries = Vec::new();
            loop {
                self.check_connected()?;
                let batch = match self.sftp.readdir(handle.value()).await {
                    Ok(batch) => batch,
                    Err(russh_sftp::client::error::Error::Status(status))
                        if status.status_code == StatusCode::Eof =>
                    {
                        break;
                    }
                    Err(error) => return Err(operation_error(&self.stop, error)),
                };
                if batch.files.is_empty() {
                    self.stop.cancel();
                    return Err(Error::Connection(
                        "empty SFTP directory response without EOF".into(),
                    ));
                }
                for entry in batch.files {
                    let name = entry.filename;
                    if name == "." || name == ".." {
                        continue;
                    }
                    if name.is_empty() || name.contains('/') || name.contains('\0') {
                        return Err(Error::Invalid("invalid SFTP directory entry".into()));
                    }
                    let joined = if path.is_empty() {
                        name.clone()
                    } else if path.ends_with('/') {
                        format!("{path}{name}")
                    } else {
                        format!("{path}/{name}")
                    };
                    entries.push(RemoteEntry {
                        path: joined,
                        name,
                        directory: entry.attrs.file_type().is_dir(),
                        regular: entry.attrs.file_type().is_file(),
                        symlink: entry.attrs.file_type().is_symlink(),
                        size: entry.attrs.size.unwrap_or(0),
                    });
                }
            }
            Ok(entries)
        }
        .await;
        let close = handle.close().await;
        let mut entries = match (read, close) {
            (Ok(entries), Ok(())) => entries,
            (Err(error), Err(close)) => return Err(Error::join(error, [close])),
            (Err(error), _) | (_, Err(error)) => return Err(error),
        };
        entries.sort_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(entries)
    }
    async fn read_limited(&self, path: &str, limit: usize) -> Result<Vec<u8>> {
        self.check_connected()?;
        let bound = u64::try_from(limit)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| Error::Invalid("remote preview size limit overflow".into()))?;
        let metadata = self.request(self.sftp.lstat(path)).await?.attrs;
        if !metadata.file_type().is_file() || metadata.size.is_none_or(|size| size >= bound) {
            return Err(Error::Invalid(
                "preview requires a regular remote file with a size within the limit".into(),
            ));
        }
        let mut file = Box::new(
            ReadFile::open(self.sftp.clone(), self.limits, path, self.stop.clone()).await?,
        );
        let mut bytes = Vec::new();
        let read = (&mut file)
            .take(bound)
            .read_to_end(&mut bytes)
            .await
            .map_err(transfer_io_error);
        let close = file.close().await;
        match (read, close) {
            (Ok(_), Ok(())) => {}
            (Err(error), Err(close)) => return Err(Error::join(error, [close])),
            (Err(error), _) | (_, Err(error)) => return Err(error),
        }
        if bytes.len() > limit {
            return Err(Error::Invalid("remote preview exceeds size limit".into()));
        }
        Ok(bytes)
    }
    fn state(&self) -> watch::Receiver<ConnectionState> {
        self.state.clone()
    }
    async fn shutdown(&self) {
        self.stop.cancel();
        let mut state = self.state.clone();
        while *state.borrow_and_update() == ConnectionState::Connected {
            if state.changed().await.is_err() {
                break;
            }
        }
    }
}
