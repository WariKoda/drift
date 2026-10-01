//! Stored records remain raw. Defaults and server links are resolved into a
//! separate RuntimeConfig and are never serialized through the store API.
use crate::{
    error::{Error, Result},
    pathmap::{Mapping, validate},
};
use serde::{Deserialize, Serialize};
use std::{
    env,
    path::{Path, PathBuf},
};

#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Auth {
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key_file: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub passphrase: String,
}
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Host {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub server: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hostname: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub port: u16,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(skip_serializing_if = "is_default_auth")]
    pub auth: Auth,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub root_path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub protocol: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<Mapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive_interval: Option<i64>,
}
fn is_zero(port: &u16) -> bool {
    *port == 0
}
fn is_false(value: &bool) -> bool {
    !value
}
fn is_default_auth(auth: &Auth) -> bool {
    *auth == Auth::default()
}
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Defaults {
    #[serde(skip_serializing_if = "is_zero")]
    pub port: u16,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user: String,
}
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct TerminalPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse: Option<bool>,
    #[serde(skip_serializing_if = "is_false")]
    pub show_hidden: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub show_ignored: bool,
}
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct GlobalConfig {
    pub defaults: Defaults,
    pub ui: TerminalPreferences,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<Host>,
}
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ProjectConfig {
    pub defaults: Defaults,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<Host>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<Mapping>,
}
#[derive(Clone)]
pub struct RuntimeConfig {
    pub hosts: Vec<Host>,
    pub mappings: Vec<Mapping>,
    pub ui: TerminalPreferences,
}
impl Host {
    pub fn validate(&self, global: bool) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(Error::Invalid("host name is required".into()));
        }
        if self
            .keep_alive_interval
            .is_some_and(|v| !(0..=86400).contains(&v))
        {
            return Err(Error::Invalid(
                "keep_alive_interval must be between 0 and 86400 seconds".into(),
            ));
        }
        validate(&self.mappings)?;
        if !self.server.is_empty()
            && (global
                || !self.hostname.is_empty()
                || self.port != 0
                || !self.user.is_empty()
                || self.auth != Auth::default()
                || !self.protocol.is_empty()
                || self.keep_alive_interval.is_some())
        {
            return Err(Error::Invalid(format!(
                "host {:?}: a server link must contain only name, server, root_path and mappings",
                self.name
            )));
        }
        Ok(())
    }
    fn with_defaults(&self, defaults: &Defaults) -> Self {
        let mut host = self.clone();
        if host.port == 0 {
            host.port = if defaults.port != 0 {
                defaults.port
            } else if matches!(host.protocol.as_str(), "ftp" | "ftps") {
                21
            } else {
                22
            };
        }
        if host.user.is_empty() {
            host.user.clone_from(&defaults.user);
        }
        host
    }
    pub fn keep_alive_seconds(&self) -> u64 {
        self.keep_alive_interval.unwrap_or(60) as u64
    }
}
impl RuntimeConfig {
    pub fn resolve(global: &GlobalConfig, project: Option<&ProjectConfig>) -> Result<Self> {
        validate_hosts(&global.hosts, true)?;
        let servers: Vec<_> = global
            .hosts
            .iter()
            .map(|h| h.with_defaults(&global.defaults))
            .collect();
        let mut result = Self {
            hosts: servers.clone(),
            mappings: vec![],
            ui: global.ui.clone(),
        };
        if let Some(project) = project {
            validate(&project.mappings)?;
            validate_hosts(&project.hosts, false)?;
            result.hosts = project
                .hosts
                .iter()
                .map(|host| {
                    if host.server.is_empty() {
                        return Ok(host.with_defaults(&project.defaults));
                    }
                    let server =
                        servers
                            .iter()
                            .find(|s| s.name == host.server)
                            .ok_or_else(|| {
                                Error::Invalid(format!(
                                    "host {:?} links missing server {:?}",
                                    host.name, host.server
                                ))
                            })?;
                    let mut resolved = server.clone();
                    resolved.name.clone_from(&host.name);
                    resolved.server.clone_from(&host.server);
                    resolved.root_path.clone_from(&host.root_path);
                    resolved.mappings.clone_from(&host.mappings);
                    Ok(resolved)
                })
                .collect::<Result<_>>()?;
            result.mappings.clone_from(&project.mappings);
        }
        Ok(result)
    }
}
pub fn validate_hosts(hosts: &[Host], global: bool) -> Result<()> {
    let mut names = std::collections::HashSet::new();
    for host in hosts {
        host.validate(global)?;
        if !names.insert(&host.name) {
            return Err(Error::Invalid(format!("duplicate host {:?}", host.name)));
        }
    }
    Ok(())
}
pub fn config_dir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(dir).join("drift"));
    }
    env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .map(|home| PathBuf::from(home).join(".config/drift"))
        .ok_or_else(|| Error::Invalid("HOME or XDG_CONFIG_HOME must be set".into()))
}
pub fn project_store_path(dir: &Path, slug: &str) -> Result<PathBuf> {
    if slug.is_empty() || slug == "." || slug == ".." || slug.contains('/') || slug.contains('\0') {
        return Err(Error::Invalid(
            "project slug is not a usable file name".into(),
        ));
    }
    Ok(dir.join("projects").join(format!("{slug}.toml")))
}
