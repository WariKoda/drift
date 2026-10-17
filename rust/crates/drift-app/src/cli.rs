//! Headless project commands reuse the GUI's transactional stores.
use crate::projects::StartOptions;
use drift_core::{
    error::{Error, Result},
    project::expand_path,
    store::Store,
};

#[derive(Debug, PartialEq, Eq)]
pub enum ProjectCommand {
    List,
    Add {
        name: String,
        path: String,
    },
    Edit {
        slug: String,
        name: Option<String>,
        path: Option<String>,
    },
    Archive(String),
    Remove(String),
    Open(String),
}
pub struct Response {
    pub output: String,
    /// Removal committed, but cleanup of hidden settings failed.
    pub warning: Option<String>,
    pub start: Option<StartOptions>,
}
/// Invoked before starting GPUI. No GUI or background runtime is needed.
pub fn run(store: &Store, command: ProjectCommand) -> Result<Response> {
    let mut response = Response {
        output: String::new(),
        warning: None,
        start: None,
    };
    if let ProjectCommand::Add { name, path } = command {
        let project = store.register(&name, expand_path(&path)?)?;
        response.output = format!(
            "Added project {:?} ({}) → {}\n",
            project.name,
            project.slug,
            project.path.display()
        );
        return Ok(response);
    }
    let registry = store.registry()?;
    registry.validate()?;
    match command {
        ProjectCommand::List => {
            if registry.projects.is_empty() {
                response.output =
                    "No projects registered. Add one with: drift-gui projects add <name> [path]\n"
                        .into();
            } else {
                response.output = "SLUG\tNAME\tPATH\tSTATUS\n".into();
                for project in registry.all() {
                    let status = match std::fs::metadata(&project.path) {
                        Ok(metadata) if metadata.is_dir() => {
                            if project.archived {
                                "archived"
                            } else {
                                "active"
                            }
                        }
                        Ok(_) => "missing",
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing",
                        Err(error) => return Err(error.into()),
                    };
                    response.output.push_str(&format!(
                        "{}\t{}\t{}\t{status}\n",
                        project.slug,
                        project.name,
                        project.path.display()
                    ));
                }
            }
        }
        ProjectCommand::Open(query) => {
            let project = registry.match_query(&query)?;
            if !std::fs::metadata(&project.path)
                .map_err(|error| {
                    Error::Invalid(format!(
                        "cannot open project {:?} at {}: {error}",
                        project.slug,
                        project.path.display()
                    ))
                })?
                .is_dir()
            {
                return Err(Error::Invalid(format!(
                    "project path is not a directory: {}",
                    project.path.display()
                )));
            }
            // Fail before opening a window when settings are malformed. The GUI
            // owns mark_opened and only writes it after a successful root load.
            store.runtime(Some(&project.slug))?;
            response.start = Some(StartOptions {
                directory: project.path.clone(),
                dashboard: false,
                no_dashboard: true,
                explicit_directory: true,
            });
        }
        ProjectCommand::Edit { slug, name, path } => {
            let project = registry
                .find(&slug)
                .ok_or_else(|| Error::Invalid(format!("no project with slug {slug:?}")))?;
            let path = match path {
                Some(path) => path,
                None => project
                    .path
                    .to_str()
                    .ok_or_else(|| Error::Invalid("project path is not UTF-8".into()))?
                    .to_owned(),
            };
            store.edit_project(project, name.as_deref().unwrap_or(&project.name), &path)?;
            response.output = format!("Updated project {slug:?}\n");
        }
        ProjectCommand::Archive(slug) => {
            let project = registry
                .find(&slug)
                .ok_or_else(|| Error::Invalid(format!("no project with slug {slug:?}")))?;
            let updated = store.archive_project(project)?;
            let status = if updated.find(&slug).unwrap().archived {
                "archived"
            } else {
                "active"
            };
            response.output = format!("Project {slug:?} is now {status}\n");
        }
        ProjectCommand::Remove(slug) => {
            let project = registry
                .find(&slug)
                .ok_or_else(|| Error::Invalid(format!("no project with slug {slug:?}")))?;
            let removed = store.remove_project(project)?;
            response.output = format!("Removed project {slug:?}\n");
            response.warning = removed.warning;
        }
        ProjectCommand::Add { .. } => unreachable!("handled before registry load"),
    }
    Ok(response)
}
