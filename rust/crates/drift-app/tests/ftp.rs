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
    store::Store, tlstrust::Manager,
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, OnceLock},
    time::{Duration, UNIX_EPOCH},
};
use support::ftp::Server;
fn id(operation: u64) -> OperationId {
    OperationId {
        project: 1,
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
async fn setup(
    server: &Server,
    local: &Path,
    root: &str,
) -> (SyncService, ComparisonService, LoadRequest) {
    let browser = BrowserService::new().unwrap();
    let mut remote = RemoteService::with_options(browser.clone(), server.options());
    if server.host().protocol == "ftps" {
        remote = remote.with_trust(Arc::new(Manager::with_roots(
            Store::new(server.dir.path().join("config")),
            server.roots(),
        )));
    }
    let mut host = server.host();
    host.root_path = root.into();
    let directory = remote
        .connect(host.clone(), id(1))
        .task
        .await
        .unwrap()
        .unwrap();
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
            connection: directory.session,
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
fn go_probe(
    local: &Path,
    root: &str,
    server: &Server,
    input: serde_json::Value,
) -> serde_json::Value {
    static PROBE: OnceLock<PathBuf> = OnceLock::new();
    let binary = PROBE.get_or_init(|| {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("go.mod").is_file())
            .unwrap()
            .to_path_buf();
        let dir = tempfile::tempdir().unwrap().keep();
        let cache = tempfile::tempdir().unwrap();
        let binary = dir.join("comparison-probe");
        let result = Command::new("go")
            .args(["build", "-o"])
            .arg(&binary)
            .arg("./testdata/comparison-probe")
            .env("GOCACHE", cache.path())
            .current_dir(repository)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        binary
    });
    let home = tempfile::tempdir().unwrap();
    let mut child = support::Process(
        Command::new(binary)
            .arg(local)
            .arg(root)
            .arg(server.port.to_string())
            .arg(server.host().protocol)
            .env("SSL_CERT_FILE", server.dir.path().join("root.pem"))
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("HOME", home.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    // Small JSON output; waiting cannot fill the pipe in these scenarios.
    let status = child.wait().unwrap();
    let mut output = String::new();
    let mut error = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut error)
        .unwrap();
    assert!(status.success(), "{error}");
    serde_json::from_str(&output).unwrap()
}
fn rows(session: &ComparisonSession) -> serde_json::Value {
    serde_json::Value::Array(session.entries.iter().map(|e| {
        let r=e.result.as_ref();
        serde_json::json!({"local":e.local.strip_prefix(session.request.location.root.base()).unwrap().to_string_lossy(),"remote":Path::new(&e.remote).strip_prefix(&session.request.connection.root).unwrap().to_string_lossy(),"status":if e.error.is_some(){"Error"}else if r.unwrap().local.is_none(){"Remote only"}else if r.unwrap().remote.is_none(){"Local only"}else{"Changed"},"decision":r.map_or(Decision::Skip,|r|r.suggestion()).label(),"binary":r.is_some_and(|r|r.binary)})
    }).collect())
}
#[tokio::test]
async fn real_ftp_comparison_and_all_sync_actions_match_go() {
    for tls in [false, true] {
        let server = if tls {
            Server::tls_with_limit("valid", 12)
        } else {
            Server::new(12)
        };
        let local = tempfile::tempdir().unwrap();
        let go_local = tempfile::tempdir().unwrap();
        let remote = server.dir.path().join("files/rust");
        let go_remote = server.dir.path().join("files/go");
        for root in [&remote, &go_remote] {
            fs::create_dir_all(root).unwrap();
        }
        let large = vec![b'x'; 2 * 1024 * 1024 + 100];
        for (name, l, r) in [
            ("same", Some(b"same".as_slice()), Some(b"same".as_slice())),
            (
                "a-upload",
                Some(b"new local".as_slice()),
                Some(b"old remote".as_slice()),
            ),
            (
                "b-download",
                Some(b"old local".as_slice()),
                Some(b"new remote".as_slice()),
            ),
            ("new/upload", Some(large.as_slice()), None),
            ("newremote/download", None, Some(large.as_slice())),
            ("c-delete-local", Some(b"delete".as_slice()), None),
            ("d-delete-remote", None, Some(b"delete".as_slice())),
            ("e-skip", Some(b"skip".as_slice()), None),
            ("binary", Some(b"a\0b".as_slice()), Some(b"c\0d".as_slice())),
        ] {
            for root in [local.path(), go_local.path()] {
                if let Some(bytes) = l {
                    write(&root.join(name), bytes, 100);
                }
            }
            for root in [&remote, &go_remote] {
                if let Some(bytes) = r {
                    write(&root.join(name), bytes, 110);
                }
            }
        }
        let (sync, comparison, request) = setup(&server, local.path(), "/rust").await;
        let expected = go_probe(
            local.path(),
            "/rust",
            &server,
            serde_json::json!({"local":[local.path()]}),
        );
        let session = compare(&comparison, request.clone()).await;
        assert_eq!(rows(&session), expected);
        let mut chosen = serde_json::Map::new();
        let decisions: Vec<_> = session
            .entries
            .iter()
            .map(|e| {
                let relative = e
                    .local
                    .strip_prefix(local.path())
                    .unwrap()
                    .to_str()
                    .unwrap();
                let decision = match relative {
                    "a-upload" | "new/upload" => Decision::Upload,
                    "b-download" | "newremote/download" => Decision::Download,
                    "c-delete-local" => Decision::DeleteLocal,
                    "d-delete-remote" => Decision::DeleteRemote,
                    _ => Decision::Skip,
                };
                chosen.insert(relative.into(), serde_json::json!(decision.label()));
                decision
            })
            .collect();
        let report = run(&sync, &session, &decisions).await;
        assert!(report.stopped.is_none());
        assert_eq!(
            report
                .outcomes
                .iter()
                .filter(|outcome| **outcome == ItemOutcome::Completed)
                .count(),
            6
        );
        assert_eq!(
            report
                .outcomes
                .iter()
                .filter(|outcome| matches!(
                    outcome,
                    ItemOutcome::Failed(_) | ItemOutcome::Unknown(_)
                ))
                .count(),
            0
        );
        let expected = go_probe(
            go_local.path(),
            "/go",
            &server,
            serde_json::json!({"local":[go_local.path()],"sync":chosen}),
        );
        assert_eq!(expected["failures"], 0);
        assert_eq!(
            expected["completed"].as_array().unwrap().len(),
            report
                .outcomes
                .iter()
                .filter(|outcome| **outcome == ItemOutcome::Completed)
                .count()
        );
        for (a, b) in [
            (local.path(), go_local.path()),
            (remote.as_path(), go_remote.as_path()),
        ] {
            for name in [
                "same",
                "a-upload",
                "b-download",
                "new/upload",
                "newremote/download",
                "c-delete-local",
                "d-delete-remote",
                "e-skip",
                "binary",
            ] {
                assert_eq!(
                    fs::read(a.join(name)).ok(),
                    fs::read(b.join(name)).ok(),
                    "{name}"
                );
            }
        }
        let refreshed = compare(&comparison, request.clone()).await;
        assert_eq!(refreshed.entries.len(), 2); // the two skipped differences remain
        request.connection.shutdown().await;
        assert!(server.commands("USER") >= 6); // Both applications connect independently.
    }
}
#[tokio::test]
async fn ftp_completion_failures_preserve_targets_and_stop_later_actions() {
    for tls in [false, true] {
        for command in ["RETR", "STOR"] {
            let server = if tls {
                Server::tls_with_limit("valid", 1)
            } else {
                Server::new(1)
            };
            let local = tempfile::tempdir().unwrap();
            write(&local.path().join("a-first"), b"delete", 100);
            write(&local.path().join("b-target"), b"old local", 100);
            write(
                &server.dir.path().join("files/b-target"),
                b"new remote",
                110,
            );
            write(&local.path().join("c-later"), b"pending", 100);
            let (sync, comparison, request) = setup(&server, local.path(), "/").await;
            let session = compare(&comparison, request.clone()).await;
            if command == "RETR" {
                server.flag("fail-completion", "RETR /b-target");
            } else {
                server.flag("drop-completion", "STOR");
            }
            let direction = if command == "RETR" {
                Decision::Download
            } else {
                Decision::Upload
            };
            let report = run(
                &sync,
                &session,
                &[Decision::DeleteLocal, direction, Decision::Upload],
            )
            .await;
            assert_eq!(report.outcomes[0], ItemOutcome::Completed);
            assert!(matches!(
                report.outcomes[1],
                ItemOutcome::Failed(_) | ItemOutcome::Unknown(_)
            ));
            assert_eq!(report.outcomes[2], ItemOutcome::Pending);
            assert!(matches!(
                report.stopped,
                Some(StopReason::ConnectionLost(_))
            ));
            assert_eq!(
                fs::read(local.path().join("b-target")).unwrap(),
                b"old local"
            );
            assert_eq!(
                fs::read(server.dir.path().join("files/b-target")).unwrap(),
                b"new remote"
            );
            assert!(sync.run(&session, &[Decision::Skip; 3], id(4)).is_err());
        }
    }
}
#[tokio::test]
async fn ftp_access_failures_have_no_sync_action_and_rename_failures_keep_old_files() {
    for tls in [false, true] {
        let server = if tls {
            Server::tls_with_limit("valid", 12)
        } else {
            Server::new(12)
        };
        let local = tempfile::tempdir().unwrap();
        write(&local.path().join("a-target"), b"new local", 100);
        write(
            &server.dir.path().join("files/a-target"),
            b"old remote",
            110,
        );
        write(&local.path().join("b-later"), b"later", 100);
        write(&server.dir.path().join("files/denied"), b"secret", 110);
        let (sync, comparison, request) = setup(&server, local.path(), "/").await;
        server.flag("deny", "MLST /denied\nSIZE /denied\nLIST /denied");
        let session = compare(&comparison, request.clone()).await;
        let denied = session
            .entries
            .iter()
            .find(|e| e.local.ends_with("denied"))
            .unwrap();
        assert!(denied.error.is_some());
        assert!(denied.result.is_none());
        let expected = go_probe(
            local.path(),
            "/",
            &server,
            serde_json::json!({"local":[local.path()]}),
        );
        // Both MLST and fallback commands must be denied in Go to retain the error.
        assert_eq!(expected, rows(&session));
        server.flag("deny", "RNTO /a-target");
        let decisions: Vec<_> = session
            .entries
            .iter()
            .map(|e| {
                if e.error.is_some() {
                    Decision::Skip
                } else {
                    Decision::Upload
                }
            })
            .collect();
        let report = run(&sync, &session, &decisions).await;
        assert!(report.stopped.is_none());
        assert_eq!(
            report
                .outcomes
                .iter()
                .filter(|outcome| **outcome == ItemOutcome::Completed)
                .count(),
            1
        );
        assert_eq!(
            report
                .outcomes
                .iter()
                .filter(|outcome| matches!(
                    outcome,
                    ItemOutcome::Failed(_) | ItemOutcome::Unknown(_)
                ))
                .count(),
            1
        );
        assert_eq!(
            fs::read(server.dir.path().join("files/a-target")).unwrap(),
            b"old remote"
        );
        assert_eq!(
            fs::read(server.dir.path().join("files/b-later")).unwrap(),
            b"later"
        );
        assert!(
            fs::read_dir(server.dir.path().join("files"))
                .unwrap()
                .all(|e| !drift_core::staging::is_staging_name(
                    &e.unwrap().file_name().to_string_lossy()
                ))
        );
        assert_eq!(request.connection.state(), ConnectionState::Connected);
        request.connection.shutdown().await;
    }
}
#[tokio::test]
async fn ftp_cancellation_retains_confirmed_actions_without_repeating_transfers() {
    for tls in [false, true] {
        for direction in [Decision::Upload, Decision::Download] {
            let server = if tls {
                Server::tls_with_limit("valid", 1)
            } else {
                Server::new(1)
            };
            let local = tempfile::tempdir().unwrap();
            write(&local.path().join("a-first"), b"delete", 100);
            let large = vec![b'x'; 16 * 1024 * 1024];
            let (l, r) = if direction == Decision::Upload {
                (large.as_slice(), b"old remote".as_slice())
            } else {
                (b"old local".as_slice(), large.as_slice())
            };
            write(&local.path().join("b-large"), l, 100);
            write(&server.dir.path().join("files/b-large"), r, 110);
            write(&local.path().join("c-later"), b"pending", 100);
            let (sync, comparison, request) = setup(&server, local.path(), "/").await;
            let session = compare(&comparison, request.clone()).await;
            server.flag(
                if direction == Decision::Upload {
                    "slow-stor"
                } else {
                    "slow-retr"
                },
                "",
            );
            let mut operation = sync
                .run(
                    &session,
                    &[Decision::DeleteLocal, direction, Decision::Upload],
                    id(3),
                )
                .unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                while operation.progress.borrow().bytes < 64 * 1024 {
                    operation.progress.changed().await.unwrap();
                }
            })
            .await
            .unwrap();
            operation.operation.cancel.cancel();
            let report = tokio::time::timeout(Duration::from_secs(5), operation.operation.task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(report.outcomes[0], ItemOutcome::Completed);
            assert!(matches!(report.stopped, Some(StopReason::Cancelled)));
            assert_eq!(report.outcomes[2], ItemOutcome::Pending);
            assert_eq!(fs::read(local.path().join("b-large")).unwrap(), l);
            assert_eq!(
                fs::read(server.dir.path().join("files/b-large")).unwrap(),
                r
            );
            assert_eq!(request.connection.state(), ConnectionState::Closed);
            assert_eq!(
                server.commands(if direction == Decision::Upload {
                    "STOR"
                } else {
                    "RETR"
                }),
                1
            );
        }
    }
}

#[tokio::test]
async fn losing_the_real_ftp_server_mid_upload_retains_confirmed_and_unknown_outcomes() {
    for tls in [false, true] {
        let mut server = if tls {
            Server::tls_with_limit("valid", 1)
        } else {
            Server::new(1)
        };
        let local = tempfile::tempdir().unwrap();
        write(&local.path().join("a-first"), b"delete", 100);
        write(
            &local.path().join("b-large"),
            &vec![b'x'; 16 * 1024 * 1024],
            100,
        );
        write(&server.dir.path().join("files/b-large"), b"old remote", 110);
        write(&local.path().join("c-later"), b"pending", 100);
        let (sync, comparison, request) = setup(&server, local.path(), "/").await;
        let session = compare(&comparison, request.clone()).await;
        server.flag("slow-stor", "");
        let mut operation = sync
            .run(
                &session,
                &[Decision::DeleteLocal, Decision::Upload, Decision::Upload],
                id(3),
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while operation.progress.borrow().bytes < 64 * 1024 {
                operation.progress.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        server.stop();
        let report = tokio::time::timeout(Duration::from_secs(5), operation.operation.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(report.outcomes[0], ItemOutcome::Completed);
        assert!(matches!(report.outcomes[1], ItemOutcome::Unknown(_)));
        assert_eq!(report.outcomes[2], ItemOutcome::Pending);
        assert!(matches!(
            report.stopped,
            Some(StopReason::ConnectionLost(_))
        ));
        assert_eq!(
            fs::read(server.dir.path().join("files/b-large")).unwrap(),
            b"old remote"
        );
        assert_eq!(server.commands("STOR"), 1);
    }
}

#[tokio::test]
async fn a_lost_ftp_rename_acknowledgement_reports_unknown_even_when_committed() {
    for tls in [false, true] {
        let server = if tls {
            Server::tls_with_limit("valid", 1)
        } else {
            Server::new(1)
        };
        let local = tempfile::tempdir().unwrap();
        write(&local.path().join("a-target"), b"new local", 100);
        write(
            &server.dir.path().join("files/a-target"),
            b"old remote",
            110,
        );
        write(&local.path().join("b-pending"), b"later", 100);
        let (sync, comparison, request) = setup(&server, local.path(), "/").await;
        let session = compare(&comparison, request.clone()).await;
        server.flag("drop-rename-reply", "");
        let report = run(&sync, &session, &[Decision::Upload, Decision::Upload]).await;
        assert!(matches!(report.outcomes[0], ItemOutcome::Unknown(_)));
        assert_eq!(report.outcomes[1], ItemOutcome::Pending);
        assert!(matches!(
            report.stopped,
            Some(StopReason::ConnectionLost(_))
        ));
        // The server did commit: lack of acknowledgement still forbids claiming success or retrying.
        assert_eq!(
            fs::read(server.dir.path().join("files/a-target")).unwrap(),
            b"new local"
        );
        assert_eq!(server.commands("STOR"), 1);
        assert_eq!(server.commands("RNTO"), 1);
        assert!(!server.dir.path().join("files/b-pending").exists());
        assert!(sync.run(&session, &[Decision::Upload; 2], id(4)).is_err());
    }
}

#[tokio::test]
async fn ftps_data_certificate_change_preserves_confirmed_actions_and_old_download_target() {
    let server = Server::tls("valid");
    let local = tempfile::tempdir().unwrap();
    write(&local.path().join("a-confirmed"), b"delete", 100);
    write(&local.path().join("b-target"), b"old local", 100);
    write(
        &server.dir.path().join("files/b-target"),
        b"new remote",
        110,
    );
    write(&local.path().join("c-pending"), b"pending", 100);
    let (sync, comparison, request) = setup(&server, local.path(), "/").await;
    let session = compare(&comparison, request.clone()).await;
    let reads = server.commands("RETR");
    server.flag("rotate-data", "");
    let report = run(
        &sync,
        &session,
        &[Decision::DeleteLocal, Decision::Download, Decision::Upload],
    )
    .await;
    assert_eq!(report.outcomes[0], ItemOutcome::Completed);
    assert!(matches!(report.outcomes[1], ItemOutcome::Failed(_)));
    assert_eq!(report.outcomes[2], ItemOutcome::Pending);
    assert!(matches!(
        report.stopped,
        Some(StopReason::ConnectionLost(_))
    ));
    assert!(matches!(
        request.connection.state(),
        ConnectionState::Certificate(_)
    ));
    assert_eq!(
        fs::read(local.path().join("b-target")).unwrap(),
        b"old local"
    );
    assert_eq!(server.commands("RETR"), reads + 1);
    assert_eq!(server.commands("STOR"), 0);
    assert!(sync.run(&session, &[Decision::Skip; 3], id(4)).is_err());
}
