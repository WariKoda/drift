#[path = "../../drift-core/tests/support/mod.rs"]
mod support;

use drift_app::{
    browser::{BrowserService, Location, OperationId},
    comparison::{ComparisonService, ComparisonSession, LoadRequest, ScopeOptions},
    logging::{Logger, Options},
    remote::RemoteService,
    sync::{ItemOutcome, StopReason, SyncResult, SyncService},
};
use drift_core::{
    config::{Host, RuntimeConfig},
    diff::Decision,
    error::Error,
    local::ProjectRoot,
    remote::ConnectionState,
    store::Store,
    tlstrust::{Manager, Problem},
};
use std::{
    fs,
    os::unix::fs::symlink,
    path::Path,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use support::ftp::Server as FtpServer;

// Keep event spelling here: assertions below do not depend on timestamps or field order.
const CONNECT_START: &str = "connection.start";
const CONNECT_COMPLETE: &str = "connection.connected";
const CONNECT_FAILED: &str = "connection.failed";
const MONITOR_FAILED: &str = "connection.monitor_failed";
const COMPARISON_START: &str = "comparison.start";
const COMPARISON_COMPLETE: &str = "comparison.completed";
const COMPARISON_FAILED: &str = "comparison.failed";
const ENTRY_FAILED: &str = "comparison.entry_failed";
const SYNC_START: &str = "sync.start";
const SYNC_COMPLETE: &str = "sync.finished";
const ITEM_START: &str = "sync.item_start";
const ITEM_FAILED: &str = "sync.item_failed";
const ITEM_UNKNOWN: &str = "sync.item_unknown";
const SYNC_STOPPED: &str = "sync.stopped";

fn id(operation: u64) -> OperationId {
    OperationId {
        project: 17,
        operation,
    }
}

fn write(path: &Path, bytes: &[u8], seconds: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::File::open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
        .unwrap();
}

fn open_logger(path: &Path, debug: bool) -> Logger {
    Logger::open(Options {
        path: path.into(),
        debug,
    })
    .unwrap()
}

fn finished_log(logger: &Logger, path: &Path, private: &[&str]) -> String {
    logger.finish().unwrap();
    let text = fs::read_to_string(path).unwrap();
    assert!(
        !text.is_empty(),
        "application operations produced no records"
    );
    for value in [
        "test-password",
        "test-passphrase",
        "BEGIN OPENSSH PRIVATE KEY",
        "BEGIN PRIVATE KEY",
    ]
    .into_iter()
    .chain(private.iter().copied())
    {
        assert!(!value.is_empty());
        assert!(
            !text.contains(value) && !text.contains(&format!("{value:?}")),
            "private data appeared in the app log"
        );
    }
    text
}

fn event_line<'a>(text: &'a str, event: &str) -> &'a str {
    text.lines()
        .find(|line| line.contains(event))
        .unwrap_or_else(|| panic!("missing application event {event:?}:\n{text}"))
}

fn failure_line<'a>(text: &'a str, event: &str) -> &'a str {
    let line = event_line(text, event);
    assert!(
        line.contains("error_kind="),
        "failure lacks a safe category: {line}"
    );
    assert!(!line.contains("error_kind=\"\""));
    assert!(
        line.contains("level=ERROR"),
        "failure is not an error record: {line}"
    );
    assert!(line.contains("host="));
    assert!(line.contains("project=\"17\""));
    assert!(line.contains("operation="));
    line
}

fn ftp_remote(server: &FtpServer, browser: BrowserService) -> RemoteService {
    let remote = RemoteService::with_options(browser, server.options());
    if server.host().protocol == "ftps" {
        remote.with_trust(Arc::new(Manager::with_roots(
            Store::new(server.dir.path().join("config")),
            server.roots(),
        )))
    } else {
        remote
    }
}

async fn setup(
    browser: BrowserService,
    remote: &RemoteService,
    host: Host,
    local: &Path,
) -> (SyncService, ComparisonService, LoadRequest) {
    let connection = tokio::time::timeout(
        Duration::from_secs(20),
        remote.connect(host.clone(), id(1)).task,
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap()
    .session;
    let location = Location {
        root: Arc::new(ProjectRoot::open(local).unwrap()),
        directory: local.into(),
        slug: Some("logging-project".into()),
        config: RuntimeConfig {
            hosts: vec![host.clone()],
            mappings: vec![],
            ui: Default::default(),
        },
    };
    (
        SyncService::new(browser.clone()),
        ComparisonService::new(browser),
        LoadRequest {
            location,
            host,
            connection,
            local: vec![],
            remote: vec![],
            scope: ScopeOptions::default(),
        },
    )
}

async fn compare(service: &ComparisonService, request: LoadRequest) -> ComparisonSession {
    tokio::time::timeout(
        Duration::from_secs(20),
        service.load(request, id(2)).operation.task,
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap()
}

async fn run(
    sync: &SyncService,
    session: &ComparisonSession,
    decisions: &[Decision],
) -> SyncResult {
    tokio::time::timeout(
        Duration::from_secs(20),
        sync.run(session, decisions, id(3)).unwrap().operation.task,
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn real_sftp_ftp_and_ftps_log_successful_lifecycles_without_credentials_or_contents() {
    for protocol in ["sftp", "ftp", "ftps"] {
        for debug in [false, true] {
            let sftp = (protocol == "sftp").then(|| support::Server::new(false));
            let ftp = (protocol != "sftp").then(|| {
                if protocol == "ftps" {
                    FtpServer::tls_with_limit("valid", 1)
                } else {
                    FtpServer::new(1)
                }
            });
            let local = tempfile::tempdir().unwrap();
            let logs = tempfile::tempdir().unwrap();
            let path = logs.path().join("app.log");
            let logger = open_logger(&path, debug);
            let browser = BrowserService::new().unwrap().with_logger(logger.clone());
            let (remote, host, files) = if let Some(server) = &sftp {
                let mut options = server.options();
                options.timeout = Duration::from_secs(15);
                (
                    RemoteService::with_options(browser.clone(), options),
                    server.key_host(true),
                    server.dir.path().join("files"),
                )
            } else {
                let server = ftp.as_ref().unwrap();
                (
                    ftp_remote(server, browser.clone()),
                    server.host(),
                    server.dir.path().join("files"),
                )
            };
            let local_content = "LOCAL-CONTENT-CANARY-logging-success";
            let remote_content = "REMOTE-CONTENT-CANARY-logging-success";
            write(
                &local.path().join("target.txt"),
                local_content.as_bytes(),
                100,
            );
            write(&files.join("target.txt"), remote_content.as_bytes(), 110);
            let (sync, comparison, request) = setup(browser, &remote, host, local.path()).await;
            let preview = tokio::time::timeout(
                Duration::from_secs(20),
                remote
                    .preview(
                        request.connection.clone(),
                        format!(
                            "{}/target.txt",
                            request.connection.root.trim_end_matches('/')
                        ),
                        id(4),
                    )
                    .task,
            )
            .await
            .unwrap()
            .unwrap()
            .unwrap();
            assert_eq!(preview, remote_content);
            let session = compare(&comparison, request.clone()).await;
            assert_eq!(session.entries.len(), 1);
            let report = run(&sync, &session, &[Decision::Upload]).await;
            assert_eq!(report.outcomes, [ItemOutcome::Completed]);
            assert!(report.stopped.is_none());
            assert_eq!(
                fs::read(files.join("target.txt")).unwrap(),
                local_content.as_bytes()
            );
            session.shutdown().await;

            let text = finished_log(&logger, &path, &[local_content, remote_content]);
            let mut previous = None;
            for event in [
                CONNECT_START,
                CONNECT_COMPLETE,
                COMPARISON_START,
                COMPARISON_COMPLETE,
                SYNC_START,
                SYNC_COMPLETE,
            ] {
                event_line(&text, event);
                let position = text.find(event).unwrap();
                assert!(
                    previous.is_none_or(|previous| previous < position),
                    "lifecycle order: {text}"
                );
                previous = Some(position);
            }
            assert!(text.contains(&request.host.name));
            for event in [
                CONNECT_FAILED,
                COMPARISON_FAILED,
                ENTRY_FAILED,
                ITEM_FAILED,
                ITEM_UNKNOWN,
                SYNC_STOPPED,
                MONITOR_FAILED,
            ] {
                assert!(
                    !text.contains(event),
                    "successful operation logged {event}: {text}"
                );
            }
            if debug {
                let item = event_line(&text, ITEM_START);
                assert!(item.contains("level=DEBUG"));
                assert!(item.contains("target.txt"));
                assert!(item.contains("decision=\"upload\""));
            } else {
                assert!(
                    !text.contains("level=DEBUG"),
                    "debug records escaped filtering"
                );
                assert!(!text.contains(ITEM_START));
            }
            assert!(!local.path().join("drift.log").exists());
        }
    }
}

#[tokio::test]
async fn real_sftp_wrong_auth_logs_a_category_without_password_or_raw_error() {
    let server = support::Server::new(false);
    let logs = tempfile::tempdir().unwrap();
    let path = logs.path().join("app.log");
    let logger = open_logger(&path, true);
    let remote = RemoteService::with_options(
        BrowserService::new().unwrap().with_logger(logger.clone()),
        server.options(),
    );
    let mut host = server.host();
    let password = "WRONG-SFTP-PASSWORD-CANARY";
    host.auth.password = password.into();
    let error = tokio::time::timeout(Duration::from_secs(20), remote.connect(host, id(1)).task)
        .await
        .unwrap()
        .unwrap()
        .err()
        .expect("the real SSH server accepted a wrong password");
    assert!(matches!(error, Error::Invalid(_)));
    let raw = error.to_string();
    let text = finished_log(&logger, &path, &[password, &raw, "authentication rejected"]);
    event_line(&text, CONNECT_START);
    assert!(failure_line(&text, CONNECT_FAILED).contains("error_kind=\"invalid\""));
    assert!(!text.contains(CONNECT_COMPLETE));
}

#[tokio::test]
async fn real_ftp_and_ftps_wrong_auth_log_categories_without_passwords_or_server_replies() {
    for tls in [false, true] {
        let server = if tls {
            FtpServer::tls_with_limit("valid", 1)
        } else {
            FtpServer::new(1)
        };
        let logs = tempfile::tempdir().unwrap();
        let path = logs.path().join("app.log");
        let logger = open_logger(&path, true);
        let remote = ftp_remote(
            &server,
            BrowserService::new().unwrap().with_logger(logger.clone()),
        );
        let mut host = server.host();
        let password = "WRONG-FTP-PASSWORD-CANARY";
        host.auth.password = password.into();
        let error = tokio::time::timeout(Duration::from_secs(20), remote.connect(host, id(1)).task)
            .await
            .unwrap()
            .unwrap()
            .err()
            .expect("the real FTP server accepted a wrong password");
        assert!(matches!(error, Error::Invalid(_)));
        assert_eq!(
            server.commands("PASS"),
            1,
            "authentication must reach the real server"
        );
        let raw = error.to_string();
        let text = finished_log(
            &logger,
            &path,
            &[password, &raw, "invalid credentials", "FTP login rejected"],
        );
        event_line(&text, CONNECT_START);
        assert!(failure_line(&text, CONNECT_FAILED).contains("error_kind=\"invalid\""));
        assert!(!text.contains(CONNECT_COMPLETE));
    }
}

#[tokio::test]
async fn real_ftps_certificate_challenge_logs_only_a_safe_failure_category() {
    let server = FtpServer::tls_with_limit("valid", 1);
    let logs = tempfile::tempdir().unwrap();
    let path = logs.path().join("app.log");
    let logger = open_logger(&path, true);
    let remote = RemoteService::with_options(
        BrowserService::new().unwrap().with_logger(logger.clone()),
        server.options(),
    )
    .with_trust(Arc::new(Manager::with_roots(
        Store::new(server.dir.path().join("config")),
        rustls::RootCertStore::empty(),
    )));
    let error = tokio::time::timeout(
        Duration::from_secs(20),
        remote.connect(server.host(), id(1)).task,
    )
    .await
    .unwrap()
    .unwrap()
    .err()
    .expect("an unknown certificate authority was accepted");
    let Error::Certificate { challenge, cause } = &error else {
        panic!("expected a typed FTPS challenge, got {error:?}");
    };
    assert_eq!(challenge.problems, [Problem::UnknownAuthority]);
    assert_eq!(server.commands("PASS"), 0);
    let raw = error.to_string();
    let text = finished_log(&logger, &path, &[&raw, cause]);
    event_line(&text, CONNECT_START);
    assert!(failure_line(&text, CONNECT_FAILED).contains("error_kind=\"certificate\""));
    assert!(!text.contains(CONNECT_COMPLETE));
}

#[tokio::test]
async fn real_symlink_escape_logs_a_comparison_entry_failure_and_preserves_the_session() {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let path = logs.path().join("app.log");
    let logger = open_logger(&path, true);
    let browser = BrowserService::new().unwrap().with_logger(logger.clone());
    let remote = RemoteService::with_options(browser.clone(), server.options());
    let secret = "OUTSIDE-CONTENT-CANARY-comparison";
    write(&outside.path().join("secret.txt"), secret.as_bytes(), 100);
    symlink(
        outside.path().join("secret.txt"),
        local.path().join("escape.txt"),
    )
    .unwrap();
    let (_, comparison, mut request) = setup(browser, &remote, server.host(), local.path()).await;
    request.local = vec!["escape.txt".into()];
    let session = compare(&comparison, request.clone()).await;
    assert_eq!(session.entries.len(), 1);
    let entry = &session.entries[0];
    assert!(entry.local.ends_with("escape.txt"));
    assert!(entry.result.is_none());
    let entry_error = entry.error.as_ref().expect("symlink escaped ProjectRoot");
    assert_eq!(request.connection.state(), ConnectionState::Connected);
    assert_eq!(
        fs::read(outside.path().join("secret.txt")).unwrap(),
        secret.as_bytes()
    );

    // A fatal scope error on the still-live session is distinct from an entry failure.
    request.local.clear();
    request.remote = vec![format!("{}/../outside", request.connection.root)];
    let error = tokio::time::timeout(
        Duration::from_secs(20),
        comparison.load(request.clone(), id(5)).operation.task,
    )
    .await
    .unwrap()
    .unwrap()
    .err()
    .expect("out-of-scope remote selection was accepted");
    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(request.connection.state(), ConnectionState::Closed);
    request.connection.shutdown().await;
    let raw = error.to_string();
    let text = finished_log(&logger, &path, &[secret, entry_error, &raw]);
    event_line(&text, COMPARISON_START);
    event_line(&text, COMPARISON_COMPLETE);
    assert!(failure_line(&text, ENTRY_FAILED).contains("escape.txt"));
    failure_line(&text, COMPARISON_FAILED);
}

#[tokio::test]
async fn real_symlink_swap_logs_failed_sync_without_losing_the_typed_report_or_later_items() {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let path = logs.path().join("app.log");
    let logger = open_logger(&path, true);
    let browser = BrowserService::new().unwrap().with_logger(logger.clone());
    let remote = RemoteService::with_options(browser.clone(), server.options());
    let files = server.dir.path().join("files");
    let local_content = "LOCAL-CONTENT-CANARY-sync";
    let old_remote = "REMOTE-CONTENT-CANARY-sync";
    let secret = "OUTSIDE-CONTENT-CANARY-sync";
    write(
        &local.path().join("a-upload.txt"),
        local_content.as_bytes(),
        100,
    );
    write(&files.join("a-upload.txt"), old_remote.as_bytes(), 110);
    write(
        &local.path().join("b-later.txt"),
        local_content.as_bytes(),
        100,
    );
    write(&outside.path().join("secret.txt"), secret.as_bytes(), 100);
    let (sync, comparison, request) = setup(browser, &remote, server.host(), local.path()).await;
    let session = compare(&comparison, request).await;
    assert_eq!(session.entries.len(), 2);
    fs::remove_file(local.path().join("a-upload.txt")).unwrap();
    symlink(
        outside.path().join("secret.txt"),
        local.path().join("a-upload.txt"),
    )
    .unwrap();
    let report = run(&sync, &session, &[Decision::Upload; 2]).await;
    let ItemOutcome::Failed(raw) = &report.outcomes[0] else {
        panic!("expected a typed item failure: {report:?}");
    };
    assert!(!raw.is_empty());
    assert_eq!(report.items[0].decision, Decision::Upload);
    assert_eq!(report.outcomes[1], ItemOutcome::Completed);
    assert!(report.stopped.is_none());
    assert_eq!(
        fs::read(files.join("a-upload.txt")).unwrap(),
        old_remote.as_bytes()
    );
    assert_eq!(
        fs::read(files.join("b-later.txt")).unwrap(),
        local_content.as_bytes()
    );
    assert_eq!(
        fs::read(outside.path().join("secret.txt")).unwrap(),
        secret.as_bytes()
    );
    session.shutdown().await;

    let text = finished_log(&logger, &path, &[local_content, old_remote, secret, raw]);
    event_line(&text, SYNC_START);
    event_line(&text, SYNC_COMPLETE);
    let failed = failure_line(&text, ITEM_FAILED);
    assert!(failed.contains("a-upload.txt"));
    assert!(failed.contains("decision=\"upload\""));
    assert!(!text.contains(ITEM_UNKNOWN));
    assert!(!text.contains(SYNC_STOPPED));
}

#[tokio::test]
async fn real_ftp_and_ftps_interrupted_upload_log_unknown_stop_and_monitor_failure_without_retry() {
    for tls in [false, true] {
        let mut server = if tls {
            FtpServer::tls_with_limit("valid", 1)
        } else {
            FtpServer::new(1)
        };
        let local = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let path = logs.path().join("app.log");
        let logger = open_logger(&path, true);
        let browser = BrowserService::new().unwrap().with_logger(logger.clone());
        let remote = ftp_remote(&server, browser.clone());
        let confirmed = "CONFIRMED-CONTENT-CANARY-interrupt";
        let old_remote = "OLD-REMOTE-CONTENT-CANARY-interrupt";
        let pending = "PENDING-CONTENT-CANARY-interrupt";
        let payload = b"UPLOAD-CONTENT-CANARY-interrupt\n".repeat(512 * 1024);
        write(&local.path().join("a-first.txt"), confirmed.as_bytes(), 100);
        write(&local.path().join("b-large.txt"), &payload, 100);
        write(
            &server.dir.path().join("files/b-large.txt"),
            old_remote.as_bytes(),
            110,
        );
        write(&local.path().join("c-later.txt"), pending.as_bytes(), 100);
        let (sync, comparison, request) =
            setup(browser, &remote, server.host(), local.path()).await;
        let session = compare(&comparison, request.clone()).await;
        assert_eq!(session.entries.len(), 3);
        let observer = remote.observe(request.connection.clone(), id(6));
        server.flag("slow-stor", "");
        let mut operation = sync
            .run(
                &session,
                &[Decision::DeleteLocal, Decision::Upload, Decision::Upload],
                id(3),
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while operation.progress.borrow_and_update().bytes < 64 * 1024 {
                operation.progress.changed().await.unwrap();
            }
        })
        .await
        .expect("the upload never made real progress");
        assert_eq!(server.commands("STOR"), 1);
        server.stop();
        let report = tokio::time::timeout(Duration::from_secs(20), operation.operation.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(report.outcomes[0], ItemOutcome::Completed);
        let ItemOutcome::Unknown(raw) = &report.outcomes[1] else {
            panic!("expected a typed unknown upload outcome: {report:?}");
        };
        assert_eq!(report.outcomes[2], ItemOutcome::Pending);
        let Some(StopReason::ConnectionLost(stop_error)) = &report.stopped else {
            panic!("expected connection loss: {report:?}");
        };
        let state = tokio::time::timeout(Duration::from_secs(10), observer.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let ConnectionState::Failed(monitor_error) = state else {
            panic!("monitor lost the terminal failure: {state:?}");
        };
        assert_eq!(
            fs::read(server.dir.path().join("files/b-large.txt")).unwrap(),
            old_remote.as_bytes()
        );
        assert!(!server.dir.path().join("files/c-later.txt").exists());
        assert!(sync.run(&session, &[Decision::Upload; 3], id(7)).is_err());
        assert_eq!(server.commands("STOR"), 1, "a failed transfer was retried");
        request.connection.shutdown().await;

        let text = finished_log(
            &logger,
            &path,
            &[
                confirmed,
                old_remote,
                pending,
                "UPLOAD-CONTENT-CANARY-interrupt",
                raw,
                raw.strip_suffix("; compare again before syncing")
                    .unwrap_or(raw),
                stop_error,
                &monitor_error,
            ],
        );
        event_line(&text, SYNC_START);
        let unknown = failure_line(&text, ITEM_UNKNOWN);
        assert!(unknown.contains("b-large.txt"));
        assert!(unknown.contains("decision=\"upload\""));
        assert!(event_line(&text, SYNC_STOPPED).contains("reason=\"connection_lost\""));
        let finished = event_line(&text, SYNC_COMPLETE);
        assert!(finished.contains("completed=\"1\""));
        assert!(finished.contains("unknown=\"1\""));
        assert!(finished.contains("pending=\"1\""));
        let monitor = event_line(&text, MONITOR_FAILED);
        assert!(monitor.contains("level=ERROR"));
        assert!(monitor.contains("category=\"connection\""));
        assert_eq!(
            text.lines()
                .filter(|line| line.contains(MONITOR_FAILED))
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn disabled_logging_preserves_real_operations_without_creating_files() {
    let server = support::Server::new(false);
    let sandbox = tempfile::tempdir().unwrap();
    let local = sandbox.path().join("project");
    let path = sandbox.path().join("logs/app.log");
    let content = b"DISABLED-CONTENT-CANARY";
    write(&local.join("target.txt"), content, 100);
    let logger = Logger::default();
    let browser = BrowserService::new().unwrap().with_logger(logger.clone());
    let remote = RemoteService::with_options(browser.clone(), server.options());
    let (sync, comparison, request) = setup(browser, &remote, server.host(), &local).await;
    let session = compare(&comparison, request).await;
    assert_eq!(session.entries.len(), 1);
    let report = run(&sync, &session, &[Decision::Upload]).await;
    assert_eq!(report.outcomes, [ItemOutcome::Completed]);
    assert!(report.stopped.is_none());
    assert_eq!(
        fs::read(server.dir.path().join("files/target.txt")).unwrap(),
        content
    );
    session.shutdown().await;
    logger.finish().unwrap();
    assert!(!path.exists());
    assert!(!path.parent().unwrap().exists());
    assert_eq!(fs::read_dir(sandbox.path()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&local).unwrap().count(), 1);
}
