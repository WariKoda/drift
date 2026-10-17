//! Project management and startup policy; all store/filesystem work is bounded.
use crate::browser::{BrowserService, Operation, OperationId};
use drift_core::{
    error::{Error, Result},
    project::{Project, Registry, git_root},
    store::Store,
};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub enum ProjectCommand {
    Load,
    Create {
        name: String,
        path: String,
    },
    Edit {
        expected: Box<Project>,
        name: String,
        path: String,
    },
    Archive {
        expected: Box<Project>,
    },
    Remove {
        expected: Box<Project>,
    },
}
pub struct ProjectResponse {
    pub registry: Registry,
    pub changed: Option<Project>,
    pub warning: Option<String>,
}
#[derive(Clone)]
pub struct StartOptions {
    pub directory: PathBuf,
    pub dashboard: bool,
    pub no_dashboard: bool,
    pub explicit_directory: bool,
}
pub enum Launch {
    Directory(PathBuf),
    Automatic(StartOptions),
}
impl From<PathBuf> for Launch {
    fn from(path: PathBuf) -> Self {
        Self::Directory(path)
    }
}
impl From<StartOptions> for Launch {
    fn from(options: StartOptions) -> Self {
        Self::Automatic(options)
    }
}
pub struct StartOutcome {
    pub directory: PathBuf,
    pub dashboard: bool,
    pub registry: Registry,
}
impl BrowserService {
    pub fn manage_projects(
        &self,
        store: Store,
        command: ProjectCommand,
        id: OperationId,
    ) -> Operation<ProjectResponse> {
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self.runtime.spawn(async move {
            let mutating = !matches!(command, ProjectCommand::Load);
            let action = move || {
                let (registry, changed, warning) = match command {
                    ProjectCommand::Load => {
                        let registry = store.registry()?;
                        registry.validate()?;
                        (registry, None, None)
                    }
                    ProjectCommand::Create { name, path } => {
                        let project =
                            store.register(&name, drift_core::project::expand_path(&path)?)?;
                        (store.registry()?, Some(project), None)
                    }
                    ProjectCommand::Edit {
                        expected,
                        name,
                        path,
                    } => (
                        store.edit_project(&expected, &name, &path)?,
                        Some(*expected),
                        None,
                    ),
                    ProjectCommand::Archive { expected } => {
                        (store.archive_project(&expected)?, Some(*expected), None)
                    }
                    ProjectCommand::Remove { expected } => {
                        let removed = store.remove_project(&expected)?;
                        (removed.registry, Some(*expected), removed.warning)
                    }
                };
                Ok(ProjectResponse {
                    registry,
                    changed,
                    warning,
                })
            };
            if mutating {
                service.blocking_mutation(&token, action).await
            } else {
                service.blocking(&token, action).await
            }
        });
        Operation { id, cancel, task }
    }
    pub fn resolve_start(
        &self,
        store: Store,
        options: StartOptions,
        id: OperationId,
    ) -> Operation<StartOutcome> {
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self.runtime.spawn(async move {
            service
                .blocking(&token, move || resolve_start(&store, options))
                .await
        });
        Operation { id, cancel, task }
    }
}
fn resolve_start(store: &Store, options: StartOptions) -> Result<StartOutcome> {
    let options = StartOptions {
        directory: drift_core::project::clean_path(&options.directory),
        ..options
    };
    let registry = store.registry()?;
    registry.validate()?;
    // Surface malformed configuration before offering a dashboard.
    store.runtime(
        registry
            .find_by_path(&options.directory)
            .map(|p| p.slug.as_str()),
    )?;
    let mut directory = options.directory;
    let dashboard = if options.no_dashboard {
        false
    } else if options.dashboard {
        true
    } else if options.explicit_directory
        || registry.find_by_path(&directory).is_some()
        || git_root(&directory)?
            .is_some_and(|root| !registry.projects.iter().any(|p| p.path == root))
    {
        false
    } else if let Some(last) = registry.most_recently_opened() {
        match std::fs::metadata(&last.path) {
            Ok(metadata) if metadata.is_dir() => {
                directory = last.path.clone();
                false
            }
            Ok(_) => !registry.projects.is_empty(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                !registry.projects.is_empty()
            }
            Err(error) => {
                return Err(Error::Invalid(format!(
                    "cannot inspect last project: {error}"
                )));
            }
        }
    } else {
        !registry.projects.is_empty()
    };
    Ok(StartOutcome {
        directory,
        dashboard,
        registry,
    })
}
