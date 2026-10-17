//! Session ownership and cancellable remote work, independent of the GUI.
use crate::browser::{
    BrowserService, Location, Operation, OperationId, classify_ignored, hard_excluded,
};
use drift_core::{
    config::Host,
    error::{Error, Result},
    local::Entry,
    pathmap::Mapper,
    remote::{self, ConnectOptions, ConnectionState, RemoteClient, RemoteEntry},
    store::Store,
    tlstrust::{Challenge, Endpoint, Manager, TrustSnapshot},
};
use std::{
    collections::BTreeSet,
    future::Future,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct RemoteSession {
    pub id: OperationId,
    pub host_name: String,
    pub root: String,
    pub(crate) client: Arc<dyn RemoteClient>,
    preview_lock: Arc<tokio::sync::Mutex<()>>,
    monitor_failure_logged: Arc<AtomicBool>,
}
impl RemoteSession {
    pub async fn shutdown(&self) {
        self.client.shutdown().await;
    }
    pub fn state(&self) -> ConnectionState {
        self.client.state().borrow().clone()
    }
    pub fn contains(&self, path: &str) -> bool {
        if !path.starts_with('/')
            || path.contains('\0')
            || std::path::Path::new(path).components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return false;
        }
        path == self.root
            || (path.starts_with(&self.root)
                && (self.root == "/" || path.as_bytes().get(self.root.len()) == Some(&b'/')))
    }
}
pub struct RemoteDirectory {
    pub session: RemoteSession,
    pub path: String,
    pub entries: Vec<RemoteEntry>,
}
/// Raw remote paths, classified independently of the browser's visibility toggles.
#[derive(Default)]
pub struct RemoteVisibility {
    pub ignored: BTreeSet<String>,
    pub excluded: BTreeSet<String>,
}
#[derive(Clone)]
pub struct RemoteService {
    browser: BrowserService,
    options: Option<ConnectOptions>,
    trust: Option<Arc<Manager>>,
}
impl RemoteService {
    /// Test a fresh resolved draft using a separate, always closed connection.
    /// The active browser session and the stored host records remain untouched.
    pub fn test_host(
        &self,
        store: Store,
        slug: Option<String>,
        draft: Host,
        required: Option<Challenge>,
        id: OperationId,
    ) -> Operation<String> {
        let service = self.clone();
        self.run(id, move |cancel| async move {
            let host_name = draft.name.clone();
            let project = id.project.to_string();
            let operation_id = id.operation.to_string();
            let fields = [
                ("host", host_name.as_str()),
                ("project", project.as_str()),
                ("operation", operation_id.as_str()),
            ];
            let host = service
                .browser
                .blocking(&cancel, move || store.preview_host(slug.as_deref(), draft))
                .await
                .inspect_err(|error| {
                    service
                        .browser
                        .logger()
                        .failure("host_test.resolve_failed", error, &fields);
                })?;
            let operation = service.connect_required(host, required, id);
            let mut task = operation.task;
            let result = tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    operation.cancel.cancel();
                    task.await
                },
                result = &mut task => result,
            }
            .map_err(|error| Error::Connection(format!("host test failed: {error}")))
            .inspect_err(|error| {
                service
                    .browser
                    .logger()
                    .failure("host_test.failed", error, &fields);
            })?;
            match result {
                Ok(directory) => {
                    directory.session.shutdown().await;
                    if cancel.is_cancelled() {
                        let error = Error::Invalid("host test cancelled".into());
                        service
                            .browser
                            .logger()
                            .failure("host_test.failed", &error, &fields);
                        return Err(error);
                    }
                    service
                        .browser
                        .logger()
                        .info("host_test.completed", &fields);
                    Ok(directory.path)
                }
                // Connection failures are logged by connect_required.
                Err(error) => Err(error),
            }
        })
    }
    pub fn inspect_host_trust(
        &self,
        store: Store,
        slug: Option<String>,
        draft: Host,
        id: OperationId,
    ) -> Operation<TrustSnapshot> {
        let browser = self.browser.clone();
        let manager = self.trust.clone();
        self.run(id, move |cancel| async move {
            browser
                .blocking(&cancel, move || {
                    let host = store.preview_host(slug.as_deref(), draft)?;
                    if host.protocol != "ftps" {
                        return Err(Error::Invalid(
                            "certificate trust applies only to FTPS hosts".into(),
                        ));
                    }
                    manager
                        .ok_or_else(|| Error::Invalid("FTPS trust manager unavailable".into()))?
                        .inspect(Endpoint::new(&host.hostname, host.port)?)
                })
                .await
        })
    }
    pub fn reset_host_trust(&self, snapshot: TrustSnapshot, id: OperationId) -> Operation<()> {
        let browser = self.browser.clone();
        let manager = self.trust.clone();
        self.run(id, move |cancel| async move {
            browser
                .blocking_mutation(&cancel, move || {
                    manager
                        .ok_or_else(|| Error::Invalid("FTPS trust manager unavailable".into()))?
                        .reset(&snapshot)
                })
                .await
        })
    }
    pub fn new(browser: BrowserService) -> Self {
        Self {
            browser,
            options: None,
            trust: None,
        }
    }
    pub fn with_options(browser: BrowserService, options: ConnectOptions) -> Self {
        Self {
            browser,
            options: Some(options),
            trust: None,
        }
    }
    pub fn with_trust(mut self, manager: Arc<Manager>) -> Self {
        self.trust = Some(manager);
        self
    }
    pub fn trust_manager(&self) -> Option<Arc<Manager>> {
        self.trust.clone()
    }
    pub fn trust_certificate(
        &self,
        challenge: Challenge,
        permanent: bool,
        id: OperationId,
    ) -> Operation<()> {
        let manager = self.trust.clone();
        let browser = self.browser.clone();
        self.run(id, move |cancel| async move {
            let manager =
                manager.ok_or_else(|| Error::Invalid("FTPS trust manager unavailable".into()))?;
            browser
                .blocking_mutation(&cancel, move || manager.grant(&challenge, permanent))
                .await
        })
    }
    fn run<T: Send + 'static, F: Future<Output = Result<T>> + Send + 'static>(
        &self,
        id: OperationId,
        action: impl FnOnce(CancellationToken) -> F + Send + 'static,
    ) -> Operation<T> {
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self
            .browser
            .runtime
            .spawn(async move { action(token).await });
        Operation { id, cancel, task }
    }
    pub fn connect(&self, host: Host, id: OperationId) -> Operation<RemoteDirectory> {
        self.connect_required(host, None, id)
    }
    pub fn connect_required(
        &self,
        host: Host,
        required: Option<Challenge>,
        id: OperationId,
    ) -> Operation<RemoteDirectory> {
        let options = self.options.clone();
        let manager = self.trust.clone();
        let browser = self.browser.clone();
        self.run(id, move |cancel| async move {
            let project = id.project.to_string();
            let operation = id.operation.to_string();
            let port = host.port.to_string();
            let fields = [
                ("host", host.name.as_str()),
                ("endpoint", host.hostname.as_str()),
                ("port", port.as_str()),
                ("protocol", host.protocol.as_str()),
                ("remote_path", host.root_path.as_str()),
                ("project", project.as_str()),
                ("operation", operation.as_str()),
            ];
            browser.logger().info("connection.start", &fields);
            let result = async {
                if required.is_some() && host.protocol != "ftps" {
                    return Err(Error::Invalid("FTPS certificate retry requires an FTPS host".into()));
                }
                let mut options = match options { Some(options) => options, None => ConnectOptions::from_environment()? };
                if host.protocol=="ftps" {
                    if let Some(manager)=manager { options.tls=Some(browser.blocking(&cancel, move || manager.policy()).await?); }
                    if let Some(challenge)=required { options.tls=Some(options.tls.take().ok_or_else(||Error::Invalid("FTPS certificate policy unavailable".into()))?.require(challenge)); }
                }
                let client = remote::connect(host.clone(), options, cancel.clone()).await?;
                let root = if host.root_path.is_empty() { "." } else { &host.root_path };
                let path = tokio::select! { _ = cancel.cancelled() => Err(Error::Invalid("connection cancelled".into())), path = client.canonicalize(root) => path };
                let path = match path { Ok(path) => path, Err(error) => { client.shutdown().await; return Err(error); } };
                let session = RemoteSession { id, host_name: host.name.clone(), root: path.clone(), client, preview_lock: Arc::new(tokio::sync::Mutex::new(())), monitor_failure_logged: Arc::new(AtomicBool::new(false)) };
                match Self::directory(session.clone(), path, cancel).await {
                    Ok(directory) => Ok(directory),
                    Err(error) => { session.shutdown().await; Err(error) },
                }
            }.await;
            match &result {
                Ok(_) => browser.logger().info("connection.connected", &fields),
                Err(error) => browser.logger().failure("connection.failed", error, &fields),
            }
            result
        })
    }
    async fn directory(
        session: RemoteSession,
        path: String,
        cancel: CancellationToken,
    ) -> Result<RemoteDirectory> {
        if !session.contains(&path) {
            return Err(Error::Invalid("remote path is outside host root".into()));
        }
        let entries = tokio::select! { _ = cancel.cancelled() => { session.shutdown().await; return Err(Error::Invalid("remote listing cancelled; connection closed".into())); }, entries = session.client.read_dir(&path) => entries? };
        let entries = entries
            .into_iter()
            .filter(|entry| {
                !crate::browser::hard_excluded(std::path::Path::new(&entry.name), entry.directory)
            })
            .collect();
        Ok(RemoteDirectory {
            session,
            path,
            entries,
        })
    }
    pub fn list(
        &self,
        session: RemoteSession,
        path: String,
        id: OperationId,
    ) -> Operation<RemoteDirectory> {
        self.run(id, move |cancel| Self::directory(session, path, cancel))
    }
    /// Classify a listing locally; cancellation does not close its connection.
    pub fn classify_entries(
        &self,
        location: Location,
        host: Host,
        session: RemoteSession,
        entries: Vec<RemoteEntry>,
        id: OperationId,
    ) -> Operation<RemoteVisibility> {
        let browser = self.browser.clone();
        self.run(id, move |cancel| async move {
            let result = async {
                if cancel.is_cancelled() {
                    return Err(Error::Invalid("operation cancelled".into()));
                }
                let base = location.root.base();
                let mapper = Mapper::new(
                    base.to_str()
                        .ok_or_else(|| Error::Invalid("project path is not UTF-8".into()))?,
                    &session.root,
                    &location.config.mappings,
                    &host.mappings,
                )?;
                let has_mappings =
                    !host.mappings.is_empty() || !location.config.mappings.is_empty();
                let mut visibility = RemoteVisibility::default();
                let mut paths = Vec::new();
                let mut local_entries = Vec::new();
                for entry in entries {
                    if cancel.is_cancelled() {
                        return Err(Error::Invalid("operation cancelled".into()));
                    }
                    if !session.contains(&entry.path)
                        || Path::new(&entry.path)
                            .strip_prefix(&session.root)
                            .map_or(true, |relative| hard_excluded(relative, entry.directory))
                    {
                        visibility.excluded.insert(entry.path);
                        continue;
                    }
                    let local = match mapper.remote_to_local(&entry.path) {
                        Ok(local) => local,
                        // A validated mapper can only reject in-scope paths here
                        // because no mapping covers them. Keep them browsable,
                        // but never query Git with their untranslated names.
                        Err(_) if has_mappings => continue,
                        Err(error) => return Err(error.into()),
                    };
                    let relative = Path::new(&local)
                        .strip_prefix(base)
                        .map_err(|_| Error::Invalid("mapped path is outside project".into()))?;
                    if hard_excluded(relative, entry.directory) {
                        visibility.excluded.insert(entry.path);
                        continue;
                    }
                    local_entries.push(Entry {
                        path: relative.into(),
                        directory: entry.directory,
                        symlink: entry.symlink,
                        size: entry.size,
                    });
                    paths.push(entry.path);
                }
                if !local_entries.is_empty() {
                    let ignored = classify_ignored(&cancel, base, &local_entries).await?;
                    visibility.ignored.extend(
                        paths
                            .into_iter()
                            .zip(ignored)
                            .filter_map(|(path, ignored)| ignored.then_some(path)),
                    );
                }
                if cancel.is_cancelled() {
                    return Err(Error::Invalid("operation cancelled".into()));
                }
                Ok(visibility)
            }
            .await;
            result.inspect_err(|error| {
                browser.logger().failure(
                    "remote.classification_failed",
                    error,
                    &[
                        ("host", host.name.as_str()),
                        ("remote_path", session.root.as_str()),
                        ("project", &id.project.to_string()),
                        ("operation", &id.operation.to_string()),
                    ],
                );
            })
        })
    }
    pub fn preview(
        &self,
        session: RemoteSession,
        path: String,
        id: OperationId,
    ) -> Operation<String> {
        self.run(id, move |cancel| async move {
            if !session.contains(&path) { return Err(Error::Invalid("remote preview is outside host root".into())); }
            // A changed selection abandons the result, but lets the current
            // bounded read close its file handle before the next preview starts.
            // Explicit Cancel/Disconnect closes the session and interrupts I/O.
            let _permit = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(Error::Invalid("remote preview cancelled".into())),
                permit = session.preview_lock.lock() => permit,
            };
            let bytes = match tokio::time::timeout(std::time::Duration::from_secs(15), session.client.read_limited(&path, 1024 * 1024)).await {
                Ok(bytes) => bytes?,
                Err(_) => { session.shutdown().await; return Err(Error::Invalid("remote preview timed out; connection closed".into())); }
            };
            if cancel.is_cancelled() { return Err(Error::Invalid("remote preview cancelled".into())); }
            if bytes.contains(&0) { return Err(Error::Invalid("remote preview is binary".into())); }
            String::from_utf8(bytes).map_err(|_| Error::Invalid("remote preview is not UTF-8".into()))
        })
    }
    pub fn observe(&self, session: RemoteSession, id: OperationId) -> Operation<ConnectionState> {
        let logger = self.browser.logger().clone();
        self.run(id, move |cancel| async move {
            let mut state = session.client.state();
            let terminal = loop {
                let current = state.borrow_and_update().clone();
                if current != ConnectionState::Connected { break current; }
                tokio::select! { _ = cancel.cancelled() => break ConnectionState::Closed, result = state.changed() => if result.is_err() { break ConnectionState::Failed("connection monitor stopped".into()); } }
            };
            let category = match &terminal {
                ConnectionState::Failed(_) => Some("connection"),
                ConnectionState::Certificate(_) => Some("certificate"),
                ConnectionState::Connected | ConnectionState::Closed => None,
            };
            if let Some(category) = category
                && !session.monitor_failure_logged.swap(true, Ordering::Relaxed)
            {
                logger.error("connection.monitor_failed", &[
                    ("host", session.host_name.as_str()),
                    ("remote_path", session.root.as_str()),
                    ("project", &id.project.to_string()),
                    ("operation", &id.operation.to_string()),
                    ("connection_operation", &session.id.operation.to_string()),
                    ("category", category),
                ]);
            }
            Ok(terminal)
        })
    }
    pub fn close(&self, session: RemoteSession) {
        self.browser.runtime.spawn(async move {
            session.shutdown().await;
        });
    }
}
