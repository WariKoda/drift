#[path = "../../drift-core/tests/support/mod.rs"]
mod support;
use drift_app::{
    browser::{BrowserService, Location, OperationId},
    comparison::{ComparisonService, ComparisonSession, LoadRequest, ScopeOptions},
    remote::RemoteService,
    sync::{ItemOutcome, StopReason, SyncResult, SyncService},
};
use drift_core::{
    config::RuntimeConfig, diff::Decision, local::ProjectRoot, remote::ConnectionState,
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

fn write(path: &Path, bytes: &[u8], seconds: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::File::open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
        .unwrap();
}
fn id(operation: u64) -> OperationId {
    OperationId {
        project: 1,
        operation,
    }
}
async fn setup(
    server: &support::Server,
    local: &Path,
) -> (SyncService, ComparisonService, LoadRequest) {
    let browser = BrowserService::new().unwrap();
    let remote = RemoteService::with_options(browser.clone(), server.options());
    let host = server.host();
    let connection = remote
        .connect(host.clone(), id(1))
        .task
        .await
        .unwrap()
        .unwrap()
        .session;
    let location = Location {
        root: Arc::new(ProjectRoot::open(local).unwrap()),
        directory: local.into(),
        slug: None,
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
        Duration::from_secs(30),
        service.load(request, id(2)).operation.task,
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap()
}
async fn run(
    service: &SyncService,
    session: &ComparisonSession,
    decisions: &[Decision],
) -> SyncResult {
    tokio::time::timeout(
        Duration::from_secs(30),
        service
            .run(session, decisions, id(3))
            .unwrap()
            .operation
            .task,
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap()
}
fn assert_no_staging(path: &Path) {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        assert!(!drift_core::staging::is_staging_name(
            &entry.file_name().to_string_lossy()
        ));
        if entry.file_type().unwrap().is_dir() {
            assert_no_staging(&entry.path());
        }
    }
}

#[tokio::test]
async fn real_sftp_executes_all_actions_streams_large_files_and_preserves_permissions() {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    write(&local.path().join("a-upload"), b"new local", 100);
    write(&remote.join("a-upload"), b"old remote", 110);
    write(&local.path().join("b-download"), b"old local", 100);
    write(&remote.join("b-download"), b"new remote", 110);
    for path in [remote.join("a-upload"), local.path().join("b-download")] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let large = vec![b'x'; 2 * 1024 * 1024 + 100];
    write(&local.path().join("newdir/upload-large"), &large, 100);
    write(&remote.join("newremote/download-large"), &large, 110);
    write(&local.path().join("c-delete-local"), b"delete", 100);
    write(&remote.join("d-delete-remote"), b"delete", 110);
    write(&local.path().join("e-skip"), b"skip", 100);
    let go_local = tempfile::tempdir().unwrap();
    let go_remote = server.dir.path().join("go-files");
    for (source, target) in [
        (local.path(), go_local.path()),
        (remote.as_path(), go_remote.as_path()),
    ] {
        fs::create_dir_all(target).unwrap();
        let mut directories = vec![std::path::PathBuf::new()];
        while let Some(relative) = directories.pop() {
            for entry in fs::read_dir(source.join(&relative)).unwrap() {
                let entry = entry.unwrap();
                let path = relative.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    fs::create_dir_all(target.join(&path)).unwrap();
                    directories.push(path);
                } else {
                    fs::copy(entry.path(), target.join(&path)).unwrap();
                    fs::File::open(target.join(&path))
                        .unwrap()
                        .set_times(
                            fs::FileTimes::new()
                                .set_modified(entry.metadata().unwrap().modified().unwrap()),
                        )
                        .unwrap();
                }
            }
        }
    }
    let (sync, comparison, request) = setup(&server, local.path()).await;
    let session = compare(&comparison, request.clone()).await;
    let decisions: Vec<_> = session
        .entries
        .iter()
        .map(|e| match e.local.file_name().unwrap().to_str().unwrap() {
            "a-upload" | "upload-large" => Decision::Upload,
            "b-download" | "download-large" => Decision::Download,
            "c-delete-local" => Decision::DeleteLocal,
            "d-delete-remote" => Decision::DeleteRemote,
            _ => Decision::Skip,
        })
        .collect();
    let report = run(&sync, &session, &decisions).await;
    let expected = go_sync(&server, go_local.path(), &go_remote, &session, &decisions);
    let mut completed: Vec<_> = report
        .items
        .iter()
        .zip(&report.outcomes)
        .filter(|(_, outcome)| **outcome == ItemOutcome::Completed)
        .map(|(item, _)| {
            item.local
                .strip_prefix(local.path())
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    completed.sort();
    assert_eq!(
        serde_json::json!({"completed":completed,"failures":0}),
        expected
    );
    assert!(report.stopped.is_none());
    assert_eq!(
        report
            .outcomes
            .iter()
            .filter(|o| **o == ItemOutcome::Completed)
            .count(),
        6
    );
    assert_eq!(
        report
            .outcomes
            .iter()
            .filter(|o| **o == ItemOutcome::Skipped)
            .count(),
        1
    );
    assert_eq!(fs::read(remote.join("a-upload")).unwrap(), b"new local");
    assert_eq!(
        fs::read(local.path().join("b-download")).unwrap(),
        b"new remote"
    );
    assert_eq!(fs::read(remote.join("newdir/upload-large")).unwrap(), large);
    assert_eq!(
        fs::read(local.path().join("newremote/download-large")).unwrap(),
        large
    );
    for path in [remote.join("a-upload"), local.path().join("b-download")] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    assert!(!local.path().join("c-delete-local").exists());
    assert!(!remote.join("d-delete-remote").exists());
    assert!(!remote.join("e-skip").exists());
    assert_no_staging(local.path());
    assert_no_staging(&remote);
    for (rust_tree, go_tree) in [
        (local.path(), go_local.path()),
        (remote.as_path(), go_remote.as_path()),
    ] {
        let mut directories = vec![std::path::PathBuf::new()];
        while let Some(relative) = directories.pop() {
            let mut actual: Vec<_> = fs::read_dir(rust_tree.join(&relative))
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            let mut expected: Vec<_> = fs::read_dir(go_tree.join(&relative))
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            actual.sort();
            expected.sort();
            assert_eq!(actual, expected);
            for name in actual {
                let path = relative.join(name);
                let actual = rust_tree.join(&path);
                let expected = go_tree.join(&path);
                if actual.is_dir() {
                    directories.push(path);
                } else {
                    assert_eq!(fs::read(&actual).unwrap(), fs::read(&expected).unwrap());
                    assert_eq!(
                        fs::metadata(&actual).unwrap().permissions().mode() & 0o777,
                        fs::metadata(&expected).unwrap().permissions().mode() & 0o777
                    );
                }
            }
        }
    }
    let refreshed = compare(&comparison, request).await;
    assert_eq!(refreshed.entries.len(), 1);
    assert!(refreshed.entries[0].local.ends_with("e-skip"));
    refreshed.shutdown().await;
}

fn go_sync(
    server: &support::Server,
    local: &Path,
    remote: &Path,
    session: &ComparisonSession,
    decisions: &[Decision],
) -> serde_json::Value {
    use std::{
        io::Write,
        process::{Command, Stdio},
        sync::OnceLock,
    };
    static PROBE: OnceLock<std::path::PathBuf> = OnceLock::new();
    let binary = PROBE.get_or_init(|| {
        let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("go.mod").is_file())
            .unwrap()
            .to_path_buf();
        let dir = tempfile::tempdir().unwrap().keep();
        let cache = tempfile::tempdir().unwrap();
        let binary = dir.join("sync-probe");
        let output = Command::new("go")
            .args(["build", "-o"])
            .arg(&binary)
            .arg("./testdata/comparison-probe")
            .env("GOCACHE", cache.path())
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        binary
    });
    let home = tempfile::tempdir().unwrap();
    let choices: serde_json::Map<String, serde_json::Value> = session
        .entries
        .iter()
        .zip(decisions)
        .map(|(entry, decision)| {
            (
                entry
                    .local
                    .strip_prefix(session.request.location.root.base())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                decision.label().into(),
            )
        })
        .collect();
    let input = serde_json::json!({"local":[local],"sync":choices});
    let mut child = Command::new(binary)
        .arg(local)
        .arg(remote)
        .arg(server.port.to_string())
        .env("HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test]
async fn filesystem_changes_after_comparison_fail_individually_without_escaping_or_stopping_later_items()
 {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    for name in ["a-source", "b-target", "c-remote-target", "d-later"] {
        write(&local.path().join(name), b"local contents", 100);
        write(&remote.join(name), b"remote contents", 110);
    }
    write(&outside.path().join("secret"), b"unchanged secret", 100);
    let (sync, comparison, request) = setup(&server, local.path()).await;
    let mut session = compare(&comparison, request.clone()).await;
    let decisions = [
        Decision::Upload,
        Decision::Download,
        Decision::Upload,
        Decision::Upload,
    ];
    assert!(sync.run(&session, &decisions[..2], id(3)).is_err());
    let original = session.entries[0].remote.clone();
    session.entries[0].remote = outside.path().join("escape").to_string_lossy().into_owned();
    assert!(sync.run(&session, &decisions, id(3)).is_err());
    session.entries[0].remote = original;
    fs::remove_file(local.path().join("a-source")).unwrap();
    symlink(outside.path().join("secret"), local.path().join("a-source")).unwrap();
    fs::remove_file(local.path().join("b-target")).unwrap();
    symlink(outside.path().join("secret"), local.path().join("b-target")).unwrap();
    fs::remove_file(remote.join("c-remote-target")).unwrap();
    symlink(
        outside.path().join("secret"),
        remote.join("c-remote-target"),
    )
    .unwrap();
    let report = run(&sync, &session, &decisions).await;
    assert!(report.stopped.is_none());
    assert!(
        report.outcomes[..3]
            .iter()
            .all(|o| matches!(o, ItemOutcome::Failed(_)))
    );
    assert_eq!(report.outcomes[3], ItemOutcome::Completed);
    assert_eq!(
        fs::read(outside.path().join("secret")).unwrap(),
        b"unchanged secret"
    );
    assert_eq!(
        fs::read(remote.join("a-source")).unwrap(),
        b"remote contents"
    );
    assert_eq!(fs::read(remote.join("d-later")).unwrap(), b"local contents");
    assert_no_staging(local.path());
    assert_no_staging(&remote);
    let failed_comparison = compare(&comparison, request).await;
    assert!(
        sync.run(
            &failed_comparison,
            &vec![Decision::Upload; failed_comparison.entries.len()],
            id(4)
        )
        .is_err()
    );
    failed_comparison.shutdown().await;
}

#[tokio::test]
async fn cancelling_active_upload_and_download_preserves_old_targets_and_confirmed_completions() {
    for direction in [Decision::Upload, Decision::Download] {
        let server = support::Server::new(false);
        let local = tempfile::tempdir().unwrap();
        let remote = server.dir.path().join("files");
        let (source, target) = if direction == Decision::Upload {
            (local.path(), remote.as_path())
        } else {
            (remote.as_path(), local.path())
        };
        write(&source.join("a-first"), b"confirmed", 100);
        write(&source.join("b-large"), &vec![b'x'; 16 * 1024 * 1024], 100);
        write(&target.join("b-large"), b"old target", 110);
        write(&source.join("c-later"), b"must not transfer", 100);
        let (sync, comparison, request) = setup(&server, local.path()).await;
        let session = compare(&comparison, request.clone()).await;
        let mut operation = sync.run(&session, &[direction; 3], id(3)).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while operation.progress.borrow_and_update().bytes < 64 * 1024 {
                operation.progress.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        operation.operation.cancel.cancel();
        let report = tokio::time::timeout(Duration::from_secs(15), operation.operation.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(report.stopped, Some(StopReason::Cancelled));
        assert_eq!(report.outcomes[0], ItemOutcome::Completed);
        assert!(matches!(
            &report.outcomes[1],
            ItemOutcome::Unknown(_) | ItemOutcome::Failed(_)
        ));
        assert_eq!(report.outcomes[2], ItemOutcome::Pending);
        assert_eq!(fs::read(target.join("a-first")).unwrap(), b"confirmed");
        assert_eq!(fs::read(target.join("b-large")).unwrap(), b"old target");
        assert!(!target.join("c-later").exists());
        assert_eq!(request.connection.state(), ConnectionState::Closed);
        assert!(sync.run(&session, &[direction; 3], id(4)).is_err());
        if direction == Decision::Download {
            assert_no_staging(local.path());
        }
        let (_, comparison, fresh_request) = setup(&server, local.path()).await;
        let fresh = compare(&comparison, fresh_request).await;
        assert_eq!(fresh.entries.len(), 2);
        assert!(
            fresh
                .entries
                .iter()
                .all(|entry| entry.local.ends_with("b-large") || entry.local.ends_with("c-later"))
        );
        fresh.shutdown().await;
    }
}

#[tokio::test]
async fn server_loss_during_upload_reports_unknown_outcome_and_never_retries() {
    let mut server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    write(&local.path().join("a-first"), b"confirmed", 100);
    write(
        &local.path().join("b-large"),
        &vec![b'x'; 16 * 1024 * 1024],
        100,
    );
    write(&remote.join("b-large"), b"old target", 110);
    write(&local.path().join("c-later"), b"must not transfer", 100);
    let (sync, comparison, request) = setup(&server, local.path()).await;
    let session = compare(&comparison, request.clone()).await;
    let mut operation = sync.run(&session, &[Decision::Upload; 3], id(3)).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while operation.progress.borrow_and_update().bytes < 64 * 1024 {
            operation.progress.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    server.stop();
    let report = tokio::time::timeout(Duration::from_secs(20), operation.operation.task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        report.stopped,
        Some(StopReason::ConnectionLost(_))
    ));
    assert_eq!(report.outcomes[0], ItemOutcome::Completed);
    assert!(matches!(report.outcomes[1], ItemOutcome::Unknown(_)));
    assert_eq!(report.outcomes[2], ItemOutcome::Pending);
    assert_eq!(fs::read(remote.join("b-large")).unwrap(), b"old target");
    assert!(!remote.join("c-later").exists());
    assert_ne!(request.connection.state(), ConnectionState::Connected);
    assert!(sync.run(&session, &[Decision::Upload; 3], id(4)).is_err());
}

#[tokio::test]
async fn download_eof_without_close_acknowledgement_never_replaces_the_old_target() {
    for rejected_target in [false, true] {
        let server = support::Server::new(false);
        let local = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let remote = server.dir.path().join("files");
        write(&local.path().join("a-download"), b"old local", 100);
        write(&remote.join("a-download"), b"complete remote data", 110);
        write(&remote.join("b-later"), b"must not transfer", 110);
        let (sync, comparison, request) = setup(&server, local.path()).await;
        let session = compare(&comparison, request.clone()).await;
        if rejected_target {
            write(&outside.path().join("secret"), b"old local", 100);
            fs::remove_file(local.path().join("a-download")).unwrap();
            symlink(
                outside.path().join("secret"),
                local.path().join("a-download"),
            )
            .unwrap();
        }
        fs::write(server.dir.path().join("drop-on-close"), "armed").unwrap();
        let operation = sync.run(&session, &[Decision::Download; 2], id(3)).unwrap();
        let report = tokio::time::timeout(Duration::from_secs(20), operation.operation.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            operation.progress.borrow().bytes,
            if rejected_target {
                0
            } else {
                b"complete remote data".len() as u64
            }
        );
        assert!(server.dir.path().join("close-dropped").exists());
        assert!(matches!(
            report.stopped,
            Some(StopReason::ConnectionLost(_))
        ));
        assert!(matches!(report.outcomes[0], ItemOutcome::Failed(_)));
        assert_eq!(report.outcomes[1], ItemOutcome::Pending);
        assert_eq!(
            fs::read(local.path().join("a-download")).unwrap(),
            b"old local"
        );
        assert!(!local.path().join("b-later").exists());
        assert_no_staging(local.path());
    }
}

#[tokio::test]
async fn one_channel_servers_keep_browsing_and_use_standard_rename_without_deleting_targets() {
    let server = support::Server::new(false);
    fs::write(server.dir.path().join("one-channel"), "limited").unwrap();
    let local = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    write(&local.path().join("a-replace"), b"new local", 100);
    write(&remote.join("a-replace"), b"old remote", 110);
    write(&local.path().join("b-new"), b"new file", 100);
    let (sync, comparison, request) = setup(&server, local.path()).await;
    let session = compare(&comparison, request.clone()).await;
    let report = run(&sync, &session, &[Decision::Upload; 2]).await;
    assert!(report.stopped.is_none());
    assert_eq!(report.outcomes[0], ItemOutcome::Completed);
    assert_eq!(report.outcomes[1], ItemOutcome::Completed);
    assert_eq!(fs::read(remote.join("a-replace")).unwrap(), b"new local");
    assert_eq!(fs::read(remote.join("b-new")).unwrap(), b"new file");
    assert_no_staging(&remote);
    assert_eq!(request.connection.state(), ConnectionState::Connected);
    request.connection.shutdown().await;
}
