use super::*;
use crate::staging::staging_name;
use russh_sftp::protocol::{FileAttributes, OpenFlags, Packet, StatusCode};

impl SftpClient {
    pub(super) async fn upload_staged(
        &self,
        path: &str,
        mut source: Box<dyn RemoteRead>,
    ) -> Result<()> {
        let mut stage_path = None;
        let written: Result<()> = async {
            self.check_connected()?;
            let target = std::path::Path::new(path);
            let parent = target.parent().unwrap_or(std::path::Path::new("."));
            let base = target
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| Error::Invalid("upload target must name a UTF-8 file".into()))?;
            let mut current = std::path::PathBuf::new();
            for part in parent.components() {
                current.push(part);
                let dir = current
                    .to_str()
                    .ok_or_else(|| Error::Invalid("remote path is not UTF-8".into()))?;
                match self.request(self.sftp.stat(dir)).await {
                    Ok(meta) if meta.attrs.file_type().is_dir() => continue,
                    Ok(_) => {
                        return Err(Error::Invalid(format!(
                            "remote parent {dir} is not a directory"
                        )));
                    }
                    Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
                if let Err(error) = self
                    .request(self.sftp.mkdir(dir, FileAttributes::empty()))
                    .await
                {
                    if matches!(error, Error::Connection(_)) {
                        return Err(error);
                    }
                    // Another client may have created it after our stat.
                    match self.request(self.sftp.stat(dir)).await {
                        Ok(meta) if meta.attrs.file_type().is_dir() => {}
                        Ok(_) => return Err(error),
                        Err(stat) => return Err(Error::join(error, [stat])),
                    }
                }
            }
            let permissions = match self.request(self.sftp.lstat(path)).await {
                Ok(meta) if meta.attrs.file_type().is_file() => {
                    meta.attrs.permissions.map(|p| p & 0o777)
                }
                Ok(_) => return Err(Error::Invalid("remote target is not a regular file".into())),
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e),
            };
            let stage = parent
                .join(staging_name(base)?)
                .to_string_lossy()
                .into_owned();
            let value = self
                .request(self.sftp.open(
                    stage.as_str(),
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                    FileAttributes::empty(),
                ))
                .await?
                .handle;
            let mut file = Handle::new(self.sftp.clone(), value, self.stop.clone());
            stage_path = Some(stage);
            let mut errors = Vec::new();
            let copied: Result<()> = async {
                let chunk = self.limits.write_len(file.value())?;
                let mut buffer = vec![0; chunk];
                let mut offset = 0u64;
                loop {
                    let len = source.read(&mut buffer).await.map_err(transfer_io_error)?;
                    if len == 0 {
                        break;
                    }
                    let next = offset
                        .checked_add(len as u64)
                        .ok_or_else(|| Error::Invalid("SFTP upload offset overflow".into()))?;
                    // Every write is acknowledged before reusing this bounded buffer.
                    self.request(
                        self.sftp
                            .write(file.value(), offset, buffer[..len].to_vec()),
                    )
                    .await?;
                    offset = next;
                }
                Ok(())
            }
            .await;
            if let Err(error) = copied {
                errors.push(error);
            }
            if errors.is_empty() {
                if let Some(permissions) = permissions {
                    let attributes = FileAttributes {
                        permissions: Some(permissions),
                        ..FileAttributes::empty()
                    };
                    if let Err(error) = self
                        .request(self.sftp.fsetstat(file.value(), attributes))
                        .await
                    {
                        errors.push(error);
                    }
                }
                // Writes are already drained. Match native flush semantics: fsync
                // only when advertised, then always await the CLOSE status.
                if self.fsync
                    && let Err(error) = self.request(self.sftp.fsync(file.value())).await
                {
                    errors.push(error);
                }
            }
            if let Err(error) = file.close().await {
                errors.push(error);
            }
            if !errors.is_empty() {
                return Err(Error::join(errors.remove(0), errors));
            }
            Ok(())
        }
        .await;
        // Always finish the source, even if preparing the destination failed.
        let closed = source.close().await;
        let result = match (written, closed) {
            (Ok(()), Ok(())) => self.commit_upload(stage_path.as_ref().unwrap(), path).await,
            (Err(a), Err(b)) => Err(Error::join(a, [b])),
            (Err(e), _) | (_, Err(e)) => Err(e),
        };
        // A source failure may belong to another connection. Reclaim our stage
        // before upload() detaches a still-healthy destination.
        if result.is_err()
            && let Some(stage) = stage_path
        {
            match self.request(self.sftp.remove(stage.as_str())).await {
                Ok(_) => {}
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(cleanup) => {
                    return Err(Error::join(result.unwrap_err(), [cleanup]));
                }
            }
        }
        result
    }
    async fn commit_upload(&self, stage: &str, path: &str) -> Result<()> {
        if self.posix_rename {
            // OpenSSH PROTOCOL §4.3: two SSH strings (u32 length + bytes).
            let mut data = Vec::new();
            for value in [stage, path] {
                let len = u32::try_from(value.len())
                    .map_err(|_| Error::Invalid("remote path is too long".into()))?;
                data.extend_from_slice(&len.to_be_bytes());
                data.extend_from_slice(value.as_bytes());
            }
            match self
                .request(self.sftp.extended("posix-rename@openssh.com", data))
                .await?
            {
                Packet::Status(status) if status.status_code == StatusCode::Ok => return Ok(()),
                Packet::Status(status) if status.status_code == StatusCode::OpUnsupported => {}
                Packet::Status(status) => {
                    return Err(operation_error(&self.stop, status.into()));
                }
                _ => {
                    self.stop.cancel();
                    return Err(Error::Connection(
                        "unexpected POSIX rename response; outcome unknown".into(),
                    ));
                }
            }
        }
        // Never delete the old target to make a non-POSIX rename succeed.
        // A lost/timeout reply above is not a reason to send a second rename.
        self.request(self.sftp.rename(stage, path))
            .await
            .map(|_| ())
    }
}
