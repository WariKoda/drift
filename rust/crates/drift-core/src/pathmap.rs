//! Lexical mapping policy, matching internal/pathmap. Filesystem confinement is
//! a separate responsibility: these translations must never authorize an open.
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Mapping {
    pub local: String,
    pub remote: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingError(pub String);
impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for MappingError {}

// POSIX paths are shared by the supported Linux/macOS platforms and remotes.
fn clean(value: &str) -> String {
    let absolute = value.starts_with('/');
    let mut parts = Vec::new();
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            ".." if !absolute => parts.push(part),
            ".." => {}
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".into()
    } else {
        joined
    }
}
fn join(base: &str, suffix: &str) -> String {
    clean(&format!("{base}/{suffix}"))
}
fn suffix<'a>(value: &'a str, base: &str) -> Option<&'a str> {
    if value == base {
        return Some("");
    }
    if base == "/" {
        return value.strip_prefix('/');
    }
    if base == "." {
        return (!value.starts_with('/') && value != ".." && !value.starts_with("../"))
            .then_some(value);
    }
    value.strip_prefix(base)?.strip_prefix('/')
}
fn relative(value: &str) -> Result<String, MappingError> {
    if value.trim().is_empty() || value.starts_with('/') || value.split('/').any(|s| s == "..") {
        return Err(MappingError(format!("invalid relative mapping: {value:?}")));
    }
    Ok(clean(value))
}

pub fn validate(mappings: &[Mapping]) -> Result<(), MappingError> {
    let normalized: Vec<_> = mappings
        .iter()
        .map(|m| Ok((relative(&m.local)?, relative(&m.remote)?)))
        .collect::<Result<_, MappingError>>()?;
    for (i, (local, remote)) in normalized.iter().enumerate() {
        for (other_local, other_remote) in &normalized[i + 1..] {
            if local == other_local || remote == other_remote {
                return Err(MappingError("duplicate mapping base".into()));
            }
            match (
                suffix(other_local, local),
                suffix(other_remote, remote),
                suffix(local, other_local),
                suffix(remote, other_remote),
            ) {
                (Some(l), Some(r), _, _) if l == r => {}
                (_, _, Some(l), Some(r)) if l == r => {}
                (None, None, None, None) => {}
                _ => return Err(MappingError("ambiguous mapping overlap".into())),
            }
        }
    }
    Ok(())
}

pub struct Mapper {
    project_root: String,
    remote_root: String,
    mappings: Vec<Mapping>,
}
impl Mapper {
    pub fn new(
        project_root: &str,
        remote_root: &str,
        project_mappings: &[Mapping],
        host_mappings: &[Mapping],
    ) -> Result<Self, MappingError> {
        let mappings = if host_mappings.is_empty() {
            project_mappings
        } else {
            host_mappings
        };
        validate(mappings)?;
        Ok(Self {
            project_root: clean(project_root),
            remote_root: clean(if remote_root.is_empty() {
                "/"
            } else {
                remote_root
            }),
            mappings: mappings.to_vec(),
        })
    }
    pub fn local_to_remote(&self, local: &str) -> Result<String, MappingError> {
        let local = clean(local);
        let best = self
            .mappings
            .iter()
            .filter_map(|m| {
                let base = join(&self.project_root, &m.local);
                suffix(&local, &base).map(|s| (base.len(), m, s))
            })
            .max_by_key(|(len, _, _)| *len);
        if let Some((_, m, s)) = best {
            return Ok(join(&join(&self.remote_root, &m.remote), s));
        }
        if !self.mappings.is_empty() {
            return Err(MappingError("local path not covered by mappings".into()));
        }
        suffix(&local, &self.project_root)
            .map(|s| join(&self.remote_root, s))
            .ok_or_else(|| MappingError("local path outside project".into()))
    }
    pub fn remote_to_local(&self, remote: &str) -> Result<String, MappingError> {
        let remote = clean(remote);
        let best = self
            .mappings
            .iter()
            .filter_map(|m| {
                let base = join(&self.remote_root, &m.remote);
                suffix(&remote, &base).map(|s| (base.len(), m, s))
            })
            .max_by_key(|(len, _, _)| *len);
        if let Some((_, m, s)) = best {
            return Ok(join(&join(&self.project_root, &m.local), s));
        }
        if !self.mappings.is_empty() {
            return Err(MappingError("remote path not covered by mappings".into()));
        }
        suffix(&remote, &self.remote_root)
            .map(|s| join(&self.project_root, s))
            .ok_or_else(|| MappingError("remote path outside host root".into()))
    }
}
