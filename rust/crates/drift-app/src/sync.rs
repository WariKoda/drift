//! Serial sync of a validated comparison snapshot. Nothing is retried.
use crate::{
    browser::{BrowserService, Operation, OperationId, hard_excluded},
    comparison::{ComparisonSession, LoadRequest, Progress},
};
use drift_core::{
    diff::Decision,
    error::{Error, Result},
    pathmap::Mapper,
    remote::{ConnectionState, RemoteRead},
};
use std::{
    path::PathBuf,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::watch,
};
use tokio_util::{io::SyncIoBridge, sync::CancellationToken};

#[derive(Clone, Debug)]
pub struct SyncItem {
    pub local: PathBuf,
    pub remote: String,
    pub decision: Decision,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemOutcome {
    Pending,
    Skipped,
    Completed,
    Failed(String),
    Unknown(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    ConnectionLost(String),
    TimedOut,
}
#[derive(Clone, Debug)]
pub struct SyncResult {
    pub items: Vec<SyncItem>,
    pub outcomes: Vec<ItemOutcome>,
    pub stopped: Option<StopReason>,
}
pub struct SyncOperation {
    pub operation: Operation<SyncResult>,
    pub progress: watch::Receiver<Progress>,
}
#[derive(Clone)]
pub struct SyncService {
    browser: BrowserService,
}
impl SyncService {
    pub fn new(browser: BrowserService) -> Self {
        Self { browser }
    }
    pub fn run(
        &self,
        session: &ComparisonSession,
        decisions: &[Decision],
        id: OperationId,
    ) -> Result<SyncOperation> {
        if session.request.connection.state() != ConnectionState::Connected {
            return Err(Error::Invalid(
                "reconnect and compare again before syncing".into(),
            ));
        }
        if decisions.len() != session.entries.len() {
            return Err(Error::Invalid(
                "sync decisions do not match the comparison".into(),
            ));
        }
        let request = session.request.clone();
        let root = request.location.root.base();
        let mapper = Mapper::new(
            root.to_str()
                .ok_or_else(|| Error::Invalid("project path is not UTF-8".into()))?,
            &request.connection.root,
            &request.location.config.mappings,
            &request.host.mappings,
        )?;
        let mut items = Vec::new();
        for (entry, &decision) in session.entries.iter().zip(decisions) {
            if decision != Decision::Skip {
                let result = entry
                    .result
                    .as_ref()
                    .filter(|_| entry.error.is_none())
                    .ok_or_else(|| {
                        Error::Invalid("cannot sync a failed comparison entry".into())
                    })?;
                let valid = match decision {
                    Decision::Upload => result.local.is_some(),
                    Decision::Download => result.remote.is_some(),
                    Decision::DeleteLocal => result.local.is_some() && result.remote.is_none(),
                    Decision::DeleteRemote => result.remote.is_some() && result.local.is_none(),
                    Decision::Skip => true,
                };
                let relative = entry
                    .local
                    .strip_prefix(root)
                    .map_err(|_| Error::Invalid("sync path is outside project".into()))?;
                let remote_relative = std::path::Path::new(
                    entry
                        .remote
                        .strip_prefix(&request.connection.root)
                        .unwrap_or(&entry.remote)
                        .trim_start_matches('/'),
                );
                if !valid
                    || hard_excluded(relative, false)
                    || hard_excluded(remote_relative, false)
                    || !request.connection.contains(&entry.remote)
                    || mapper.local_to_remote(
                        entry
                            .local
                            .to_str()
                            .ok_or_else(|| Error::Invalid("sync path is not UTF-8".into()))?,
                    )? != entry.remote
                {
                    return Err(Error::Invalid(
                        "sync action or path is outside the comparison scope".into(),
                    ));
                }
            }
            items.push(SyncItem {
                local: entry.local.clone(),
                remote: entry.remote.clone(),
                decision,
            });
        }
        let (progress, receiver) = watch::channel(Progress {
            phase: "Syncing",
            completed: 0,
            total: items.len(),
            bytes: 0,
        });
        let service = self.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = self
            .browser
            .runtime
            .spawn(async move { Ok(service.execute(request, items, token, progress).await) });
        Ok(SyncOperation {
            operation: Operation { id, cancel, task },
            progress: receiver,
        })
    }
    async fn execute(
        &self,
        request: LoadRequest,
        items: Vec<SyncItem>,
        cancel: CancellationToken,
        progress: watch::Sender<Progress>,
    ) -> SyncResult {
        let mut report = SyncResult {
            outcomes: vec![ItemOutcome::Pending; items.len()],
            items,
            stopped: None,
        };
        let mut state = request.connection.client.state();
        for i in 0..report.items.len() {
            if cancel.is_cancelled() {
                report.stopped = Some(StopReason::Cancelled);
                break;
            }
            if request.connection.state() != ConnectionState::Connected {
                report.stopped = Some(StopReason::ConnectionLost(format!(
                    "{:?}",
                    request.connection.state()
                )));
                break;
            }
            let item = &report.items[i];
            if item.decision == Decision::Skip {
                report.outcomes[i] = ItemOutcome::Skipped;
                progress.send_modify(|p| p.completed += 1);
                continue;
            }
            let mut activity = progress.subscribe();
            let idle = async {
                while let Ok(Ok(())) =
                    tokio::time::timeout(Duration::from_secs(60), activity.changed()).await
                {
                }
            };
            let disconnected = async {
                while *state.borrow_and_update() == ConnectionState::Connected {
                    if state.changed().await.is_err() {
                        break;
                    }
                }
            };
            let mut operation = Box::pin(self.item(&request, item, &cancel, &progress));
            let result = tokio::select! {
                // Preserve a completion acknowledged at the cancellation boundary.
                biased;
                result = &mut operation => result,
                _ = cancel.cancelled() => {
                    report.stopped = Some(StopReason::Cancelled);
                    request.connection.shutdown().await;
                    operation.await
                },
                _ = disconnected => {
                    report.stopped = Some(StopReason::ConnectionLost(format!("{:?}", request.connection.state())));
                    cancel.cancel();
                    request.connection.shutdown().await;
                    operation.await
                },
                _ = idle => {
                    report.stopped = Some(StopReason::TimedOut);
                    cancel.cancel();
                    request.connection.shutdown().await;
                    operation.await
                },
            };
            if report.stopped.is_none() {
                if matches!(result, Err(Error::Connection(_)))
                    || request.connection.state() != ConnectionState::Connected
                {
                    let reason = if let Err(Error::Connection(error)) = &result {
                        error.clone()
                    } else {
                        format!("{:?}", request.connection.state())
                    };
                    report.stopped = Some(StopReason::ConnectionLost(reason));
                    request.connection.shutdown().await;
                } else if cancel.is_cancelled() {
                    report.stopped = Some(StopReason::Cancelled);
                }
            }
            report.outcomes[i] = match result {
                Ok(()) => ItemOutcome::Completed,
                Err(e)
                    if report.stopped.is_some()
                        && matches!(item.decision, Decision::Upload | Decision::DeleteRemote) =>
                {
                    ItemOutcome::Unknown(format!("{e}; compare again before syncing"))
                }
                Err(e) => ItemOutcome::Failed(e.to_string()),
            };
            progress.send_modify(|p| p.completed += 1);
            if report.stopped.is_some() {
                break;
            }
        }
        if report.stopped.is_some() {
            request.connection.shutdown().await;
        }
        report
    }
    async fn item(
        &self,
        request: &LoadRequest,
        item: &SyncItem,
        cancel: &CancellationToken,
        progress: &watch::Sender<Progress>,
    ) -> Result<()> {
        let root = request.location.root.clone();
        let path = item.local.clone();
        match item.decision {
            Decision::Upload => {
                let file = self
                    .browser
                    .blocking_mutation(cancel, move || root.read_regular(&path))
                    .await?;
                let source = LocalSource(tokio::fs::File::from_std(file.into_std()));
                request
                    .connection
                    .client
                    .upload(
                        &item.remote,
                        Box::new(TransferRead {
                            inner: Box::new(source),
                            cancel: cancel.clone(),
                            progress: progress.clone(),
                        }),
                    )
                    .await
            }
            Decision::Download => {
                let source = request.connection.client.open(&item.remote).await?;
                let source = TransferRead {
                    inner: source,
                    cancel: cancel.clone(),
                    progress: progress.clone(),
                };
                let bridge = SyncIoBridge::new(source);
                let runtime = self.browser.runtime.handle().clone();
                // Already opened remote streams must be closed even if cancellation
                // arrives before this worker is admitted. Cancellation is checked
                // by TransferRead/close, so use an uncancelled admission token.
                self.browser
                    .blocking_mutation(&CancellationToken::new(), move || {
                        root.write_atomic(&path, bridge, |source| {
                            runtime.block_on(Box::new(source.into_inner()).close())
                        })
                    })
                    .await
            }
            Decision::DeleteLocal => {
                self.browser
                    .blocking_mutation(cancel, move || root.remove(&path))
                    .await
            }
            Decision::DeleteRemote => request.connection.client.delete(&item.remote).await,
            Decision::Skip => Ok(()),
        }
    }
}

struct LocalSource(tokio::fs::File);
impl AsyncRead for LocalSource {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}
#[async_trait::async_trait]
impl RemoteRead for LocalSource {
    async fn close(self: Box<Self>) -> Result<()> {
        nix::unistd::close(self.0.into_std().await)
            .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;
        Ok(())
    }
}
struct TransferRead {
    inner: Box<dyn RemoteRead>,
    cancel: CancellationToken,
    progress: watch::Sender<Progress>,
}
impl AsyncRead for TransferRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // std::io::copy retries Interrupted, so cancellation must be terminal.
        if self.cancel.is_cancelled() {
            return Poll::Ready(Err(std::io::Error::other("sync cancelled")));
        }
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if matches!(result, Poll::Ready(Ok(()))) {
            let bytes = buf.filled().len() - before;
            if bytes > 0 {
                self.progress.send_modify(|p| p.bytes += bytes as u64);
            }
        }
        result
    }
}
#[async_trait::async_trait]
impl RemoteRead for TransferRead {
    async fn close(self: Box<Self>) -> Result<()> {
        self.inner.close().await?;
        if self.cancel.is_cancelled() {
            return Err(Error::Invalid("sync cancelled before commit".into()));
        }
        Ok(())
    }
}
