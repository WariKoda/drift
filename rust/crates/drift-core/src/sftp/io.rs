//! Bounded handle and streaming adapters for the SDK's public raw session.
use super::{Error, RawSftpSession, RemoteRead, Result, operation_error, sftp_error};
use async_trait::async_trait;
use russh_sftp::{
    client::{Config, error::Error as SftpError, rawsession::Limits},
    protocol::{FileAttributes, OpenFlags, StatusCode},
};
use std::{
    future::Future,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker, ready},
};
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

const CHUNK: u64 = 32 * 1024;
// Framed DATA response, and READ/WRITE request excluding the handle bytes.
const DATA_OVERHEAD: u64 = 13;
const IO_OVERHEAD: u64 = 25;

#[derive(Clone, Copy)]
pub(super) struct TransferLimits {
    packet: u64,
    read: u64,
    write: u64,
    write_packet: u64,
}
impl TransferLimits {
    pub(super) fn new(config: &Config, limits: Limits) -> Result<Self> {
        // OpenSSH zero limits mean unlimited; absent limits use bounded defaults.
        // Clamp in u64 before conversion so oversized advertised values cannot wrap.
        let packet = limits
            .packet_len
            .filter(|n| *n > 0)
            .unwrap_or(u64::from(config.max_packet_len))
            .min(u64::from(config.max_packet_len));
        let read = limits
            .read_len
            .filter(|n| *n > 0)
            .unwrap_or(CHUNK)
            .min(CHUNK);
        let write = limits
            .write_len
            .filter(|n| *n > 0)
            .unwrap_or(CHUNK)
            .min(CHUNK);
        let write_packet = u64::from(config.max_write_packet_len).min(packet);
        if packet <= IO_OVERHEAD || write_packet <= IO_OVERHEAD {
            return Err(Error::Invalid(
                "SFTP packet limits are too small for file I/O".into(),
            ));
        }
        Ok(Self {
            packet,
            read,
            write,
            write_packet,
        })
    }
    pub(super) fn read_len(&self, handle: &str) -> Result<u32> {
        let overhead = IO_OVERHEAD
            .checked_add(
                u64::try_from(handle.len())
                    .map_err(|_| Error::Connection("SFTP handle length overflow".into()))?,
            )
            .ok_or_else(|| Error::Connection("SFTP handle length overflow".into()))?;
        if overhead > self.packet {
            return Err(Error::Connection("SFTP handle exceeds packet limit".into()));
        }
        let len = self.read.min(self.packet - DATA_OVERHEAD);
        u32::try_from(len)
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| Error::Invalid("SFTP read limit is zero or unrepresentable".into()))
    }
    pub(super) fn write_len(&self, handle: &str) -> Result<usize> {
        let overhead = IO_OVERHEAD
            .checked_add(
                u64::try_from(handle.len())
                    .map_err(|_| Error::Connection("SFTP handle length overflow".into()))?,
            )
            .ok_or_else(|| Error::Connection("SFTP handle length overflow".into()))?;
        let len = self.write_packet.saturating_sub(overhead).min(self.write);
        usize::try_from(len)
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| Error::Invalid("SFTP packet limit leaves no room for write data".into()))
    }
}

pub(super) struct Handle {
    session: Arc<RawSftpSession>,
    value: Option<String>,
    stop: CancellationToken,
}
impl Handle {
    pub(super) fn new(
        session: Arc<RawSftpSession>,
        value: String,
        stop: CancellationToken,
    ) -> Self {
        Self {
            session,
            value: Some(value),
            stop,
        }
    }
    pub(super) fn value(&self) -> &str {
        self.value.as_deref().expect("SFTP handle is open")
    }
    pub(super) async fn close(&mut self) -> Result<()> {
        let Some(value) = self.value.clone() else {
            return Ok(());
        };
        let session = self.session.clone();
        let mut close = std::pin::pin!(session.close(value));
        let result = std::future::poll_fn(|cx| {
            let result = close.as_mut().poll(cx);
            // The SDK queues CLOSE synchronously on its first poll. Until then,
            // retain the handle so cancellation still leaves Drop able to close it.
            // Pending means queued: discarding that ACK is safe, as in SDK File Drop.
            if matches!(result, Poll::Ready(Err(_))) {
                self.stop.cancel();
            }
            self.value = None;
            result
        })
        .await;
        result.map(|_| ()).map_err(|error| {
            Error::Connection(format!("SFTP handle CLOSE unacknowledged: {error}"))
        })
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            // close_nowait is SDK-private. Public close queues CLOSE on its first
            // poll, before awaiting the ACK. Poll once and discard only this Drop
            // acknowledgement, matching the native File's best-effort cleanup.
            let mut close = std::pin::pin!(self.session.close(value));
            if let Poll::Ready(Err(_)) =
                close.as_mut().poll(&mut Context::from_waker(Waker::noop()))
            {
                // An unsent or rejected CLOSE leaves a resource unreclaimed.
                self.stop.cancel();
            }
        }
    }
}

type PendingRead =
    Pin<Box<dyn Future<Output = std::result::Result<Option<Vec<u8>>, SftpError>> + Send>>;

pub(super) struct ReadFile {
    handle: Handle,
    chunk: u32,
    offset: u64,
    pending: Option<PendingRead>,
    buffer: Vec<u8>,
    consumed: usize,
    eof: bool,
    failure: Option<SftpError>,
}
impl ReadFile {
    pub(super) async fn open(
        session: Arc<RawSftpSession>,
        limits: TransferLimits,
        path: &str,
        stop: CancellationToken,
    ) -> Result<Self> {
        if stop.is_cancelled() {
            return Err(Error::Connection("SFTP connection closed".into()));
        }
        let value = session
            .open(path, OpenFlags::READ, FileAttributes::empty())
            .await
            .map_err(|error| operation_error(&stop, error))?
            .handle;
        let mut handle = Handle::new(session, value, stop);
        let chunk = match limits.read_len(handle.value()) {
            Ok(chunk) => chunk,
            Err(error) => {
                if matches!(error, Error::Connection(_)) {
                    handle.stop.cancel();
                }
                return Err(match handle.close().await {
                    Ok(()) => error,
                    Err(close) => Error::join(error, [close]),
                });
            }
        };
        Ok(Self {
            handle,
            chunk,
            offset: 0,
            pending: None,
            buffer: Vec::new(),
            consumed: 0,
            eof: false,
            failure: None,
        })
    }
}
impl AsyncRead for ReadFile {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let file = self.get_mut();
        if out.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if let Some(error) = &file.failure {
            return Poll::Ready(Err(error.clone().into()));
        }
        if file.handle.stop.is_cancelled() {
            file.pending = None;
            let error = SftpError::UnexpectedBehavior("SFTP connection closed".into());
            file.failure = Some(error.clone());
            return Poll::Ready(Err(error.into()));
        }
        if file.consumed == file.buffer.len() {
            if file.eof {
                return Poll::Ready(Ok(()));
            }
            if file.pending.is_none() {
                let session = file.handle.session.clone();
                let handle = file.handle.value().to_owned();
                let offset = file.offset;
                // Do not prefetch: one request and at most one bounded buffer.
                let len = file
                    .chunk
                    .min(u32::try_from(out.remaining()).unwrap_or(u32::MAX));
                file.pending = Some(Box::pin(async move {
                    match session.read(handle, offset, len).await {
                        Ok(data) if data.data.is_empty() || data.data.len() > len as usize => {
                            Err(SftpError::UnexpectedBehavior(
                                "empty or oversized SFTP DATA response".into(),
                            ))
                        }
                        Ok(data) => Ok(Some(data.data)),
                        Err(SftpError::Status(status)) if status.status_code == StatusCode::Eof => {
                            Ok(None)
                        }
                        Err(error) => Err(error),
                    }
                }));
            }
            let result = ready!(file.pending.as_mut().unwrap().as_mut().poll(cx));
            file.pending = None;
            match result {
                Ok(Some(bytes)) => {
                    let Some(offset) = file.offset.checked_add(bytes.len() as u64) else {
                        let error =
                            SftpError::UnexpectedBehavior("SFTP read offset overflow".into());
                        file.handle.stop.cancel();
                        file.failure = Some(error.clone());
                        return Poll::Ready(Err(error.into()));
                    };
                    file.offset = offset;
                    file.buffer = bytes;
                    file.consumed = 0;
                }
                Ok(None) => {
                    file.eof = true;
                    return Poll::Ready(Ok(()));
                }
                Err(error) => {
                    if matches!(sftp_error(error.clone()), Error::Connection(_)) {
                        file.handle.stop.cancel();
                    }
                    file.failure = Some(error.clone());
                    return Poll::Ready(Err(error.into()));
                }
            }
        }
        let len = out.remaining().min(file.buffer.len() - file.consumed);
        out.put_slice(&file.buffer[file.consumed..file.consumed + len]);
        file.consumed += len;
        Poll::Ready(Ok(()))
    }
}
#[async_trait]
impl RemoteRead for ReadFile {
    async fn close(mut self: Box<Self>) -> Result<()> {
        self.pending = None;
        let closed = self.handle.close().await;
        // AsyncRead carries io::Error, but download completion must retain the
        // typed terminal cause even when the server still acknowledges CLOSE.
        match (self.failure.take(), closed) {
            (Some(error), Err(close)) => Err(Error::join(
                operation_error(&self.handle.stop, error),
                [close],
            )),
            (Some(error), Ok(())) => Err(operation_error(&self.handle.stop, error)),
            (None, result) => result,
        }
    }
}
