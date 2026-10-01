//! Session ownership and cancellable remote work, independent of the GUI.
use crate::browser::{BrowserService, Operation, OperationId};
use drift_core::{
    config::Host,
    error::{Error, Result},
    remote::{self, ConnectOptions, ConnectionState, RemoteClient, RemoteEntry},
};
use std::{future::Future, sync::Arc};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct RemoteSession {
    pub id: OperationId,
    pub host_name: String,
    pub root: String,
    client: Arc<dyn RemoteClient>,
    preview_lock: Arc<tokio::sync::Mutex<()>>,
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
#[derive(Clone)]
pub struct RemoteService {
    browser: BrowserService,
    options: Option<ConnectOptions>,
}
impl RemoteService {
    pub fn new(browser: BrowserService) -> Self {
        Self {
            browser,
            options: None,
        }
    }
    pub fn with_options(browser: BrowserService, options: ConnectOptions) -> Self {
        Self {
            browser,
            options: Some(options),
        }
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
        let options = self.options.clone();
        self.run(id, move |cancel| async move {
            let options = match options { Some(options) => options, None => ConnectOptions::from_environment()? };
            let client = remote::connect(host.clone(), options, cancel.clone()).await?;
            let root = if host.root_path.is_empty() { "." } else { &host.root_path };
            let path = tokio::select! { _ = cancel.cancelled() => return Err(Error::Invalid("connection cancelled".into())), path = client.canonicalize(root) => path? };
            let session = RemoteSession { id, host_name: host.name, root: path.clone(), client, preview_lock: Arc::new(tokio::sync::Mutex::new(())) };
            Self::directory(session, path, cancel).await
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
        self.run(id, move |cancel| async move {
            let mut state = session.client.state();
            loop {
                let current = state.borrow_and_update().clone();
                if current != ConnectionState::Connected { return Ok(current); }
                tokio::select! { _ = cancel.cancelled() => return Ok(ConnectionState::Closed), result = state.changed() => if result.is_err() { return Ok(ConnectionState::Failed("connection monitor stopped".into())); } }
            }
        })
    }
    pub fn close(&self, session: RemoteSession) {
        self.browser.runtime.spawn(async move {
            session.shutdown().await;
        });
    }
}
