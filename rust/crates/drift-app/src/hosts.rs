//! Host management uses raw records and expected versions, independently of GPUI.
use drift_core::{
    config::{Auth, Host},
    error::{Error, Result},
    pathmap::Mapping,
    store::{HostCatalog, LinkCatalog, LinkTarget, LinkedHostSave, Promotion},
};

pub enum HostCommand {
    Load,
    LinkTargets,
    OfferLink {
        desired: Box<Host>,
    },
    SaveLink {
        expected: Option<Box<Host>>,
        desired: Box<Host>,
        target: Box<LinkTarget>,
    },
    SelectLink {
        expected: Box<LinkTarget>,
    },
    Save {
        expected: Option<Box<Host>>,
        desired: Box<Host>,
    },
    Delete {
        expected: Box<Host>,
    },
}
pub enum HostResponse {
    Loaded(Box<HostCatalog>),
    LinkTargets(Box<LinkCatalog>),
    LinkOffer(Box<LinkCatalog>),
    LinkSaved(Box<LinkedHostSave>),
    LinkSelected(Box<Promotion>),
    Saved,
    Deleted,
}

#[derive(Default)]
pub struct HostDraft {
    pub name: String,
    pub server: String,
    pub hostname: String,
    pub port: String,
    pub user: String,
    pub protocol: String,
    pub auth_kind: String,
    pub password: String,
    pub key_file: String,
    pub passphrase: String,
    pub root_path: String,
    pub keep_alive: String,
    pub mappings: Vec<Mapping>,
}
impl From<&Host> for HostDraft {
    fn from(host: &Host) -> Self {
        Self {
            name: host.name.clone(),
            server: host.server.clone(),
            hostname: host.hostname.clone(),
            port: if host.port == 0 {
                String::new()
            } else {
                host.port.to_string()
            },
            user: host.user.clone(),
            protocol: host.protocol.clone(),
            auth_kind: host.auth.kind.clone(),
            password: host.auth.password.clone(),
            key_file: host.auth.key_file.clone(),
            passphrase: host.auth.passphrase.clone(),
            root_path: host.root_path.clone(),
            keep_alive: host
                .keep_alive_interval
                .map_or_else(String::new, |v| v.to_string()),
            mappings: host.mappings.clone(),
        }
    }
}
impl HostDraft {
    pub fn build(&self, global: bool) -> Result<Host> {
        if self.root_path.trim().is_empty() {
            return Err(Error::Invalid("Root path is required".into()));
        }
        let mut host = Host {
            name: self.name.trim().into(),
            server: self.server.clone(),
            root_path: self.root_path.clone(),
            mappings: self.mappings.clone(),
            ..Host::default()
        };
        if self.server.is_empty() {
            if self.hostname.trim().is_empty() {
                return Err(Error::Invalid("Hostname is required".into()));
            }
            if !matches!(self.protocol.as_str(), "" | "sftp" | "ftp" | "ftps") {
                return Err(Error::Invalid("Unsupported protocol".into()));
            }
            if !matches!(
                self.auth_kind.as_str(),
                "" | "keyfile" | "password" | "agent"
            ) {
                return Err(Error::Invalid("Unsupported authentication method".into()));
            }
            host.hostname = self.hostname.trim().into();
            host.port = if self.port.trim().is_empty() {
                0
            } else {
                self.port
                    .trim()
                    .parse::<u16>()
                    .ok()
                    .filter(|p| *p != 0)
                    .ok_or_else(|| {
                        Error::Invalid("Port must be 1–65535 or empty for the default".into())
                    })?
            };
            host.user = self.user.clone();
            host.protocol = self.protocol.clone();
            host.auth = match (self.protocol.as_str(), self.auth_kind.as_str()) {
                ("ftp" | "ftps", _) | (_, "password") => Auth {
                    kind: "password".into(),
                    password: self.password.clone(),
                    ..Auth::default()
                },
                (_, "agent") => Auth {
                    kind: "agent".into(),
                    ..Auth::default()
                },
                (_, kind) => Auth {
                    kind: kind.into(),
                    key_file: self.key_file.clone(),
                    passphrase: self.passphrase.clone(),
                    ..Auth::default()
                },
            };
            host.keep_alive_interval = if self.keep_alive.trim().is_empty() {
                None
            } else {
                Some(self.keep_alive.trim().parse::<i64>().map_err(|_| {
                    Error::Invalid("Keep-alive must be 0–86400 seconds or empty for 60".into())
                })?)
            };
        }
        host.validate(global)?;
        Ok(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_values_remain_absent_and_explicit_zero_remains_disabled() {
        let host = Host {
            name: "prod".into(),
            hostname: "example".into(),
            root_path: "/srv".into(),
            ..Host::default()
        };
        let mut draft = HostDraft::from(&host);
        let built = draft.build(false).unwrap();
        assert!(built == host);
        draft.keep_alive = "0".into();
        assert_eq!(draft.build(false).unwrap().keep_alive_interval, Some(0));
        for value in ["-1", "86401", "seconds"] {
            draft.keep_alive = value.into();
            assert!(draft.build(false).is_err());
        }
        draft.keep_alive.clear();
        for value in ["0", "65536", "invalid"] {
            draft.port = value.into();
            assert!(draft.build(false).is_err());
        }
    }
    #[test]
    fn links_strip_connection_fields_and_authentication_uses_protocol_specific_fields() {
        let mut draft = HostDraft {
            name: "prod".into(),
            server: "shared".into(),
            root_path: "/srv".into(),
            hostname: "hidden.example".into(),
            password: "hidden secret".into(),
            port: "invalid hidden port".into(),
            ..HostDraft::default()
        };
        let link = draft.build(false).unwrap();
        assert!(link.hostname.is_empty());
        assert_eq!(link.port, 0);
        assert!(link.auth == Auth::default());
        assert!(draft.build(true).is_err());
        draft.server.clear();
        draft.port.clear();
        draft.auth_kind = "agent".into();
        assert_eq!(draft.build(false).unwrap().auth.kind, "agent");
        for protocol in ["ftp", "ftps"] {
            draft.protocol = protocol.into();
            let host = draft.build(false).unwrap();
            assert_eq!(host.auth.kind, "password");
            assert_eq!(host.auth.password, "hidden secret");
            assert!(host.auth.key_file.is_empty());
        }
        draft.mappings = vec![Mapping {
            local: "../outside".into(),
            remote: "deploy".into(),
        }];
        assert!(draft.build(false).is_err());
    }
}
