//! Native FTP. A lease reserves one control connection through the final reply.
mod metadata;
mod transfer;
use crate::{
    config::{Host, expand_env},
    error::{Error, Result},
    remote::{
        ConnectOptions, ConnectionState, RemoteClient, RemoteEntry, RemoteMetadata, RemoteRead,
    },
};
use async_trait::async_trait;
use std::{
    net::Shutdown,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
use suppaftp::{FtpError, Mode, Status, tokio::AsyncFtpStream, types::FileType};
use tokio::{
    io::AsyncReadExt,
    sync::{OwnedMutexGuard, OwnedSemaphorePermit, Semaphore, watch},
};
use tokio_util::sync::CancellationToken;

type Socket = Arc<Mutex<Option<std::net::TcpStream>>>;
struct Lifetime {
    stop: CancellationToken,
    state: watch::Sender<ConnectionState>,
    sockets: Mutex<Vec<Weak<Mutex<Option<std::net::TcpStream>>>>>,
}
impl Lifetime {
    fn terminate(&self, state: ConnectionState) {
        let sockets = self.sockets.lock().unwrap();
        if self.stop.is_cancelled() {
            return;
        }
        self.state.send_replace(state);
        self.stop.cancel();
        for socket in sockets.iter().filter_map(Weak::upgrade) {
            if let Some(socket) = socket.lock().unwrap().take() {
                let _ = socket.shutdown(Shutdown::Both);
            }
        }
    }
    fn check(&self) -> Result<()> {
        match &*self.state.borrow() {
            ConnectionState::Connected => Ok(()),
            ConnectionState::Closed => Err(Error::Connection("FTP connection closed".into())),
            ConnectionState::Failed(message) => Err(Error::Connection(message.clone())),
        }
    }
    fn slot(&self) -> Socket {
        let socket = Arc::new(Mutex::new(None));
        self.sockets.lock().unwrap().push(Arc::downgrade(&socket));
        socket
    }
    async fn dial(
        &self,
        address: std::net::SocketAddr,
        slot: &Socket,
    ) -> std::io::Result<tokio::net::TcpStream> {
        let stream = tokio::select! {
            biased;
            _ = self.stop.cancelled() => return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "FTP connection closed")),
            stream = tokio::net::TcpStream::connect(address) => stream?,
        }.into_std()?;
        let _sockets = self.sockets.lock().unwrap();
        if self.stop.is_cancelled() {
            let _ = stream.shutdown(Shutdown::Both);
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "FTP connection closed",
            ));
        }
        *slot.lock().unwrap() = Some(stream.try_clone()?);
        tokio::net::TcpStream::from_std(stream)
    }
}
struct Connection {
    ftp: AsyncFtpStream,
    last_activity: Instant,
    _control: Socket,
}
struct Lease {
    // Release the command lock before releasing the capacity reservation.
    connection: OwnedMutexGuard<Connection>,
    _permit: OwnedSemaphorePermit,
    life: Arc<Lifetime>,
    completed: bool,
}
impl Lease {
    fn complete<T>(&mut self, result: Result<T>) -> Result<T> {
        self.completed = true;
        self.connection.last_activity = Instant::now();
        if let Err(Error::Connection(message)) = &result {
            self.life
                .terminate(ConnectionState::Failed(message.clone()));
        }
        result
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.completed {
            self.life.terminate(ConnectionState::Failed(
                "FTP operation abandoned before completion; reconnect before comparing".into(),
            ));
        }
    }
}
pub struct FtpClient {
    connections: Arc<Vec<Arc<tokio::sync::Mutex<Connection>>>>,
    capacity: Arc<Semaphore>,
    life: Arc<Lifetime>,
}
impl Drop for FtpClient {
    fn drop(&mut self) {
        self.life.terminate(ConnectionState::Closed);
    }
}
struct Setup(Option<Arc<Lifetime>>);
impl Drop for Setup {
    fn drop(&mut self) {
        if let Some(life) = &self.0 {
            life.terminate(ConnectionState::Closed);
        }
    }
}
fn ftp_error(error: FtpError) -> Error {
    let terminal = matches!(
        &error,
        FtpError::ConnectionError(_) | FtpError::BadResponse | FtpError::DataConnectionAlreadyOpen
    ) || matches!(&error, FtpError::UnexpectedResponse(response) if matches!(response.status.code(), 421 | 426));
    if terminal {
        Error::Connection(format!("FTP: {error}"))
    } else {
        Error::Invalid(format!("FTP: {error}"))
    }
}
fn unsupported(error: &FtpError) -> bool {
    matches!(error, FtpError::UnexpectedResponse(response) if matches!(response.status.code(), 500 | 501 | 502 | 504))
}
fn command_value(value: &str) -> Result<()> {
    if value.contains(['\r', '\n', '\0']) {
        return Err(Error::Invalid(
            "FTP command contains a forbidden control character".into(),
        ));
    }
    Ok(())
}
pub async fn connect(
    host: Host,
    options: ConnectOptions,
    cancel: CancellationToken,
) -> Result<FtpClient> {
    let (state, _) = watch::channel(ConnectionState::Connected);
    let life = Arc::new(Lifetime {
        stop: CancellationToken::new(),
        state,
        sockets: Mutex::new(vec![]),
    });
    let mut setup = Setup(Some(life.clone()));
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(Error::Invalid("FTP connection cancelled".into())),
        result = tokio::time::timeout(options.timeout, establish(host, life)) => result.map_err(|_| Error::Invalid("FTP connection/authentication timed out".into()))?,
    };
    // The successful client owns shutdown now. No setup cancellation escapes.
    if result.is_ok() {
        setup.0 = None;
    }
    result
}
async fn establish(host: Host, life: Arc<Lifetime>) -> Result<FtpClient> {
    let hostname = host.hostname.trim_matches(['[', ']']);
    if hostname.is_empty() || host.user.is_empty() {
        return Err(Error::Invalid("FTP hostname and user are required".into()));
    }
    let password = expand_env(&host.auth.password);
    command_value(&host.user)?;
    command_value(&password)?;
    let port = if host.port == 0 { 21 } else { host.port };
    let mut addresses: Vec<_> = tokio::net::lookup_host((hostname, port)).await?.collect();
    addresses.sort_by_key(|address| !address.is_ipv4());
    let mut primary = None;
    let mut last = None;
    for address in addresses {
        let slot = life.slot();
        match life.dial(address, &slot).await {
            Ok(stream) => {
                primary = Some((address, slot, stream));
                break;
            }
            Err(error) => last = Some(error),
        }
    }
    let (address, slot, stream) = primary.ok_or_else(|| {
        last.map(Error::Io)
            .unwrap_or_else(|| Error::Invalid("FTP endpoint has no addresses".into()))
    })?;
    let mut connections = vec![Arc::new(tokio::sync::Mutex::new(
        login(stream, slot, &host.user, &password, life.clone())
            .await
            .map_err(ftp_error)?,
    ))];
    while connections.len() < 4 {
        let slot = life.slot();
        let stream = life.dial(address, &slot).await?;
        match login(stream, slot, &host.user, &password, life.clone()).await {
            Ok(connection) => connections.push(Arc::new(tokio::sync::Mutex::new(connection))),
            Err(FtpError::UnexpectedResponse(response))
                if matches!(response.status.code(), 421 | 530) =>
            {
                break;
            }
            Err(error) => return Err(ftp_error(error)),
        }
    }
    let client = FtpClient {
        capacity: Arc::new(Semaphore::new(connections.len())),
        connections: Arc::new(connections),
        life,
    };
    if host.keep_alive_seconds() > 0 {
        client.monitor(Duration::from_secs(host.keep_alive_seconds()));
    }
    Ok(client)
}
async fn login(
    stream: tokio::net::TcpStream,
    slot: Socket,
    user: &str,
    password: &str,
    life: Arc<Lifetime>,
) -> std::result::Result<Connection, FtpError> {
    let data = life.slot();
    let mut ftp = AsyncFtpStream::connect_with_stream(stream)
        .await?
        .passive_stream_builder(move |address| {
            let life = life.clone();
            let data = data.clone();
            Box::pin(async move {
                life.dial(address, &data)
                    .await
                    .map_err(FtpError::ConnectionError)
            })
        });
    // Preserve status for adaptive worker admission, without a credential reply.
    ftp.login(user, password)
        .await
        .map_err(|error| match error {
            FtpError::UnexpectedResponse(response) => FtpError::UnexpectedResponse(
                suppaftp::types::Response::new(response.status, b"FTP login rejected".to_vec()),
            ),
            error => error,
        })?;
    ftp.transfer_type(FileType::Binary).await?;
    ftp.set_mode(Mode::ExtendedPassive);
    // Some servers implement only PASV; capability fallback is handled before
    // transfers, rather than repeating a transfer command after a failure.
    if let Err(error) = ftp
        .custom_command("EPSV", &[Status::ExtendedPassiveMode])
        .await
    {
        if unsupported(&error) {
            ftp.set_mode(Mode::Passive);
            ftp.set_passive_nat_workaround(true);
        } else {
            return Err(error);
        }
    }
    Ok(Connection {
        ftp,
        last_activity: Instant::now(),
        _control: slot,
    })
}
impl FtpClient {
    async fn lease(&self) -> Result<Lease> {
        self.life.check()?;
        let permit = tokio::select! {
            biased;
            _ = self.life.stop.cancelled() => return Err(Error::Connection("FTP connection closed".into())),
            permit = self.capacity.clone().acquire_owned() => permit.map_err(|_| Error::Connection("FTP connection closed".into()))?,
        };
        self.life.check()?;
        for connection in self.connections.iter() {
            if let Ok(connection) = connection.clone().try_lock_owned() {
                return Ok(Lease {
                    connection,
                    _permit: permit,
                    life: self.life.clone(),
                    completed: false,
                });
            }
        }
        Err(Error::Connection(
            "FTP pool capacity invariant violated".into(),
        ))
    }
    fn monitor(&self, interval: Duration) {
        let connections = self.connections.clone();
        let capacity = self.capacity.clone();
        let life = self.life.clone();
        tokio::spawn(async move {
            let mut ticks = tokio::time::interval(interval);
            ticks.tick().await;
            loop {
                tokio::select! { biased; _ = life.stop.cancelled() => return, _ = ticks.tick() => {} }
                for connection in connections.iter() {
                    let Ok(permit) = capacity.clone().try_acquire_owned() else {
                        break;
                    };
                    let Ok(connection) = connection.clone().try_lock_owned() else {
                        continue;
                    };
                    let mut lease = Lease {
                        connection,
                        _permit: permit,
                        life: life.clone(),
                        completed: false,
                    };
                    if lease.connection.last_activity.elapsed() < interval {
                        lease.completed = true;
                        continue;
                    }
                    let result = tokio::select! {
                        biased;
                        _ = life.stop.cancelled() => return,
                        result = tokio::time::timeout(Duration::from_secs(15), lease.connection.ftp.noop()) => match result {
                            Ok(result) => result.map_err(|error| Error::Connection(format!("FTP keep-alive: {error}"))),
                            Err(_) => Err(Error::Connection("FTP keep-alive timed out".into())),
                        },
                    };
                    if lease.complete(result).is_err() {
                        return;
                    }
                }
            }
        });
    }
}
#[async_trait]
impl RemoteClient for FtpClient {
    async fn stat(&self, path: &str) -> Result<RemoteMetadata> {
        command_value(path)?;
        let mut lease = self.lease().await?;
        let result = metadata::stat(&mut lease.connection.ftp, path).await;
        lease.complete(result)
    }
    async fn read_dir(&self, path: &str) -> Result<Vec<RemoteEntry>> {
        command_value(path)?;
        let mut lease = self.lease().await?;
        let result = metadata::directory(&mut lease.connection.ftp, path).await;
        lease.complete(result)
    }
    async fn canonicalize(&self, path: &str) -> Result<String> {
        command_value(path)?;
        let mut lease = self.lease().await?;
        let result = async {
            lease.connection.ftp.cwd(path).await.map_err(ftp_error)?;
            let path = lease.connection.ftp.pwd().await.map_err(ftp_error)?;
            command_value(&path)?;
            if !path.starts_with('/')
                || std::path::Path::new(&path)
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(Error::Invalid(
                    "FTP server returned an invalid working directory".into(),
                ));
            }
            // Every connection uses absolute paths after this root resolution.
            Ok(path.trim_end_matches('/').to_owned())
                .map(|p| if p.is_empty() { "/".into() } else { p })
        }
        .await;
        lease.complete(result)
    }
    async fn open(&self, path: &str) -> Result<Box<dyn RemoteRead>> {
        self.open_stream(path).await
    }
    async fn upload(&self, path: &str, source: Box<dyn RemoteRead>) -> Result<()> {
        self.upload_staged(path, source).await
    }
    async fn delete(&self, path: &str) -> Result<()> {
        command_value(path)?;
        let mut lease = self.lease().await?;
        let result = lease.connection.ftp.rm(path).await.map_err(ftp_error);
        lease.complete(result)
    }
    async fn read_limited(&self, path: &str, limit: usize) -> Result<Vec<u8>> {
        let metadata = self.stat(path).await?;
        if !metadata.regular || metadata.size > limit as u64 {
            return Err(Error::Invalid(
                "preview requires a regular remote file of at most 1 MiB".into(),
            ));
        }
        let mut stream = self.open(path).await?;
        let mut bytes = vec![];
        let read = (&mut stream)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(Error::Io);
        let close = stream.close().await;
        if let Err(error) = read {
            return Err(Error::join(error, close.err()));
        }
        close?;
        if bytes.len() > limit {
            return Err(Error::Invalid("remote preview exceeds size limit".into()));
        }
        Ok(bytes)
    }
    fn state(&self) -> watch::Receiver<ConnectionState> {
        self.life.state.subscribe()
    }
    async fn shutdown(&self) {
        self.life.terminate(ConnectionState::Closed);
    }
}
