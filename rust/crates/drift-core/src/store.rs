use crate::{
    config::{Defaults, GlobalConfig, Host, ProjectConfig, RuntimeConfig, project_store_path},
    error::{Error, Result},
    project::{Project, Registry, now},
};
use fs2::FileExt;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
}
/// One consistent, raw snapshot for a host management form.
pub struct HostCatalog {
    pub hosts: Vec<Host>,
    pub servers: Vec<Host>,
    pub defaults: Defaults,
    pub runtime: RuntimeConfig,
}
impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn global(&self) -> Result<GlobalConfig> {
        self.read(&self.dir.join("config.toml"))
    }
    pub fn project(&self, slug: &str) -> Result<ProjectConfig> {
        self.read(&project_store_path(&self.dir, slug)?)
    }
    pub fn registry(&self) -> Result<Registry> {
        self.read(&self.dir.join("projects.toml"))
    }
    pub fn runtime(&self, slug: Option<&str>) -> Result<RuntimeConfig> {
        let global = self.global()?;
        let project = slug.map(|slug| self.project(slug)).transpose()?;
        RuntimeConfig::resolve(&global, project.as_ref())
    }
    pub fn host_catalog(&self, slug: Option<&str>) -> Result<HostCatalog> {
        self.with_lock(|| {
            let global = self.global()?;
            let project = slug.map(|slug| self.project(slug)).transpose()?;
            let runtime = RuntimeConfig::resolve(&global, project.as_ref())?;
            let (hosts, defaults) = match project {
                Some(project) => (project.hosts, project.defaults),
                None => (global.hosts.clone(), global.defaults),
            };
            Ok(HostCatalog {
                hosts,
                servers: global.hosts,
                defaults,
                runtime,
            })
        })
    }
    fn read<T: DeserializeOwned + Default>(&self, path: &Path) -> Result<T> {
        match fs::read_to_string(path) {
            Ok(data) => Ok(toml::from_str(&data)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(error) => Err(error.into()),
        }
    }
    /// flock is held until the File is dropped, including every error path.
    /// Nonblocking acquisition makes contention visible, never freezes the UI.
    fn lock(&self) -> Result<File> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.dir)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.dir.join("write.lock"))?;
        file.try_lock_exclusive().map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                Error::Busy
            } else {
                e.into()
            }
        })?;
        Ok(file)
    }
    fn with_lock<T>(&self, action: impl FnOnce() -> Result<T>) -> Result<T> {
        let lock = self.lock()?;
        let result = action();
        // A concurrently forked child can briefly inherit the descriptor before
        // exec. Unlock the shared open-file description explicitly, rather than
        // relying on the final descriptor close in that child.
        let released = FileExt::unlock(&lock);
        match (result, released) {
            (result, Ok(())) => result,
            (Ok(_), Err(error)) => Err(error.into()),
            (Err(error), Err(release)) => Err(Error::Invalid(format!(
                "{error}; releasing configuration lock failed: {release}"
            ))),
        }
    }
    fn write<T: Serialize>(&self, path: &Path, value: &T) -> Result<()> {
        let data = toml::to_string_pretty(value)?;
        let parent = path
            .parent()
            .ok_or_else(|| Error::Invalid("store has no parent directory".into()))?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
        let mut stage = tempfile::NamedTempFile::new_in(parent)?;
        stage
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        stage.write_all(data.as_bytes())?;
        stage.as_file().sync_all()?;
        stage.persist(path).map_err(|e| Error::Io(e.error))?;
        Ok(())
    }
    pub fn save_host(
        &self,
        slug: Option<&str>,
        expected: Option<&Host>,
        desired: Host,
    ) -> Result<()> {
        self.with_lock(|| {
            let mut global = self.global()?;
            if let Some(slug) = slug {
                let mut project = self.project(slug)?;
                replace_host(&mut project.hosts, expected, Some(desired))?;
                RuntimeConfig::resolve(&global, Some(&project))?;
                self.write(&project_store_path(&self.dir, slug)?, &project)
            } else {
                if expected.is_some_and(|before| before.name != desired.name) {
                    self.ensure_server_unused(&expected.unwrap().name)?;
                }
                replace_host(&mut global.hosts, expected, Some(desired))?;
                RuntimeConfig::resolve(&global, None)?;
                self.write(&self.dir.join("config.toml"), &global)
            }
        })
    }
    pub fn delete_host(&self, slug: Option<&str>, expected: &Host) -> Result<()> {
        self.with_lock(|| {
            let mut global = self.global()?;
            if let Some(slug) = slug {
                let mut project = self.project(slug)?;
                replace_host(&mut project.hosts, Some(expected), None)?;
                RuntimeConfig::resolve(&global, Some(&project))?;
                self.write(&project_store_path(&self.dir, slug)?, &project)
            } else {
                self.ensure_server_unused(&expected.name)?;
                replace_host(&mut global.hosts, Some(expected), None)?;
                RuntimeConfig::resolve(&global, None)?;
                self.write(&self.dir.join("config.toml"), &global)
            }
        })
    }
    fn ensure_server_unused(&self, name: &str) -> Result<()> {
        let entries = match fs::read_dir(self.dir.join("projects")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();
            if filename.starts_with('.')
                || !filename.ends_with(".toml")
                || entry.file_type()?.is_dir()
            {
                continue;
            }
            let project: ProjectConfig = self.read(&entry.path())?;
            if project.hosts.iter().any(|h| h.server == name) {
                return Err(Error::Invalid(format!(
                    "server {name:?} is linked by {}",
                    filename.trim_end_matches(".toml")
                )));
            }
        }
        Ok(())
    }
    pub fn save_project(&self, expected: Option<&Project>, desired: Project) -> Result<()> {
        self.with_lock(|| {
            let mut registry = self.registry()?;
            if let Some(expected) = expected {
                let current = registry
                    .find(&expected.slug)
                    .ok_or_else(|| Error::Conflict(format!("project {}", expected.slug)))?;
                if current != expected {
                    return Err(Error::Conflict(format!("project {}", expected.slug)));
                }
                if desired.slug != expected.slug {
                    return Err(Error::Invalid(
                        "editing cannot change a project slug".into(),
                    ));
                }
                let index = registry
                    .projects
                    .iter()
                    .position(|p| p.slug == expected.slug)
                    .unwrap();
                registry.projects[index] = desired;
            } else {
                if registry.find(&desired.slug).is_some() {
                    return Err(Error::Conflict(format!("project {}", desired.slug)));
                }
                registry.projects.push(desired);
            }
            registry.validate()?;
            self.write(&self.dir.join("projects.toml"), &registry)
        })
    }
    pub fn register(&self, name: &str, path: PathBuf) -> Result<Project> {
        self.with_lock(|| {
            let mut registry = self.registry()?;
            let date = now();
            let project = Project {
                slug: registry.unique_slug(name),
                name: name.into(),
                path,
                archived: false,
                created_at: date,
                updated_at: date,
                opened_at: None,
            };
            registry.projects.push(project.clone());
            registry.validate()?;
            self.write(&self.dir.join("projects.toml"), &registry)?;
            Ok(project)
        })
    }
    pub fn mark_opened(&self, slug: &str) -> Result<()> {
        self.with_lock(|| {
            let mut registry = self.registry()?;
            let project = registry
                .projects
                .iter_mut()
                .find(|p| p.slug == slug)
                .ok_or_else(|| Error::Conflict(format!("project {slug}")))?;
            project.opened_at = Some(now());
            self.write(&self.dir.join("projects.toml"), &registry)
        })
    }
}
fn replace_host(
    hosts: &mut Vec<Host>,
    expected: Option<&Host>,
    desired: Option<Host>,
) -> Result<()> {
    let index = if let Some(expected) = expected {
        let index = hosts
            .iter()
            .position(|h| h.name == expected.name)
            .ok_or_else(|| Error::Conflict(format!("host {}", expected.name)))?;
        if hosts[index] != *expected {
            return Err(Error::Conflict(format!("host {}", expected.name)));
        }
        Some(index)
    } else {
        None
    };
    if let Some(desired) = desired {
        if hosts
            .iter()
            .enumerate()
            .any(|(i, h)| h.name == desired.name && Some(i) != index)
        {
            return Err(Error::Invalid(format!("duplicate host {:?}", desired.name)));
        }
        if let Some(index) = index {
            hosts[index] = desired;
        } else {
            hosts.push(desired);
        }
    } else if let Some(index) = index {
        hosts.remove(index);
    }
    Ok(())
}
