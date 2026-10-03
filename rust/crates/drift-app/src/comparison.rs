//! Selection expansion, bounded comparison workers and comparison ownership.
//! This module knows neither GPUI nor the concrete transport implementation.
use crate::{
    browser::{BrowserService, Location, Operation, OperationId, classify_ignored, hard_excluded},
    remote::RemoteSession,
};
use drift_core::{
    config::Host,
    diff::{DiffResult, FileMetadata, TEXT_LIMIT},
    error::{Error, Result},
    local::{Entry, ProjectRoot},
    pathmap::Mapper,
    remote::ConnectionState,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncReadExt, sync::watch, task::JoinSet};
use tokio_util::sync::CancellationToken;

/// Browser mark eligibility shares the comparison's mapping and exclusion policy.
/// No remote I/O or local filesystem access is needed for this lexical check.
pub fn remote_markable_paths(
    location: &Location,
    host: &Host,
    remote_root: &str,
    paths: Vec<String>,
) -> Result<BTreeSet<String>> {
    let base = location
        .root
        .base()
        .to_str()
        .ok_or_else(|| Error::Invalid("project path is not UTF-8".into()))?;
    let mapper = Mapper::new(base, remote_root, &location.config.mappings, &host.mappings)?;
    Ok(paths
        .into_iter()
        .filter(|path| {
            mapper.remote_to_local(path).ok().is_some_and(|local| {
                let relative = std::path::Path::new(&local).strip_prefix(location.root.base());
                relative.is_ok_and(|relative| !hard_excluded(relative, false))
            })
        })
        .collect())
}

#[derive(Clone, Copy, Default)]
pub struct ScopeOptions {
    pub include_ignored: bool,
}
#[derive(Clone)]
pub struct LoadRequest {
    pub location: Location,
    pub host: Host,
    pub connection: RemoteSession,
    /// Empty selections compare the entire project, restricted by mappings.
    pub local: Vec<PathBuf>,
    pub remote: Vec<String>,
    pub scope: ScopeOptions,
}
#[derive(Clone, Debug, Default)]
pub struct ScopeSummary {
    pub pairs: usize,
    pub hidden: usize,
    pub ignored_skipped: usize,
    pub hard_excluded_skipped: usize,
    pub explicit_ignored: usize,
}
pub struct ComparisonEntry {
    pub local: PathBuf,
    pub remote: String,
    pub result: Option<Arc<DiffResult>>,
    pub error: Option<String>,
}
pub struct ComparisonSession {
    pub request: LoadRequest,
    pub entries: Vec<ComparisonEntry>,
    pub scope: ScopeSummary,
}
impl ComparisonSession {
    pub async fn shutdown(self) {
        self.request.connection.shutdown().await;
    }
}
#[derive(Clone, Debug)]
pub struct Progress {
    pub phase: &'static str,
    pub completed: usize,
    pub total: usize,
    pub bytes: u64,
}
pub struct ComparisonOperation {
    pub operation: Operation<ComparisonSession>,
    pub progress: watch::Receiver<Progress>,
}
#[derive(Clone)]
pub struct ComparisonService {
    browser: BrowserService,
}
impl ComparisonService {
    pub fn new(browser: BrowserService) -> Self {
        Self { browser }
    }
    pub fn load(&self, request: LoadRequest, id: OperationId) -> ComparisonOperation {
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let (progress, receiver) = watch::channel(Progress {
            phase: "Scanning",
            completed: 0,
            total: 0,
            bytes: 0,
        });
        let mut activity = receiver.clone();
        let task = self.browser.runtime.spawn(async move {
            let project = id.project.to_string();
            let operation = id.operation.to_string();
            let port = request.host.port.to_string();
            let local_path = request.location.root.base().to_string_lossy();
            let fields = [
                ("host", request.host.name.as_str()),
                ("endpoint", request.host.hostname.as_str()),
                ("port", port.as_str()),
                ("protocol", request.host.protocol.as_str()),
                ("local_path", local_path.as_ref()),
                ("remote_path", request.connection.root.as_str()),
                ("project", project.as_str()),
                ("operation", operation.as_str()),
            ];
            service.browser.logger().info("comparison.start", &fields);
            let idle = async {
                while let Ok(Ok(())) = tokio::time::timeout(Duration::from_secs(60), activity.changed()).await {}
            };
            let result = tokio::select! {
                biased;
                _ = token.cancelled() => Err(Error::Invalid("comparison cancelled; connection closed".into())),
                result = service.compare(&request, &token, &progress, id) => result,
                _ = idle => Err(Error::Invalid("comparison made no progress for 60 seconds; connection closed".into())),
            };
            match result {
                Ok((entries, scope)) => {
                    let count = entries.len().to_string();
                    let failed = entries.iter().filter(|entry| entry.error.is_some()).count().to_string();
                    let pairs = scope.pairs.to_string();
                    let mut completed_fields = fields.to_vec();
                    completed_fields.extend([
                        ("entries", count.as_str()),
                        ("failed", failed.as_str()),
                        ("pairs", pairs.as_str()),
                    ]);
                    service.browser.logger().info("comparison.completed", &completed_fields);
                    Ok(ComparisonSession { request, entries, scope })
                }
                Err(error) => {
                    service.browser.logger().failure("comparison.failed", &error, &fields);
                    token.cancel();
                    request.connection.shutdown().await;
                    Err(error)
                }
            }
        });
        ComparisonOperation {
            operation: Operation { id, cancel, task },
            progress: receiver,
        }
    }
    async fn compare(
        &self,
        request: &LoadRequest,
        cancel: &CancellationToken,
        progress: &watch::Sender<Progress>,
        id: OperationId,
    ) -> Result<(Vec<ComparisonEntry>, ScopeSummary)> {
        if request.connection.state() != ConnectionState::Connected {
            return Err(Error::Invalid(
                "connection is closed; reconnect before comparing".into(),
            ));
        }
        let root = request.location.root.clone();
        let base = root
            .base()
            .to_str()
            .ok_or_else(|| Error::Invalid("project path is not UTF-8".into()))?;
        let mapper = Mapper::new(
            base,
            &request.connection.root,
            &request.location.config.mappings,
            &request.host.mappings,
        )?;
        let mappings = if request.host.mappings.is_empty() {
            &request.location.config.mappings
        } else {
            &request.host.mappings
        };
        let mut local = request.local.clone();
        let remote = request.remote.clone();
        if local.is_empty() && remote.is_empty() {
            local = if mappings.is_empty() {
                vec![PathBuf::from(".")]
            } else {
                mappings.iter().map(|m| PathBuf::from(&m.local)).collect()
            };
        }
        let mut scope = ScopeSummary::default();
        let mut skipped_ignored = BTreeSet::new();
        let mut errors = Vec::new();
        let mut pairs = BTreeMap::new();
        let mut explicit = BTreeSet::new();
        let mut local_dirs = BTreeSet::new();
        let mut remote_dirs = BTreeSet::new();
        if request.local.is_empty() && request.remote.is_empty() {
            for path in local.drain(..) {
                let path = root.base().join(path);
                remote_dirs.insert(mapper.local_to_remote(&path.to_string_lossy())?);
                local_dirs.insert(path);
            }
        }
        // Gather all direct files first, so exceptions do not depend on order.
        for path in local {
            let absolute = if path.is_absolute() {
                path
            } else {
                root.base().join(path)
            };
            let stat_root = root.clone();
            let stat_path = absolute.clone();
            let metadata = self
                .browser
                .blocking(cancel, move || stat_root.metadata(&stat_path))
                .await;
            let translated = mapper.local_to_remote(
                absolute
                    .to_str()
                    .ok_or_else(|| Error::Invalid("local selection is not UTF-8".into()))?,
            );
            let remote_path = match translated {
                Ok(p) => p,
                Err(error) => {
                    errors.push(self.failed_entry(
                        &request.connection,
                        id,
                        (absolute, String::new()),
                        error.into(),
                        "map_local",
                    ));
                    continue;
                }
            };
            match metadata {
                Ok(meta) if meta.is_dir() => {
                    local_dirs.insert(absolute);
                    remote_dirs.insert(remote_path);
                }
                Ok(meta) if meta.is_file() => {
                    explicit.insert(absolute.clone());
                    pairs.insert(absolute, remote_path);
                }
                Ok(_) => errors.push(self.failed_entry(
                    &request.connection,
                    id,
                    (absolute, remote_path),
                    Error::Invalid("local selection is not a regular file or directory".into()),
                    "stat_local",
                )),
                Err(error) => errors.push(self.failed_entry(
                    &request.connection,
                    id,
                    (absolute, remote_path),
                    error,
                    "stat_local",
                )),
            }
            progress.send_modify(|p| p.completed += 1);
        }
        for path in remote {
            if !request.connection.contains(&path) {
                return Err(Error::Invalid(
                    "remote selection is outside host root".into(),
                ));
            }
            let local_path = match mapper.remote_to_local(&path) {
                Ok(p) => PathBuf::from(p),
                Err(error) => {
                    errors.push(self.failed_entry(
                        &request.connection,
                        id,
                        (PathBuf::new(), path),
                        error.into(),
                        "map_remote",
                    ));
                    continue;
                }
            };
            match request.connection.client.stat(&path).await {
                Ok(meta) if meta.directory => {
                    local_dirs.insert(local_path);
                    remote_dirs.insert(path);
                }
                Ok(meta) if meta.regular => {
                    explicit.insert(local_path.clone());
                    pairs.insert(local_path, path);
                }
                Ok(_) => errors.push(self.failed_entry(
                    &request.connection,
                    id,
                    (local_path, path),
                    Error::Invalid("remote selection is not a regular file or directory".into()),
                    "stat_remote",
                )),
                Err(error) => errors.push(self.failed_entry(
                    &request.connection,
                    id,
                    (local_path, path),
                    error,
                    "stat_remote",
                )),
            }
            progress.send_modify(|p| p.completed += 1);
        }
        // Walk the union of corresponding directory scopes on both sides.
        // Classify each level before descent, then all file pairs in one batch.
        while let Some(path) = local_dirs.pop_first() {
            let relative = path
                .strip_prefix(root.base())
                .map_err(|_| Error::Invalid("selected directory is outside project".into()))?
                .to_path_buf();
            if hard_excluded(&relative, true) {
                scope.hard_excluded_skipped += 1;
                continue;
            }
            let classification = classify_ignored(
                cancel,
                root.base(),
                &[Entry {
                    path: relative,
                    directory: true,
                    symlink: false,
                    size: 0,
                }],
            )
            .await?;
            if classification[0] && !request.scope.include_ignored {
                if skipped_ignored.insert(path.clone()) {
                    scope.ignored_skipped += 1;
                }
                continue;
            }
            let walk_root = root.clone();
            let walk_path = path.clone();
            let listed = self
                .browser
                .blocking(cancel, move || walk_root.entries(&walk_path))
                .await;
            let entries = match listed {
                Ok(entries) => entries,
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    errors.push(
                        self.failed_entry(
                            &request.connection,
                            id,
                            (
                                path.clone(),
                                mapper
                                    .local_to_remote(&path.to_string_lossy())
                                    .unwrap_or_default(),
                            ),
                            error,
                            "walk_local",
                        ),
                    );
                    continue;
                }
            };
            let ignored = classify_ignored(cancel, root.base(), &entries).await?;
            for (entry, ignored) in entries.into_iter().zip(ignored) {
                let absolute = root.base().join(&entry.path);
                if entry.directory && ignored && !request.scope.include_ignored {
                    if skipped_ignored.insert(absolute) {
                        scope.ignored_skipped += 1;
                    }
                    continue;
                }
                if entry.directory {
                    local_dirs.insert(absolute);
                } else if !entry.symlink {
                    match mapper.local_to_remote(&absolute.to_string_lossy()) {
                        Ok(remote) => {
                            pairs.insert(absolute, remote);
                        }
                        Err(error) => errors.push(self.failed_entry(
                            &request.connection,
                            id,
                            (absolute, String::new()),
                            error.into(),
                            "map_local",
                        )),
                    }
                }
            }
            progress.send_modify(|p| p.completed += 1);
        }
        while let Some(path) = remote_dirs.pop_first() {
            if !request.connection.contains(&path) {
                return Err(Error::Invalid(
                    "mapped directory is outside host root".into(),
                ));
            }
            let local = PathBuf::from(mapper.remote_to_local(&path)?);
            let relative = local
                .strip_prefix(root.base())
                .map_err(|_| Error::Invalid("mapped directory is outside project".into()))?
                .to_path_buf();
            if hard_excluded(&relative, true)
                || hard_excluded(
                    std::path::Path::new(
                        path.strip_prefix(&request.connection.root)
                            .unwrap_or(&path)
                            .trim_start_matches('/'),
                    ),
                    true,
                )
            {
                scope.hard_excluded_skipped += 1;
                continue;
            }
            let classification = classify_ignored(
                cancel,
                root.base(),
                &[Entry {
                    path: relative,
                    directory: true,
                    symlink: false,
                    size: 0,
                }],
            )
            .await?;
            if classification[0] && !request.scope.include_ignored {
                if skipped_ignored.insert(local) {
                    scope.ignored_skipped += 1;
                }
                continue;
            }
            let entries = match request.connection.client.read_dir(&path).await {
                Ok(entries) => entries,
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    errors.push(self.failed_entry(
                        &request.connection,
                        id,
                        (
                            PathBuf::from(mapper.remote_to_local(&path).unwrap_or_default()),
                            path,
                        ),
                        error,
                        "walk_remote",
                    ));
                    continue;
                }
            };
            let mut mapped = Vec::new();
            for entry in entries {
                if !request.connection.contains(&entry.path) {
                    return Err(Error::Invalid(
                        "remote listing returned an out-of-scope path".into(),
                    ));
                }
                if let Ok(local) = mapper.remote_to_local(&entry.path) {
                    let local = PathBuf::from(local);
                    let relative = local
                        .strip_prefix(root.base())
                        .map_err(|_| Error::Invalid("mapped file is outside project".into()))?
                        .to_path_buf();
                    if hard_excluded(&relative, entry.directory)
                        || hard_excluded(
                            std::path::Path::new(
                                entry
                                    .path
                                    .strip_prefix(&request.connection.root)
                                    .unwrap_or(&entry.path)
                                    .trim_start_matches('/'),
                            ),
                            entry.directory,
                        )
                    {
                        scope.hard_excluded_skipped += 1;
                        continue;
                    }
                    if !entry.directory && !entry.regular {
                        continue;
                    }
                    mapped.push((
                        entry,
                        local,
                        Entry {
                            path: relative,
                            directory: false,
                            symlink: false,
                            size: 0,
                        },
                    ));
                }
            }
            let classify: Vec<_> = mapped
                .iter()
                .map(|(entry, _, local)| Entry {
                    directory: entry.directory,
                    ..local.clone()
                })
                .collect();
            let ignored = classify_ignored(cancel, root.base(), &classify).await?;
            for ((entry, local, _), ignored) in mapped.into_iter().zip(ignored) {
                if entry.directory {
                    if ignored && !request.scope.include_ignored {
                        if skipped_ignored.insert(local) {
                            scope.ignored_skipped += 1;
                        }
                    } else {
                        remote_dirs.insert(entry.path);
                    }
                } else {
                    pairs.insert(local, entry.path);
                }
            }
            progress.send_modify(|p| p.completed += 1);
        }
        let classify: Vec<_> = pairs
            .keys()
            .map(|path| Entry {
                path: path.strip_prefix(root.base()).unwrap_or(path).into(),
                directory: false,
                symlink: false,
                size: 0,
            })
            .collect();
        let ignored = classify_ignored(cancel, root.base(), &classify).await?;
        let mut jobs = Vec::new();
        for (((local, remote), entry), ignored) in pairs.into_iter().zip(classify).zip(ignored) {
            if hard_excluded(&entry.path, false)
                || hard_excluded(
                    std::path::Path::new(
                        remote
                            .strip_prefix(&request.connection.root)
                            .unwrap_or(&remote)
                            .trim_start_matches('/'),
                    ),
                    false,
                )
            {
                scope.hard_excluded_skipped += 1;
                continue;
            }
            let direct = explicit.contains(&local);
            if ignored && !request.scope.include_ignored && !direct {
                if skipped_ignored.insert(local) {
                    scope.ignored_skipped += 1;
                }
                continue;
            }
            if ignored && direct && !request.scope.include_ignored {
                scope.explicit_ignored += 1;
            }
            if entry
                .path
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
                || remote
                    .strip_prefix(&request.connection.root)
                    .unwrap_or(&remote)
                    .trim_start_matches('/')
                    .split('/')
                    .any(|part| part.starts_with('.') && part != "." && part != "..")
            {
                scope.hidden += 1;
            }
            jobs.push((local, remote));
        }
        scope.pairs = jobs.len();
        progress.send_modify(|p| {
            p.phase = "Comparing";
            p.completed = 0;
            p.total = jobs.len();
        });
        let mut pending = jobs.into_iter();
        let mut workers = JoinSet::new();
        let mut entries = errors;
        let limit = if matches!(request.host.protocol.as_str(), "ftp" | "ftps") {
            4
        } else {
            8
        };
        loop {
            while workers.len() < limit {
                let Some((local, remote)) = pending.next() else {
                    break;
                };
                let service = self.clone();
                let root = root.clone();
                let connection = request.connection.clone();
                let token = cancel.clone();
                let progress = progress.clone();
                workers.spawn(self.browser.runtime.tasks.track_future(async move {
                    let result = service
                        .pair(root, &connection, &local, &remote, &token, &progress)
                        .await;
                    match result {
                        Ok(result) => ComparisonEntry {
                            local,
                            remote,
                            result: Some(Arc::new(result)),
                            error: None,
                        },
                        Err(error) => {
                            service.failed_entry(&connection, id, (local, remote), error, "pair")
                        }
                    }
                }));
            }
            let Some(result) = workers.join_next().await else {
                break;
            };
            let entry =
                result.map_err(|e| Error::Invalid(format!("comparison worker failed: {e}")))?;
            if entry.error.is_some() || entry.result.as_ref().is_some_and(|r| r.differs()) {
                entries.push(entry);
            }
            progress.send_modify(|p| p.completed += 1);
            if request.connection.state() != ConnectionState::Connected {
                return Err(Error::Invalid(
                    "connection lost during comparison; reconnect before comparing".into(),
                ));
            }
        }
        entries.sort_by(|a, b| a.local.cmp(&b.local).then(a.remote.cmp(&b.remote)));
        Ok((entries, scope))
    }
    fn failed_entry(
        &self,
        connection: &RemoteSession,
        id: OperationId,
        paths: (PathBuf, String),
        error: Error,
        stage: &'static str,
    ) -> ComparisonEntry {
        let (local, remote) = paths;
        self.browser.logger().failure(
            "comparison.entry_failed",
            &error,
            &[
                ("host", connection.host_name.as_str()),
                ("local_path", &local.to_string_lossy()),
                ("remote_path", remote.as_str()),
                ("project", &id.project.to_string()),
                ("operation", &id.operation.to_string()),
                ("stage", stage),
            ],
        );
        let message = match stage {
            "walk_local" => format!("walk local: {error}"),
            "walk_remote" => format!("walk remote: {error}"),
            _ => error.to_string(),
        };
        ComparisonEntry {
            local,
            remote,
            result: None,
            error: Some(message),
        }
    }
    async fn pair(
        &self,
        root: Arc<ProjectRoot>,
        connection: &RemoteSession,
        local: &std::path::Path,
        remote: &str,
        cancel: &CancellationToken,
        progress: &watch::Sender<Progress>,
    ) -> Result<DiffResult> {
        let stat_root = root.clone();
        let stat_path = local.to_path_buf();
        let local_meta = match self
            .browser
            .blocking(cancel, move || stat_root.metadata(&stat_path))
            .await
        {
            Ok(meta) if meta.is_file() => Some(FileMetadata {
                size: meta.len(),
                modified: meta.modified().ok().map(|t| t.into_std()),
            }),
            Ok(_) => {
                return Err(Error::Invalid(
                    "local counterpart is not a regular file".into(),
                ));
            }
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let remote_meta = match connection.client.stat(remote).await {
            Ok(meta) if meta.regular => Some(FileMetadata {
                size: meta.size,
                modified: meta.modified,
            }),
            Ok(_) => {
                return Err(Error::Invalid(
                    "remote counterpart is not a regular file".into(),
                ));
            }
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let mut result = DiffResult {
            local: local_meta,
            remote: remote_meta,
            ..Default::default()
        };
        if let (Some(local), Some(remote)) = (&result.local, &result.remote) {
            if local.size == remote.size
                && local.modified.is_some()
                && local.modified == remote.modified
            {
                return Ok(result);
            }
        } else if result.local.is_none() && result.remote.is_none() {
            return Ok(result);
        }
        let large = result.local.as_ref().is_some_and(|m| m.size > TEXT_LIMIT)
            || result.remote.as_ref().is_some_and(|m| m.size > TEXT_LIMIT);
        if large {
            result.binary = true;
            if result.local.as_ref().map(|m| m.size) != result.remote.as_ref().map(|m| m.size) {
                result.content_diff = true;
                return Ok(result);
            }
        }
        let path = local.to_path_buf();
        let token = cancel.clone();
        let activity = progress.clone();
        let local_data = if result.local.is_some() {
            self.browser
                .blocking(cancel, move || {
                    let mut file = root.read_regular(&path)?;
                    let mut data = Vec::new();
                    let mut digest = Sha256::new();
                    let mut size = 0;
                    let mut buffer = [0u8; 64 * 1024];
                    loop {
                        if token.is_cancelled() {
                            return Err(Error::Invalid("comparison cancelled".into()));
                        }
                        let n = file.read(&mut buffer)?;
                        if n == 0 {
                            break;
                        }
                        size += n as u64;
                        activity.send_modify(|p| p.bytes += n as u64);
                        if large {
                            digest.update(&buffer[..n]);
                        } else {
                            if size > TEXT_LIMIT {
                                return Err(Error::Invalid(
                                    "local file grew beyond the text limit; compare again".into(),
                                ));
                            }
                            data.extend_from_slice(&buffer[..n]);
                        }
                    }
                    nix::unistd::close(file.into_std())
                        .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;
                    Ok((data, digest.finalize().to_vec(), size))
                })
                .await?
        } else {
            (Vec::new(), Vec::new(), 0)
        };
        let mut remote_data = Vec::new();
        let mut remote_digest = Sha256::new();
        let mut size = 0;
        if result.remote.is_some() {
            let mut file = connection.client.open(remote).await?;
            let read: Result<()> = async {
                let mut buffer = [0u8; 64 * 1024];
                loop {
                    let n = file.read(&mut buffer).await?;
                    if n == 0 {
                        break;
                    }
                    size += n as u64;
                    progress.send_modify(|p| p.bytes += n as u64);
                    if large {
                        remote_digest.update(&buffer[..n]);
                    } else {
                        if size > TEXT_LIMIT {
                            return Err(Error::Invalid(
                                "remote file grew beyond the text limit; compare again".into(),
                            ));
                        }
                        remote_data.extend_from_slice(&buffer[..n]);
                    }
                }
                Ok(())
            }
            .await;
            let closed = file.close().await;
            match (read, closed) {
                (Err(a), Err(b)) => {
                    return Err(Error::Invalid(format!("{a}; close remote file: {b}")));
                }
                (Err(e), _) | (_, Err(e)) => return Err(e),
                _ => {}
            }
        }
        if large {
            result.content_diff =
                local_data.2 != size || local_data.1 != remote_digest.finalize().as_slice();
        } else {
            result = self
                .browser
                .blocking(cancel, move || {
                    result.text(&local_data.0, &remote_data);
                    Ok(result)
                })
                .await?;
        }
        Ok(result)
    }
}
