//! Project mutations preserve slug identity and coordinate registry/store writes.
use super::*;

pub struct ProjectRemoval {
    pub registry: Registry,
    /// The registry removal committed, but deleting the hidden store failed.
    pub warning: Option<String>,
}
impl Store {
    pub fn edit_project(&self, expected: &Project, name: &str, path: &str) -> Result<Registry> {
        let mut desired = expected.clone();
        desired.name = name.trim().into();
        desired.path = crate::project::expand_path(path)?;
        desired.updated_at = now();
        self.with_lock(|| {
            let mut registry = self.registry()?;
            if registry.find(&expected.slug) != Some(expected) {
                return Err(Error::Conflict(format!("project {}", expected.slug)));
            }
            let index = registry
                .projects
                .iter()
                .position(|p| p.slug == expected.slug)
                .unwrap();
            registry.projects[index] = desired;
            registry.validate()?;
            self.write(&self.dir.join("projects.toml"), &registry)?;
            Ok(registry)
        })
    }
    pub fn archive_project(&self, expected: &Project) -> Result<Registry> {
        self.with_lock(|| {
            let mut registry = self.registry()?;
            let project = registry
                .projects
                .iter_mut()
                .find(|p| p.slug == expected.slug)
                .ok_or_else(|| Error::Conflict(format!("project {}", expected.slug)))?;
            if project != expected {
                return Err(Error::Conflict(format!("project {}", expected.slug)));
            }
            project.archived = !project.archived;
            project.updated_at = now();
            registry.validate()?;
            self.write(&self.dir.join("projects.toml"), &registry)?;
            Ok(registry)
        })
    }
    pub fn remove_project(&self, expected: &Project) -> Result<ProjectRemoval> {
        self.with_lock(|| {
            let mut registry = self.registry()?;
            if registry.find(&expected.slug) != Some(expected) {
                return Err(Error::Conflict(format!("project {}", expected.slug)));
            }
            registry.projects.retain(|p| p.slug != expected.slug);
            registry.validate()?;
            let path = project_store_path(&self.dir, &expected.slug)?;
            let staged = match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
                Ok(metadata) => {
                    if !metadata.is_file() {
                        return Err(Error::Invalid("project store is not a regular file".into()));
                    }
                    let stage = tempfile::Builder::new()
                        .prefix(&format!(".{}.deleting-", expected.slug))
                        .tempfile_in(path.parent().unwrap())?;
                    let stage = stage
                        .into_temp_path()
                        .keep()
                        .map_err(|error| Error::Io(error.error))?;
                    fs::remove_file(&stage)?;
                    fs::rename(&path, &stage)?;
                    Some(stage)
                }
            };
            if let Err(error) = self.write(&self.dir.join("projects.toml"), &registry) {
                let rollback = staged
                    .as_ref()
                    .and_then(|stage| fs::rename(stage, &path).err())
                    .map(Error::Io);
                return Err(Error::join(error, rollback));
            }
            let warning = staged.and_then(|stage| {
                fs::remove_file(&stage).err().map(|error| {
                    format!(
                        "Project removed, but deleting hidden settings {} failed: {error}",
                        stage.display()
                    )
                })
            });
            Ok(ProjectRemoval { registry, warning })
        })
    }
}
