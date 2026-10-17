use super::tls::Stream;
use super::{FtpClient, Lease, command_value, ftp_error, metadata};
use crate::{
    error::{Error, Result},
    remote::{ConnectionState, RemoteRead},
    staging::staging_name,
};
use async_trait::async_trait;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use suppaftp::tokio::TransferStream;
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};

struct FtpRead {
    transfer: Option<TransferStream<Stream>>,
    lease: Lease,
}
impl AsyncRead for FtpRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let result = Pin::new(self.transfer.as_mut().unwrap()).poll_read(cx, buffer);
        if let Poll::Ready(Err(error)) = &result {
            self.lease.life.terminate(ConnectionState::Failed(format!(
                "FTP data connection: {error}"
            )));
        }
        result
    }
}
#[async_trait]
impl RemoteRead for FtpRead {
    async fn close(mut self: Box<Self>) -> Result<()> {
        let result = self
            .transfer
            .take()
            .unwrap()
            .finish()
            .await
            .map_err(ftp_error);
        self.lease.complete(result)
    }
}
impl FtpClient {
    pub(super) async fn open_stream(&self, path: &str) -> Result<Box<dyn RemoteRead>> {
        command_value(path)?;
        let mut lease = self.lease().await?;
        match lease.connection.ftp.retr_as_stream(path).await {
            Ok(transfer) => Ok(Box::new(FtpRead {
                transfer: Some(transfer),
                lease,
            })),
            Err(error) => lease.complete(Err(ftp_error(error))),
        }
    }
    pub(super) async fn upload_staged(
        &self,
        path: &str,
        mut source: Box<dyn RemoteRead>,
    ) -> Result<()> {
        // Even invalid paths and failed admission consume and close the source.
        let admission = async {
            command_value(path)?;
            self.lease().await
        }
        .await;
        let mut lease = match admission {
            Ok(lease) => lease,
            Err(error) => return Err(Error::join(error, source.close().await.err())),
        };
        let mut stage = None;
        let prepare = async {
            let target = std::path::Path::new(path);
            let parent = target.parent().and_then(|p| p.to_str()).filter(|p| !p.is_empty()).unwrap_or(".");
            // Never replace directories or links with a staged regular file.
            match metadata::stat(&mut lease.connection.ftp, path).await {
                Ok(metadata) if !metadata.regular => return Err(Error::Invalid("FTP upload target is not a regular file".into())),
                Ok(_) => {},
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return Err(error),
            }
            let mut current = if parent.starts_with('/') { "/".to_owned() } else { String::new() };
            for part in parent.split('/').filter(|p| !p.is_empty() && *p != ".") {
                if !current.ends_with('/') && !current.is_empty() { current.push('/'); }
                current.push_str(part);
                if let Err(error) = lease.connection.ftp.mkdir(&current).await {
                    // A refusal is ignorable only if this exact directory exists.
                    if !matches!(&error, suppaftp::FtpError::UnexpectedResponse(response) if matches!(response.status.code(), 550 | 521)) { return Err(ftp_error(error)); }
                    let metadata = metadata::stat(&mut lease.connection.ftp, &current).await?;
                    if !metadata.directory { return Err(Error::Invalid("FTP upload parent is not a directory".into())); }
                }
            }
            let name = target.file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::Invalid("FTP upload requires a file name".into()))?;
            let path = format!("{}/{}", parent.trim_end_matches('/'), staging_name(name)?);
            stage = Some(path.clone());
            let mut transfer = lease.connection.ftp.put_with_stream(&path).await.map_err(ftp_error)?;
            let copy = tokio::io::copy(&mut source, &mut transfer).await.map_err(Error::Io);
            let flush = transfer.flush().await.map_err(|e| Error::Connection(format!("FTP upload flush: {e}")));
            let finish = transfer.finish().await.map_err(ftp_error);
            let mut failures = copy.err().into_iter().chain(flush.err()).chain(finish.err());
            if let Some(first) = failures.next() { return Err(Error::join(first, failures)); }
            Ok(())
        }.await;
        let close = source.close().await;
        let mut failures = prepare.err().into_iter().chain(close.err());
        let mut result = if let Some(first) = failures.next() {
            Err(Error::join(first, failures))
        } else {
            Ok(())
        };
        if result.is_ok() {
            result = lease
                .connection
                .ftp
                .rename(stage.as_deref().unwrap(), path)
                .await
                .map_err(ftp_error);
        }
        if let Err(error) = result {
            // Lost acknowledgements are never followed by a repeated mutation.
            let cleanup =
                if !matches!(error, Error::Connection(_)) && !self.life.stop.is_cancelled() {
                    if let Some(stage) = stage {
                        lease.connection.ftp.rm(stage).await.err().map(ftp_error)
                    } else {
                        None
                    }
                } else {
                    None
                };
            return lease.complete(Err(Error::join(error, cleanup)));
        }
        lease.complete(Ok(()))
    }
}
