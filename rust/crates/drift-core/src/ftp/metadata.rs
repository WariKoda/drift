use super::{command_value, ftp_error, unsupported};
use crate::{
    error::{Error, Result},
    remote::{RemoteEntry, RemoteMetadata},
};
use std::{path::Path, time::SystemTime};
use suppaftp::{
    FtpError,
    list::{File, ListParser},
    tokio::AsyncFtpStream,
};

fn parse_listing(lines: Vec<String>) -> Result<Vec<File>> {
    lines
        .into_iter()
        .filter(|line| !line.trim().is_empty() && !line.starts_with("total "))
        .map(|line| {
            let file = ListParser::parse_posix(&line)
                .or_else(|_| ListParser::parse_dos(&line))
                .map_err(|_| Error::Invalid("cannot parse FTP directory entry".into()))?;
            command_value(file.name())?;
            if file.name().is_empty() || file.name().contains('/') {
                return Err(Error::Invalid("invalid FTP directory entry name".into()));
            }
            Ok(file)
        })
        .collect()
}
/// 550 proves nothing until an accessible parent listing excludes the name.
async fn missing(ftp: &mut AsyncFtpStream, path: &str, cause: FtpError) -> Error {
    let probe =
        matches!(&cause, FtpError::UnexpectedResponse(response) if response.status.code() == 550);
    let mut cause = ftp_error(cause);
    if !probe {
        return cause;
    }
    let mut current = Path::new(path);
    while let (Some(parent), Some(name)) = (current.parent(), current.file_name()) {
        let parent_text = parent
            .to_str()
            .filter(|text| !text.is_empty())
            .unwrap_or(".");
        match ftp.list(Some(parent_text)).await {
            Ok(lines) => {
                let entries = match parse_listing(lines) {
                    Ok(entries) => entries,
                    Err(error) => return Error::join(cause, [error]),
                };
                if entries
                    .iter()
                    .any(|entry| entry.name() == name.to_string_lossy())
                {
                    return cause;
                }
                return Error::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("remote path {path} is absent from its parent listing"),
                ));
            }
            Err(error) => {
                // Only an actual 550 permits checking the next ancestor. A
                // malformed listing or terminal socket failure cannot prove absence.
                let ancestor = matches!(&error, FtpError::UnexpectedResponse(response) if response.status.code() == 550);
                cause = Error::join(cause, [ftp_error(error)]);
                if !ancestor {
                    return cause;
                }
            }
        }
        current = parent;
    }
    cause
}
pub(super) async fn stat(ftp: &mut AsyncFtpStream, path: &str) -> Result<RemoteMetadata> {
    match ftp.mlst(Some(path)).await {
        Ok(line) => {
            let file = ListParser::parse_mlst(&line)
                .map_err(|_| Error::Invalid("cannot parse FTP MLST metadata".into()))?;
            return Ok(RemoteMetadata {
                size: file.size() as u64,
                modified: line
                    .split(';')
                    .any(|fact| {
                        fact.split_once('=')
                            .is_some_and(|(name, _)| name.eq_ignore_ascii_case("modify"))
                    })
                    .then(|| file.modified()),
                directory: file.is_directory(),
                regular: file.is_file(),
            });
        }
        Err(error)
            if unsupported(&error)
                || matches!(&error, FtpError::UnexpectedResponse(response) if response.status.code() == 550) =>
            {}
        Err(error) => return Err(ftp_error(error)),
    }
    match ftp.size(path).await {
        Ok(size) => {
            let modified = match ftp.mdtm(path).await {
                Ok(time) => Some(SystemTime::from(time.and_utc())),
                Err(error)
                    if unsupported(&error)
                        || matches!(&error, FtpError::UnexpectedResponse(response) if response.status.code() == 550) =>
                {
                    None
                }
                Err(error) => return Err(ftp_error(error)),
            };
            Ok(RemoteMetadata {
                size: size as u64,
                modified,
                directory: false,
                regular: true,
            })
        }
        Err(error) if matches!(&error, FtpError::UnexpectedResponse(response) if response.status.code() == 550) => {
            match ftp.list(Some(path)).await {
                Ok(lines) => {
                    parse_listing(lines)?;
                    Ok(RemoteMetadata {
                        size: 0,
                        modified: None,
                        directory: true,
                        regular: false,
                    })
                }
                Err(list_error) => {
                    let classified = missing(ftp, path, list_error).await;
                    if matches!(&classified, Error::Io(e) if e.kind() == std::io::ErrorKind::NotFound)
                    {
                        Err(classified)
                    } else {
                        Err(Error::join(ftp_error(error), [classified]))
                    }
                }
            }
        }
        Err(error) => Err(ftp_error(error)),
    }
}
pub(super) async fn directory(ftp: &mut AsyncFtpStream, path: &str) -> Result<Vec<RemoteEntry>> {
    // Keep the typed status until missing classification is complete.
    let lines = match ftp.list(Some(path)).await {
        Ok(lines) => lines,
        Err(error) => return Err(missing(ftp, path, error).await),
    };
    let mut entries = vec![];
    for file in parse_listing(lines)? {
        let name = file.name();
        if name == "." || name == ".." {
            continue;
        }
        entries.push(RemoteEntry {
            name: name.into(),
            path: format!("{}/{}", path.trim_end_matches('/'), name),
            directory: file.is_directory(),
            regular: file.is_file(),
            symlink: file.is_symlink(),
            size: file.size() as u64,
        });
    }
    entries.sort_by(|a, b| b.directory.cmp(&a.directory).then(a.name.cmp(&b.name)));
    Ok(entries)
}
