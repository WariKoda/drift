//! Complete the protected data handshake on first I/O. SuppaFTP 12.1.0 connects
//! TLS before reading the preliminary FTP reply. A legitimate 550 has no data
//! handshake, so eager TLS deadlocks there. Deferring I/O lets it read that reply
//! first; every byte still passes the same pinned Rustls verifier before use.
use super::Lifetime;
use crate::remote::ConnectionState;
use async_trait::async_trait;
use std::sync::Arc;
use std::{
    fmt,
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
};
use suppaftp::{
    FtpError, FtpResult,
    tokio::{AsyncTlsConnector, TokioTlsStream},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tokio_rustls::{
    TlsConnector,
    client::{Connect, TlsStream},
};

pub(super) struct Connector(pub TlsConnector, pub Arc<Lifetime>);
impl fmt::Debug for Connector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FTPS Rustls connector")
    }
}
#[async_trait]
impl AsyncTlsConnector for Connector {
    type Stream = Stream;
    async fn connect(&self, domain: &str, stream: TcpStream) -> FtpResult<Stream> {
        let name = rustls::pki_types::ServerName::try_from(domain.to_owned())
            .map_err(|error| FtpError::SecureError(error.to_string()))?;
        Ok(Stream(
            State::Handshake(self.0.connect(name, stream)),
            self.1.clone(),
        ))
    }
}
enum State {
    Handshake(Connect<TcpStream>),
    Ready(TlsStream<TcpStream>),
    Failed(String),
}
pub(super) struct Stream(State, Arc<Lifetime>);
impl fmt::Debug for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FTPS Rustls stream")
    }
}
impl Stream {
    fn ready(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<&mut TlsStream<TcpStream>>> {
        if let State::Handshake(handshake) = &mut self.0 {
            self.0 = match Pin::new(handshake).poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Ok(stream)) => State::Ready(stream),
                Poll::Ready(Err(error)) => {
                    // The peer may leave the FTP control socket open without a
                    // completion reply. Close all I/O immediately so the typed
                    // challenge can reach the UI instead of waiting for it.
                    self.1
                        .terminate(ConnectionState::Failed(format!("FTPS handshake: {error}")));
                    self.0 = State::Failed(error.to_string());
                    return Poll::Ready(Err(error));
                }
            };
        }
        match &mut self.0 {
            State::Ready(stream) => Poll::Ready(Ok(stream)),
            State::Failed(error) => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                error.clone(),
            ))),
            State::Handshake(_) => unreachable!("pending handshake returned above"),
        }
    }
}
impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(std::task::ready!(self.ready(cx))?).poll_read(cx, buf)
    }
}
impl AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(std::task::ready!(self.ready(cx))?).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(std::task::ready!(self.ready(cx))?).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(std::task::ready!(self.ready(cx))?).poll_shutdown(cx)
    }
}
impl TokioTlsStream for Stream {
    type InnerStream = Self;
    fn tcp_stream(self) -> FtpResult<TcpStream> {
        match self.0 {
            State::Ready(stream) => Ok(stream.into_inner().0),
            _ => Err(FtpError::SecureError(
                "FTPS socket cannot be unwrapped before a completed handshake".into(),
            )),
        }
    }
    fn get_ref(&self) -> &TcpStream {
        match &self.0 {
            State::Ready(stream) => stream.get_ref().0,
            State::Handshake(handshake) => handshake
                .get_ref()
                .expect("pending handshake owns its socket"),
            State::Failed(_) => unreachable!("failed FTP streams are invalidated before reuse"),
        }
    }
    fn mut_ref(&mut self) -> &mut Self {
        self
    }
}
