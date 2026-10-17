//! All local reads/mutations resolve through an opened capability directory.
//! Lexical checks alone never authorize I/O. Nonblocking opens and descriptor
//! metadata checks also refuse a FIFO swapped in after the path was inspected.
use crate::{
    error::{Error, Result},
    staging::is_staging_name,
};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions, OpenOptionsExt},
};
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub const PREVIEW_LIMIT: u64 = 1024 * 1024;
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
    pub symlink: bool,
    pub size: u64,
}
pub struct ProjectRoot {
    base: PathBuf,
    dir: Dir,
}
impl ProjectRoot {
    pub fn open(base: &Path) -> Result<Self> {
        let base = std::path::absolute(base)?;
        let dir = Dir::open_ambient_dir(&base, ambient_authority())?;
        Ok(Self { base, dir })
    }
    pub fn base(&self) -> &Path {
        &self.base
    }
    pub fn metadata(&self, path: &Path) -> Result<cap_std::fs::Metadata> {
        Ok(self.dir.metadata(self.relative(path)?)?)
    }
    fn relative<'a>(&self, path: &'a Path) -> Result<&'a Path> {
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.base)
                .map_err(|_| Error::Invalid("local path outside project".into()))?
        } else {
            path
        };
        if relative.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(Error::Invalid("local path outside project".into()));
        }
        Ok(if relative.as_os_str().is_empty() {
            Path::new(".")
        } else {
            relative
        })
    }
    pub fn entries(&self, path: &Path) -> Result<Vec<Entry>> {
        let relative = self.relative(path)?;
        let mut entries = Vec::new();
        for entry in self.dir.read_dir(relative)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_str().is_none() {
                return Err(Error::Invalid(
                    "cannot display a filename that is not UTF-8".into(),
                ));
            }
            let file_type = entry.file_type()?;
            if is_staging_name(&name.to_string_lossy())
                || (file_type.is_dir() && skip_directory(&name.to_string_lossy()))
                || matches!(name.to_str(), Some(".git" | ".svn" | ".hg"))
            {
                continue;
            }
            let path = relative
                .join(name)
                .components()
                .filter(|c| !matches!(c, Component::CurDir))
                .collect::<PathBuf>();
            let metadata = self.dir.symlink_metadata(&path)?;
            if !metadata.is_file() && !metadata.is_dir() && !metadata.file_type().is_symlink() {
                continue;
            }
            entries.push(Entry {
                path,
                directory: metadata.is_dir(),
                symlink: metadata.file_type().is_symlink(),
                size: metadata.len(),
            });
        }
        entries.sort_by(|a, b| b.directory.cmp(&a.directory).then(a.path.cmp(&b.path)));
        Ok(entries)
    }
    pub fn read_regular(&self, path: &Path) -> Result<cap_std::fs::File> {
        let relative = self.relative(path)?;
        let metadata = self.dir.metadata(relative)?;
        if !metadata.is_file() {
            return Err(Error::Invalid("local path is not a regular file".into()));
        }
        let file = self.dir.open_with(
            relative,
            OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK),
        )?;
        if !file.metadata()?.is_file() {
            return Err(Error::Invalid(
                "local path changed to a non-regular file".into(),
            ));
        }
        Ok(file)
    }
    pub fn preview(&self, path: &Path) -> Result<String> {
        let file = self.read_regular(path)?;
        if file.metadata()?.len() > PREVIEW_LIMIT {
            return Err(Error::Invalid("preview is limited to 1 MiB".into()));
        }
        let mut bytes = Vec::new();
        file.take(PREVIEW_LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > PREVIEW_LIMIT {
            return Err(Error::Invalid("preview is limited to 1 MiB".into()));
        }
        if bytes.contains(&0) {
            return Err(Error::Invalid("binary files cannot be previewed".into()));
        }
        String::from_utf8(bytes).map_err(|_| Error::Invalid("file is not UTF-8 text".into()))
    }
    pub fn remove(&self, path: &Path) -> Result<()> {
        Ok(self.dir.remove_file(self.relative(path)?)?)
    }
    /// The caller supplies completion (e.g. closing a remote stream and checking
    /// the FTP final reply). It must succeed before the destination is replaced.
    pub fn write_atomic<R: Read>(
        &self,
        path: &Path,
        mut source: R,
        complete: impl FnOnce(R) -> Result<()>,
    ) -> Result<()> {
        let prepared = (|| {
            let relative = self.relative(path)?;
            let parent = relative
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            self.dir.create_dir_all(parent)?;
            let parent = self.dir.open_dir(parent)?;
            let name = relative
                .file_name()
                .ok_or_else(|| Error::Invalid("target must name a file".into()))?
                .to_owned();
            let permissions = match parent.symlink_metadata(&name) {
                Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
                Ok(_) => return Err(Error::Invalid("local target is not a regular file".into())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            let stage_name = crate::staging::staging_name(&name.to_string_lossy())?;
            let stage =
                parent.open_with(&stage_name, OpenOptions::new().write(true).create_new(true))?;
            Ok::<_, Error>((parent, name, permissions, stage_name, stage))
        })();
        let (parent, name, permissions, stage_name, mut stage) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                return match complete(source) {
                    Ok(()) => Err(error),
                    Err(close) => Err(Error::join(error, [close])),
                };
            }
        };
        let result = (|| {
            let mut errors = Vec::new();
            if let Some(permissions) = permissions
                && let Err(error) = stage.set_permissions(permissions)
            {
                errors.push(Error::Io(error));
            }
            if errors.is_empty()
                && let Err(error) = std::io::copy(&mut source, &mut stage)
            {
                errors.push(Error::Io(error));
            }
            if let Err(error) = complete(source) {
                errors.push(error);
            }
            if let Err(error) = stage.flush() {
                errors.push(Error::Io(error));
            }
            if errors.is_empty()
                && let Err(error) = stage.sync_all()
            {
                errors.push(Error::Io(error));
            }
            if let Err(error) = nix::unistd::close(stage.into_std()) {
                errors.push(Error::Io(std::io::Error::from_raw_os_error(error as i32)));
            }
            if !errors.is_empty() {
                return Err(Error::join(errors.remove(0), errors));
            }
            parent.rename(&stage_name, &parent, &name)?;
            Ok(())
        })();
        if result.is_err()
            && let Err(cleanup) = parent.remove_file(&stage_name)
        {
            return Err(Error::join(
                result.unwrap_err(),
                [Error::Invalid(format!("remove staging file: {cleanup}"))],
            ));
        }
        result
    }
}
pub fn skip_directory(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".svn" | ".hg" | "node_modules" | ".idea" | ".vscode"
    )
}
