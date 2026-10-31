//! Read-only active palette source. Call `load` on a background executor.
use drift_core::omarchy::{MAX_PALETTE_BYTES, Palette};
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct PaletteSource {
    theme: Option<PathBuf>,
}

impl PaletteSource {
    /// Resolve the state root, not the legacy config directory. No filesystem I/O.
    pub fn from_env() -> Self {
        Self::from_roots(
            std::env::var_os("XDG_STATE_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        )
    }

    /// Explicit roots keep callers/tests independent of process-global environment.
    pub fn from_roots(state: Option<&OsStr>, home: Option<&OsStr>) -> Self {
        let state = state
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                home.map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .map(|home| home.join(".local/state"))
            });
        Self {
            theme: state.map(|state| state.join("omarchy/current/theme")),
        }
    }

    /// Re-resolve each sample: both content edits and atomic theme/current symlink
    /// replacements must be noticed even when timestamps and file sizes match.
    pub fn load(&self) -> Result<Palette, String> {
        let failure =
            |reason: &str| format!("Omarchy theme unavailable: {reason}. Using System appearance.");
        let path = self
            .theme
            .as_ref()
            .ok_or_else(|| failure("no state directory"))?;
        let directory =
            fs::canonicalize(path).map_err(|_| failure("active theme directory cannot be read"))?;
        let file = File::options()
            .read(true)
            .custom_flags(nix::libc::O_NONBLOCK)
            .open(directory.join("colors.toml"))
            .map_err(|_| failure("colors.toml cannot be read"))?;
        let metadata = file
            .metadata()
            .map_err(|_| failure("palette metadata cannot be read"))?;
        if !metadata.is_file() || metadata.len() > MAX_PALETTE_BYTES as u64 {
            return Err(failure("palette must be a regular file of at most 64 KiB"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_PALETTE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| failure("palette cannot be read"))?;
        if bytes.len() > MAX_PALETTE_BYTES {
            return Err(failure("palette exceeds 64 KiB"));
        }
        let light = match fs::metadata(directory.join("light.mode")) {
            Ok(metadata) => metadata.is_file(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(failure("light.mode cannot be read")),
        };
        let text = std::str::from_utf8(&bytes).map_err(|_| failure("palette is not UTF-8"))?;
        Palette::parse(text, light).map_err(|error| failure(&error.to_string()))
    }

    pub fn theme_directory(&self) -> Option<&Path> {
        self.theme.as_deref()
    }
}

#[cfg(test)]
mod tests;
