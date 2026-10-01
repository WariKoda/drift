use super::*;
use crate::staging::staging_name;
use russh_sftp::protocol::{FileAttributes, OpenFlags, Packet, StatusCode};
use tokio::io::AsyncWriteExt;

impl SftpClient {
    pub(super) async fn upload_staged(
        &self,
        path: &str,
        mut source: Box<dyn RemoteRead>,
    ) -> Result<()> {
        let mut stage_path = None;
        let written: Result<()> = async {
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
                match self.sftp.metadata(dir).await {
                    Ok(meta) if meta.file_type().is_dir() => continue,
                    Ok(_) => {
                        return Err(Error::Invalid(format!(
                            "remote parent {dir} is not a directory"
                        )));
                    }
                    Err(e) => match sftp_error(e) {
                        Error::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        e => return Err(e),
                    },
                }
                if let Err(error) = self.sftp.create_dir(dir).await {
                    // Another client may have created it after our stat.
                    if !self
                        .sftp
                        .metadata(dir)
                        .await
                        .is_ok_and(|m| m.file_type().is_dir())
                    {
                        return Err(sftp_error(error));
                    }
                }
            }
            let permissions = match self.sftp.symlink_metadata(path).await {
                Ok(meta) if meta.file_type().is_file() => meta.permissions.map(|p| p & 0o777),
                Ok(_) => return Err(Error::Invalid("remote target is not a regular file".into())),
                Err(e) => match sftp_error(e) {
                    Error::Io(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    e => return Err(e),
                },
            };
            let stage = parent
                .join(staging_name(base)?)
                .to_string_lossy()
                .into_owned();
            let mut file = self
                .sftp
                .open_with_flags(
                    &stage,
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                )
                .await
                .map_err(sftp_error)?;
            stage_path = Some(stage);
            let mut errors = Vec::new();
            if let Err(error) = tokio::io::copy(&mut source, &mut file).await {
                errors.push(transfer_io_error(error));
            }
            if errors.is_empty() {
                if let Some(permissions) = permissions {
                    let attributes = FileAttributes {
                        permissions: Some(permissions),
                        ..FileAttributes::empty()
                    };
                    if let Err(error) = file.set_metadata(attributes).await {
                        errors.push(sftp_error(error));
                    }
                }
                if let Err(error) = file.flush().await {
                    errors.push(transfer_io_error(error));
                }
            }
            if let Err(error) = file.close().await {
                errors.push(transfer_io_error(error));
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
        if result.is_err()
            && let Some(stage) = stage_path
        {
            match self.sftp.remove_file(&stage).await {
                Ok(()) => {}
                Err(e) => match sftp_error(e) {
                    Error::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    cleanup => {
                        return Err(Error::join(result.unwrap_err(), [cleanup]));
                    }
                },
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
                .rename
                .as_ref()
                .expect("advertised POSIX rename has a control channel")
                .extended("posix-rename@openssh.com", data)
                .await
                .map_err(sftp_error)?
            {
                Packet::Status(status) if status.status_code == StatusCode::Ok => return Ok(()),
                Packet::Status(status) if status.status_code == StatusCode::OpUnsupported => {}
                Packet::Status(status) => return Err(sftp_error(status.into())),
                _ => {
                    return Err(Error::Connection(
                        "unexpected POSIX rename response; outcome unknown".into(),
                    ));
                }
            }
        }
        // Never delete the old target to make a non-POSIX rename succeed.
        // A lost/timeout reply above is not a reason to send a second rename.
        match self.sftp.rename(stage, path).await.map_err(sftp_error) {
            Err(Error::Connection(error)) => Err(Error::Connection(error)),
            Err(error) if self.rename_unavailable.is_some() => Err(Error::Invalid(format!(
                "{error}; POSIX rename channel unavailable: {}",
                self.rename_unavailable.as_ref().unwrap()
            ))),
            result => result,
        }
    }
}
