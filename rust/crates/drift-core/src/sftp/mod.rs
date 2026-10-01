//! Native SSH/SFTP transport; the monitor outlives the connect operation.
mod known_hosts;
mod transfer;
use crate::{
    config::Host,
    error::{Error, Result},
    remote::{
        ConnectOptions, ConnectionState, RemoteClient, RemoteEntry, RemoteMetadata, RemoteRead,
    },
};
use async_trait::async_trait;
use russh::{
    client,
    keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate, agent::client::AgentClient},
};
use russh_sftp::client::{RawSftpSession, SftpSession};
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
    sftp: Arc<SftpSession>,
    rename: Option<Arc<RawSftpSession>>,
    posix_rename: bool,
    rename_unavailable: Option<String>,
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
        SftpError::IO(_) | SftpError::Timeout | SftpError::UnexpectedBehavior(_)
    ) {
        return Error::Connection(e.to_string());
    }
    if let russh_sftp::client::error::Error::Status(status) = &e {
        let kind = match status.status_code {
            russh_sftp::protocol::StatusCode::NoSuchFile => Some(std::io::ErrorKind::NotFound),
            russh_sftp::protocol::StatusCode::PermissionDenied => {
                Some(std::io::ErrorKind::PermissionDenied)
            }
            _ => None,
        };
        if let Some(kind) = kind {
            return Error::Io(std::io::Error::new(kind, e));
        }
    }
    Error::Invalid(format!("SFTP: {e}"))
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
    let sftp = Arc::new(
        SftpSession::new(channel.into_stream())
            .await
            .map_err(sftp_error)?,
    );
    // The pinned high-level API doesn't expose SSH_FXP_EXTENDED. A second
    // subsystem channel on the same authenticated SSH connection handles
    // POSIX rename; there is no additional login or separate connection.
    let control: Result<_> = async {
        let channel = ssh.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        let rename = RawSftpSession::new(channel.into_stream());
        let version = rename.init().await.map_err(sftp_error)?;
        let supported = version
            .extensions
            .get("posix-rename@openssh.com")
            .is_some_and(|v| v == "1");
        Ok((Arc::new(rename), supported))
    }
    .await;
    let (rename, posix_rename, rename_unavailable) = match control {
        Ok((rename, supported)) => (Some(rename), supported, None),
        Err(Error::Connection(error)) => return Err(Error::Connection(error)),
        Err(error) => (None, false, Some(error.to_string())),
    };
    let stop = CancellationToken::new();
    let (state, receiver) = watch::channel(ConnectionState::Connected);
    let interval = host.keep_alive_seconds();
    let monitor_stop = stop.clone();
    let monitor_sftp = sftp.clone();
    let monitor_rename = rename.clone();
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(Duration::from_secs(interval.max(1)));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticks.tick().await;
        let (terminal, joined) = loop {
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
        if let Some(rename) = monitor_rename {
            let _ = rename.close_session();
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), monitor_sftp.close()).await;
        if !joined {
            let _ = tokio::time::timeout(Duration::from_secs(2), ssh).await;
        }
        let _ = state.send(terminal);
    });
    Ok(SftpClient {
        sftp,
        rename,
        posix_rename,
        rename_unavailable,
        stop,
        state: receiver,
    })
}
fn expand_env(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            result.push(c);
            continue;
        }
        let mut name = String::new();
        if chars.peek() == Some(&'{') {
            chars.next();
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                }
                name.push(c);
            }
        } else {
            while chars
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
            {
                name.push(chars.next().unwrap());
            }
        }
        if name.is_empty() {
            result.push('$');
        } else {
            result.push_str(&std::env::var(name).unwrap_or_default());
        }
    }
    result
}
#[async_trait]
impl RemoteRead for russh_sftp::client::fs::File {
    async fn close(self: Box<Self>) -> Result<()> {
        (*self).close().await.map_err(transfer_io_error)
    }
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
        self.sftp.remove_file(path).await.map_err(sftp_error)
    }
    async fn stat(&self, path: &str) -> Result<RemoteMetadata> {
        let metadata = self.sftp.metadata(path).await.map_err(sftp_error)?;
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
        Ok(Box::new(self.sftp.open(path).await.map_err(sftp_error)?))
    }
    async fn canonicalize(&self, path: &str) -> Result<String> {
        self.sftp.canonicalize(path).await.map_err(sftp_error)
    }
    async fn read_dir(&self, path: &str) -> Result<Vec<RemoteEntry>> {
        let mut entries = Vec::new();
        for entry in self.sftp.read_dir(path).await.map_err(sftp_error)? {
            let name = entry.file_name();
            if name.is_empty() || name.contains('/') || name.contains('\0') {
                return Err(Error::Invalid("invalid SFTP directory entry".into()));
            }
            entries.push(RemoteEntry {
                path: entry.path(),
                name,
                directory: entry.file_type().is_dir(),
                regular: entry.file_type().is_file(),
                symlink: entry.file_type().is_symlink(),
                size: entry.metadata().size.unwrap_or(0),
            });
        }
        entries.sort_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(entries)
    }
    async fn read_limited(&self, path: &str, limit: usize) -> Result<Vec<u8>> {
        let metadata = self.sftp.symlink_metadata(path).await.map_err(sftp_error)?;
        if !metadata.file_type().is_file() || metadata.size.is_some_and(|size| size > limit as u64)
        {
            return Err(Error::Invalid(
                "preview requires a regular remote file of at most 1 MiB".into(),
            ));
        }
        let mut file = self.sftp.open(path).await.map_err(sftp_error)?;
        let mut bytes = Vec::new();
        let read = (&mut file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .await;
        let close = file.close().await;
        read?;
        close?;
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
