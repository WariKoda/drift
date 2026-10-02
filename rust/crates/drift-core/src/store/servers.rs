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
impl Store {
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
