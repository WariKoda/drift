//! Background-only local browsing. GPUI receives values and operation handles;
//! capability roots and blocking work remain in this application layer.
use crate::hosts::{HostCommand, HostResponse};
use drift_core::{
    config::RuntimeConfig,
    error::{Error, Result},
    local::{Entry, ProjectRoot, skip_directory},
    project::Registry,
    staging::is_staging_name,
    store::Store,
};
use std::{
    future::Future,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    runtime::{Builder, Runtime},
    sync::Semaphore,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationId {
    pub project: u64,
    pub operation: u64,
}
#[derive(Clone)]
pub struct BrowserService {
    pub(crate) runtime: Arc<BackgroundRuntime>,
    permits: Arc<Semaphore>,
    logger: crate::logging::Logger,
}
pub(crate) struct BackgroundRuntime {
    runtime: Option<Runtime>,
    pub(crate) tasks: TaskTracker,
}
impl Deref for BackgroundRuntime {
    type Target = Runtime;
    fn deref(&self) -> &Runtime {
        self.runtime
            .as_ref()
            .expect("runtime exists until shutdown")
    }
}
impl BackgroundRuntime {
    pub(crate) fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn_on(future, self.handle())
    }
    pub(crate) fn spawn_blocking<F, T>(&self, action: F) -> tokio::task::JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.tasks.spawn_blocking_on(action, self.handle())
    }
}
impl Drop for BackgroundRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}
#[derive(Clone)]
pub struct Location {
    pub root: Arc<ProjectRoot>,
    pub directory: PathBuf,
    pub slug: Option<String>,
    pub config: RuntimeConfig,
}
impl Location {
    pub fn parent_directory(&self) -> Option<PathBuf> {
        let directory = self.directory.components().collect::<PathBuf>();
        let parent = directory.parent()?;
        if self.slug.is_some()
            && (directory == self.root.base() || !parent.starts_with(self.root.base()))
        {
            return None;
        }
        Some(parent.into())
    }
}
pub struct Directory {
    pub location: Location,
    pub entries: Vec<Entry>,
    pub registry: Registry,
}
pub struct Operation<T> {
    pub id: OperationId,
    pub cancel: CancellationToken,
    pub task: tokio::task::JoinHandle<Result<T>>,
}
impl BrowserService {
    pub fn manage_hosts(
        &self,
        store: Store,
        slug: Option<String>,
        command: HostCommand,
        id: OperationId,
    ) -> Operation<HostResponse> {
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self.runtime.spawn(async move {
            let mutating = !matches!(command, HostCommand::Load | HostCommand::LinkTargets);
            let action = move || match command {
                HostCommand::Load => store
                    .host_catalog(slug.as_deref())
                    .map(Box::new)
                    .map(HostResponse::Loaded),
                HostCommand::Save { expected, desired } => {
                    store.save_host(slug.as_deref(), expected.as_deref(), *desired)?;
                    Ok(HostResponse::Saved)
                }
                HostCommand::Delete { expected } => {
                    store.delete_host(slug.as_deref(), &expected)?;
                    Ok(HostResponse::Deleted)
                }
                HostCommand::LinkTargets => store
                    .link_targets(
                        slug.as_deref()
                            .ok_or_else(|| Error::Invalid("open a project to link hosts".into()))?,
                    )
                    .map(Box::new)
                    .map(HostResponse::LinkTargets),
                HostCommand::SelectLink { expected } => store
                    .select_link_target(
                        slug.as_deref()
                            .ok_or_else(|| Error::Invalid("open a project to link hosts".into()))?,
                        &expected,
                    )
                    .map(Box::new)
                    .map(HostResponse::LinkSelected),
            };
            if mutating {
                service.blocking_mutation(&token, action).await
            } else {
                service.blocking(&token, action).await
            }
        });
        Operation { id, cancel, task }
    }
    pub fn new() -> Result<Self> {
        Ok(Self {
            runtime: Arc::new(BackgroundRuntime {
                runtime: Some(
                    Builder::new_multi_thread()
                        .worker_threads(2)
                        .max_blocking_threads(4)
                        .enable_all()
                        .build()?,
                ),
                tasks: TaskTracker::new(),
            }),
            permits: Arc::new(Semaphore::new(4)),
            logger: crate::logging::Logger::default(),
        })
    }
    pub fn with_logger(mut self, logger: crate::logging::Logger) -> Self {
        self.logger = logger;
        self
    }
    pub fn logger(&self) -> &crate::logging::Logger {
        &self.logger
    }
    /// Call after views have cancelled/dropped their operations and the GUI loop has ended.
    /// Terminal outcomes and transport cleanup must precede closing the diagnostic writer.
    pub fn wait_for_shutdown(&self) {
        self.runtime.tasks.close();
        self.runtime.block_on(self.runtime.tasks.wait());
    }
    pub fn open(
        &self,
        store: Store,
        directory: PathBuf,
        id: OperationId,
        show_hidden: bool,
        show_ignored: bool,
    ) -> Operation<Directory> {
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self.runtime.spawn(async move {
            let prepare_store = store.clone();
            let mut result = service
                .blocking(&token, move || {
                    let directory = drift_core::project::clean_path(&directory);
                    let registry = prepare_store.registry()?;
                    let project = registry.find_by_path(&directory);
                    let base = project.map_or(directory.clone(), |p| {
                        drift_core::project::clean_path(&p.path)
                    });
                    let slug = project.map(|p| p.slug.clone());
                    let config = prepare_store.runtime(slug.as_deref())?;
                    let root = Arc::new(ProjectRoot::open(&base)?);
                    let location = Location {
                        root,
                        directory,
                        slug,
                        config,
                    };
                    Ok(Directory {
                        location,
                        entries: vec![],
                        registry,
                    })
                })
                .await?;
            result.entries = service
                .list(&token, &result.location, show_hidden, show_ignored)
                .await?;
            if let Some(slug) = result.location.slug.clone() {
                result.registry = service
                    .blocking_mutation(&token, move || {
                        store.mark_opened(&slug)?;
                        store.registry()
                    })
                    .await?;
            }
            Ok(result)
        });
        Operation { id, cancel, task }
    }
    pub fn read_directory(
        &self,
        store: Store,
        location: Location,
        id: OperationId,
        show_hidden: bool,
        show_ignored: bool,
    ) -> Operation<Directory> {
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self.runtime.spawn(async move {
            let entries = service
                .list(&token, &location, show_hidden, show_ignored)
                .await?;
            service
                .blocking(&token, move || {
                    let mut location = location;
                    location.config = store.host_catalog(location.slug.as_deref())?.runtime;
                    let registry = store.registry()?;
                    Ok(Directory {
                        location,
                        entries,
                        registry,
                    })
                })
                .await
        });
        Operation { id, cancel, task }
    }
    pub fn preview(&self, location: Location, path: PathBuf, id: OperationId) -> Operation<String> {
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self.runtime.spawn(async move {
            service
                .blocking(&token, move || location.root.preview(&path))
                .await
        });
        Operation { id, cancel, task }
    }
    pub fn register(
        &self,
        store: Store,
        name: String,
        path: PathBuf,
        id: OperationId,
    ) -> Operation<PathBuf> {
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self.runtime.spawn(async move {
            service
                .blocking_mutation(&token, move || {
                    let path = drift_core::project::git_root(&path)?.unwrap_or(path);
                    store.register(&name, path.clone())?;
                    Ok(path)
                })
                .await
        });
        Operation { id, cancel, task }
    }
    pub(crate) async fn blocking<T: Send + 'static>(
        &self,
        cancel: &CancellationToken,
        action: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = tokio::select! {
            _ = cancel.cancelled() => return Err(Error::Invalid("operation cancelled".into())),
            permit = self.permits.clone().acquire_owned() => permit.map_err(|_| Error::Invalid("background worker closed".into()))?,
        };
        let token = cancel.clone();
        let task = self.runtime.spawn_blocking(move || {
            let _permit = permit;
            if token.is_cancelled() {
                return Err(Error::Invalid("operation cancelled".into()));
            }
            action()
        });
        tokio::select! {
            _ = cancel.cancelled() => Err(Error::Invalid("operation cancelled".into())),
            result = task => result.map_err(|e| Error::Invalid(format!("background operation failed: {e}")))?,
        }
    }
    /// Mutations must finish before a sync reports their outcome. Cancellation
    /// can stop admission, but cannot detach an already-running commit/delete.
    pub(crate) async fn blocking_mutation<T: Send + 'static>(
        &self,
        cancel: &CancellationToken,
        action: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(Error::Invalid("operation cancelled".into())),
            permit = self.permits.clone().acquire_owned() => permit.map_err(|_| Error::Invalid("background worker closed".into()))?,
        };
        let token = cancel.clone();
        self.runtime
            .spawn_blocking(move || {
                let _permit = permit;
                if token.is_cancelled() {
                    return Err(Error::Invalid("operation cancelled".into()));
                }
                action()
            })
            .await
            .map_err(|e| Error::Invalid(format!("background mutation failed: {e}")))?
    }
    async fn list(
        &self,
        cancel: &CancellationToken,
        location: &Location,
        show_hidden: bool,
        show_ignored: bool,
    ) -> Result<Vec<Entry>> {
        let root = location.root.clone();
        let directory = location.directory.clone();
        let mut entries = self
            .blocking(cancel, move || root.entries(&directory))
            .await?;
        entries.retain(|e| show_hidden || !e.path.components().any(|c| matches!(c, std::path::Component::Normal(name) if name.to_string_lossy().starts_with('.'))));
        if !show_ignored && !entries.is_empty() {
            let ignored = classify_ignored(cancel, location.root.base(), &entries).await?;
            entries = entries
                .into_iter()
                .zip(ignored)
                .filter_map(|(entry, ignored)| (!ignored).then_some(entry))
                .collect();
        }
        Ok(entries)
    }
    pub fn find(
        &self,
        location: Location,
        id: OperationId,
        show_hidden: bool,
        show_ignored: bool,
    ) -> Operation<Vec<Entry>> {
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self.runtime.spawn(async move {
            let root = location.root.clone(); let walk_cancel = token.clone();
            let mut files = service.blocking(&token, move || {
                let mut directories = vec![PathBuf::from(".")]; let mut files = Vec::new();
                while let Some(dir) = directories.pop() {
                    if walk_cancel.is_cancelled() { return Err(Error::Invalid("operation cancelled".into())); }
                    for entry in root.entries(&dir)? {
                        if !show_hidden && entry.path.components().any(|c| matches!(c, std::path::Component::Normal(name) if name.to_string_lossy().starts_with('.'))) { continue; }
                        if entry.directory { directories.push(entry.path); }
                        else if !entry.symlink { files.push(entry); }
                    }
                }
                files.sort_by(|a,b| a.path.cmp(&b.path));
                Ok(files)
            }).await?;
            if !show_ignored && !files.is_empty() {
                let ignored = classify_ignored(&token, location.root.base(), &files).await?;
                files = files.into_iter().zip(ignored).filter_map(|(entry, ignored)| (!ignored).then_some(entry)).collect();
            }
            Ok(files)
        });
        Operation { id, cancel, task }
    }
}

/// Git owns ignore semantics (including tracked files, worktrees and global
/// excludes). One NUL-delimited query per scan also supports remote-only paths.
pub(crate) async fn classify_ignored(
    cancel: &CancellationToken,
    root: &Path,
    entries: &[Entry],
) -> Result<Vec<bool>> {
    let mut discover = tokio::process::Command::new("git");
    discover
        .args(["-C"])
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .kill_on_drop(true);
    let output = tokio::select! {
        _ = cancel.cancelled() => return Err(Error::Invalid("operation cancelled".into())),
        output = discover.output() => output?,
    };
    let mut command = tokio::process::Command::new("git");
    // Let Git resolve worktree prefixes itself. Its reported canonical root
    // may differ from the opened project's spelling (e.g. /var on macOS).
    // Local file access remains through ProjectRoot's directory capability.
    command.arg("-C").arg(root);
    let temporary = if output.status.success() {
        None
    } else if output.status.code() == Some(128)
        && String::from_utf8_lossy(&output.stderr)
            .to_lowercase()
            .contains("not a git repository")
    {
        let dir = tempfile::tempdir()?;
        let mut init = tokio::process::Command::new("git");
        init.args(["init", "--bare", "--quiet"])
            .arg(dir.path())
            .kill_on_drop(true);
        let init = tokio::select! { _ = cancel.cancelled() => return Err(Error::Invalid("operation cancelled".into())), output = init.output() => output? };
        if !init.status.success() {
            return Err(Error::Invalid(format!(
                "initialize Git ignore metadata: {}",
                String::from_utf8_lossy(&init.stderr)
            )));
        }
        command
            .arg(format!("--git-dir={}", dir.path().display()))
            .arg(format!("--work-tree={}", root.display()))
            .args(["-c", "core.excludesFile=/dev/null"]);
        command.arg("check-ignore").arg("--no-index");
        Some(dir)
    } else {
        return Err(Error::Invalid(format!(
            "find Git worktree: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    };
    if temporary.is_none() {
        command.arg("check-ignore");
    }
    command
        .args(["--stdin", "-z", "--verbose", "--non-matching"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut input = Vec::new();
    for entry in entries {
        let relative = entry
            .path
            .to_str()
            .ok_or_else(|| Error::Invalid("ignore path is not UTF-8".into()))?;
        if relative.contains('\0') {
            return Err(Error::Invalid("ignore path contains NUL".into()));
        }
        input.extend_from_slice(if relative.is_empty() {
            b"."
        } else {
            relative.as_bytes()
        });
        if entry.directory {
            input.push(b'/');
        }
        input.push(0);
    }
    let mut child = command.spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::Invalid("Git stdin unavailable".into()))?;
    use tokio::io::AsyncWriteExt;
    // Read output concurrently: a large finder query must not deadlock on full
    // stdout/stderr pipes while its stdin is still being written.
    let (written, output) = tokio::select! {
        _ = cancel.cancelled() => return Err(Error::Invalid("operation cancelled".into())),
        result = async {
            let write = async { stdin.write_all(&input).await?; drop(stdin); Ok::<(), std::io::Error>(()) };
            tokio::join!(write, child.wait_with_output())
        } => result,
    };
    let output = output?;
    // Git may reject its index before consuming stdin. Prefer that failure to
    // its secondary broken pipe; a successful process still requires a full write.
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(Error::Invalid(format!(
            "Git ignore classification: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    written?;
    let fields: Vec<_> = output.stdout.split(|b| *b == 0).collect();
    if fields.len() != entries.len() * 4 + 1 || fields.last() != Some(&&b""[..]) {
        return Err(Error::Invalid("malformed Git ignore output".into()));
    }
    Ok(fields[..fields.len() - 1]
        .chunks(4)
        .map(|record| !record[0].is_empty() && !record[2].starts_with(b"!"))
        .collect())
}

#[cfg(test)]
mod tests;

pub fn hard_excluded(path: &Path, directory: bool) -> bool {
    let parts: Vec<_> = path.components().collect();
    parts.iter().enumerate().any(|(i, c)| {
        let name = c.as_os_str().to_string_lossy();
        (skip_directory(&name)
            && (i + 1 < parts.len() || directory || matches!(&*name, ".git" | ".svn" | ".hg")))
            || (!directory && i + 1 == parts.len() && is_staging_name(&name))
    })
}
