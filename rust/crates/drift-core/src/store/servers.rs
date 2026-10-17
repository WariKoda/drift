//! Cross-project server selection and promotion under the shared write lock.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq)]
pub struct LinkTarget {
    /// None identifies a global server; otherwise this is the owning slug.
    pub project: Option<String>,
    pub host: Host,
    pub used_by: Vec<String>,
    stored: Host,
    defaults: Defaults,
}
pub struct LinkCatalog {
    pub targets: Vec<LinkTarget>,
    pub project_names: BTreeMap<String, String>,
}
pub struct Promotion {
    pub server: Host,
    /// The global copy is committed even if updating the source store fails.
    /// Callers must show this warning and must not automatically repeat promotion.
    pub warning: Option<String>,
}
/// A successful selection may have committed a server even when a later write
/// failed. Callers must retain that server and must not repeat promotion.
pub struct LinkedHostSave {
    pub server: Host,
    pub warning: Option<String>,
    pub destination_error: Option<Error>,
}

fn same_endpoint(a: &Host, b: &Host) -> bool {
    let a_protocol = if a.protocol.is_empty() {
        "sftp"
    } else {
        &a.protocol
    };
    let b_protocol = if b.protocol.is_empty() {
        "sftp"
    } else {
        &b.protocol
    };
    let port = |h: &Host| {
        if h.port != 0 {
            h.port
        } else if matches!(h.protocol.as_str(), "ftp" | "ftps") {
            21
        } else {
            22
        }
    };
    !a.hostname.is_empty()
        && a.hostname.eq_ignore_ascii_case(&b.hostname)
        && port(a) == port(b)
        && a.user == b.user
        && a_protocol == b_protocol
}

impl Store {
    /// Offer discovery never saves the draft or promotes a source host.
    pub fn matching_link_targets(&self, current: &str, desired: &Host) -> Result<LinkCatalog> {
        if !desired.server.is_empty() {
            return Err(Error::Invalid(
                "link offers require a direct project host".into(),
            ));
        }
        let effective = self.preview_host(Some(current), desired.clone())?;
        let mut catalog = self.link_targets(current)?;
        catalog
            .targets
            .retain(|target| same_endpoint(&effective, &target.host));
        Ok(catalog)
    }

    /// Validate the entire transaction before committing global → source →
    /// destination. After promotion commits, later failures are result fields,
    /// not an ordinary error that could prompt another promotion.
    pub fn save_linked_host(
        &self,
        current: &str,
        expected: Option<&Host>,
        desired: Host,
        target: &LinkTarget,
    ) -> Result<LinkedHostSave> {
        let lock = self.lock()?;
        let result = (|| {
            desired.validate(false)?;
            if !desired.server.is_empty() {
                return Err(Error::Invalid(
                    "link acceptance requires a direct project host".into(),
                ));
            }
            let destination_path = project_store_path(&self.dir, current)?;
            let mut global = self.global()?;
            let mut destination = self.project(current)?;
            replace_host(&mut destination.hosts, expected, Some(desired.clone()))?;
            RuntimeConfig::resolve(&global, Some(&destination))?;
            let effective = desired.with_defaults(&destination.defaults);

            let Some(slug) = &target.project else {
                let stored = global.hosts.iter().find(|h| h.name == target.stored.name);
                if stored != Some(&target.stored) || global.defaults != target.defaults {
                    return Err(Error::Conflict("selected server".into()));
                }
                let server = target.stored.with_defaults(&global.defaults);
                if !same_endpoint(&effective, &server) {
                    return Err(Error::Conflict("destination endpoint".into()));
                }
                let link = Host {
                    name: desired.name.clone(),
                    server: server.name.clone(),
                    root_path: desired.root_path.clone(),
                    mappings: desired.mappings.clone(),
                    ..Host::default()
                };
                replace_host(&mut destination.hosts, Some(&desired), Some(link))?;
                RuntimeConfig::resolve(&global, Some(&destination))?;
                self.write(&destination_path, &destination)?;
                return Ok(LinkedHostSave {
                    server,
                    warning: None,
                    destination_error: None,
                });
            };
            if slug == current {
                return Err(Error::Invalid(
                    "the host belongs to the open project".into(),
                ));
            }
            let source_path = project_store_path(&self.dir, slug)?;
            let mut source = self.project(slug)?;
            let index = source
                .hosts
                .iter()
                .position(|h| h.name == target.stored.name)
                .ok_or_else(|| Error::Conflict("selected source host".into()))?;
            if source.hosts[index] != target.stored || source.defaults != target.defaults {
                return Err(Error::Conflict("selected source host".into()));
            }
            if !source.hosts[index].server.is_empty() {
                return Err(Error::Invalid("source host already links a server".into()));
            }
            let mut server = source.hosts[index].with_defaults(&source.defaults);
            if !same_endpoint(&effective, &server) {
                return Err(Error::Conflict("destination endpoint".into()));
            }
            let name = server.name.clone();
            if global.hosts.iter().any(|h| h.name == server.name) {
                server.name = format!("{slug}-{name}");
                let mut suffix = 2;
                while global.hosts.iter().any(|h| h.name == server.name) {
                    server.name = format!("{slug}-{name}-{suffix}");
                    suffix += 1;
                }
            }
            server.mappings.clear();
            source.hosts[index] = Host {
                name: target.stored.name.clone(),
                server: server.name.clone(),
                root_path: target.stored.root_path.clone(),
                mappings: target.stored.mappings.clone(),
                ..Host::default()
            };
            let link = Host {
                name: desired.name.clone(),
                server: server.name.clone(),
                root_path: desired.root_path.clone(),
                mappings: desired.mappings.clone(),
                ..Host::default()
            };
            replace_host(&mut destination.hosts, Some(&desired), Some(link))?;
            global.hosts.push(server.clone());
            RuntimeConfig::resolve(&global, Some(&source))?;
            RuntimeConfig::resolve(&global, Some(&destination))?;
            let server = server.with_defaults(&global.defaults);
            // An empty source user cannot be materialized against a nonempty
            // global default. Do not accept a promotion that changes accounts.
            if !same_endpoint(&effective, &server) {
                return Err(Error::Conflict("promoted server endpoint".into()));
            }
            self.write(&self.dir.join("config.toml"), &global)?;
            let warning = self.write(&source_path, &source).err().map(|error| {
                format!("Server was added, but the source project keeps its own copy of the host: {error}")
            });
            let destination_error = self.write(&destination_path, &destination).err();
            Ok(LinkedHostSave {
                server,
                warning,
                destination_error,
            })
        })();
        // Preserve committed outcomes even if releasing the lock fails.
        match (result, FileExt::unlock(&lock)) {
            (result, Ok(())) => result,
            (Ok(mut saved), Err(error)) => {
                let warning = saved.warning.get_or_insert_with(String::new);
                if !warning.is_empty() {
                    warning.push_str("; ");
                }
                warning.push_str(&format!(
                    "Releasing the configuration lock failed after saving: {error}"
                ));
                Ok(saved)
            }
            (Err(error), Err(release)) => Err(Error::Invalid(format!(
                "{error}; releasing configuration lock failed: {release}"
            ))),
        }
    }

    pub(super) fn project_stores(&self) -> Result<BTreeMap<String, ProjectConfig>> {
        let entries = match fs::read_dir(self.dir.join("projects")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new());
            }
            Err(error) => return Err(error.into()),
        };
        let mut stores = BTreeMap::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            let filename = entry.file_name();
            if filename.as_encoded_bytes().starts_with(b".")
                || !filename.as_encoded_bytes().ends_with(b".toml")
            {
                continue;
            }
            let filename = filename
                .into_string()
                .map_err(|_| Error::Invalid("project store filename is not UTF-8".into()))?;
            let slug = filename.strip_suffix(".toml").unwrap();
            project_store_path(&self.dir, slug)?;
            let store = self
                .read(&entry.path())
                .map_err(|error| Error::Invalid(format!("project store {slug}: {error}")))?;
            stores.insert(slug.into(), store);
        }
        Ok(stores)
    }
    pub fn link_targets(&self, current: &str) -> Result<LinkCatalog> {
        self.with_lock(|| {
            let global = self.global()?;
            RuntimeConfig::resolve(&global, None)?;
            let stores = self.project_stores()?;
            let mut targets = Vec::new();
            let mut servers = global.hosts;
            servers.sort_by(|a, b| a.name.cmp(&b.name));
            for stored in servers {
                let used_by = stores
                    .iter()
                    .filter(|(_, store)| store.hosts.iter().any(|h| h.server == stored.name))
                    .map(|(slug, _)| slug.clone())
                    .collect();
                targets.push(LinkTarget {
                    project: None,
                    host: stored.with_defaults(&global.defaults),
                    used_by,
                    stored,
                    defaults: global.defaults.clone(),
                });
            }
            for (slug, mut store) in stores {
                if slug == current {
                    continue;
                }
                crate::config::validate_hosts(&store.hosts, false)?;
                store.hosts.sort_by(|a, b| a.name.cmp(&b.name));
                for stored in store.hosts.into_iter().filter(|h| h.server.is_empty()) {
                    targets.push(LinkTarget {
                        project: Some(slug.clone()),
                        host: stored.with_defaults(&store.defaults),
                        used_by: vec![],
                        stored,
                        defaults: store.defaults.clone(),
                    });
                }
            }
            let project_names = self
                .registry()?
                .projects
                .into_iter()
                .map(|p| (p.slug, p.name))
                .collect();
            Ok(LinkCatalog {
                targets,
                project_names,
            })
        })
    }
    /// Selecting a project host promotes it; selecting a server only verifies
    /// the snapshot. The destination project is written later by Save host.
    pub fn select_link_target(&self, current: &str, expected: &LinkTarget) -> Result<Promotion> {
        self.with_lock(|| {
            project_store_path(&self.dir, current)?;
            let mut global = self.global()?;
            RuntimeConfig::resolve(&global, None)?;
            let Some(slug) = &expected.project else {
                let host = global.hosts.iter().find(|h| h.name == expected.stored.name);
                if host != Some(&expected.stored) || global.defaults != expected.defaults {
                    return Err(Error::Conflict(format!("server {}", expected.stored.name)));
                }
                return Ok(Promotion { server: expected.stored.with_defaults(&global.defaults), warning: None });
            };
            if slug == current { return Err(Error::Invalid("the host belongs to the open project".into())); }
            let mut stores = self.project_stores()?;
            let source = stores.get_mut(slug).ok_or_else(|| Error::Conflict(format!("project {slug}")))?;
            let index = source.hosts.iter().position(|h| h.name == expected.stored.name).ok_or_else(|| Error::Conflict(format!("host {} of {slug}", expected.stored.name)))?;
            if source.hosts[index] != expected.stored || source.defaults != expected.defaults {
                return Err(Error::Conflict(format!("host {} of {slug}", expected.stored.name)));
            }
            if !expected.stored.server.is_empty() { return Err(Error::Invalid("source host already links a server".into())); }
            let mut server = source.hosts[index].with_defaults(&source.defaults);
            let name = server.name.clone();
            if global.hosts.iter().any(|h| h.name == server.name) {
                server.name = format!("{slug}-{name}");
                let mut suffix = 2;
                while global.hosts.iter().any(|h| h.name == server.name) {
                    server.name = format!("{slug}-{name}-{suffix}");
                    suffix += 1;
                }
            }
            server.mappings.clear();
            source.hosts[index] = Host { name: expected.stored.name.clone(), server: server.name.clone(), root_path: expected.stored.root_path.clone(), mappings: expected.stored.mappings.clone(), ..Host::default() };
            global.hosts.push(server.clone());
            RuntimeConfig::resolve(&global, Some(source))?;
            self.write(&self.dir.join("config.toml"), &global)?;
            let server = server.with_defaults(&global.defaults);
            let warning = self.write(&project_store_path(&self.dir, slug)?, source).err().map(|error| format!("Server {:?} was added, but project {slug} keeps its own copy of the host: {error}", server.name));
            Ok(Promotion { server, warning })
        })
    }
}
