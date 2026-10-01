#[path = "../../drift-core/tests/support/mod.rs"]
mod support;
use drift_app::{
    browser::{BrowserService, Location, OperationId},
    comparison::{ComparisonService, ComparisonSession, LoadRequest, ScopeOptions},
    remote::RemoteService,
};
use drift_core::{
    config::RuntimeConfig, diff::Decision, local::ProjectRoot, pathmap::Mapping,
    remote::ConnectionState,
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, OnceLock},
    time::{Duration, UNIX_EPOCH},
};

fn write(path: &Path, text: &[u8], seconds: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
    fs::File::open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
        .unwrap();
}
async fn request(
    server: &support::Server,
    local: &Path,
    mappings: Vec<Mapping>,
) -> (ComparisonService, LoadRequest) {
    let browser = BrowserService::new().unwrap();
    let remote = RemoteService::with_options(browser.clone(), server.options());
    let host = server.host();
    let connection = remote
        .connect(
            host.clone(),
            OperationId {
                project: 1,
                operation: 1,
            },
        )
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
            mappings,
            ui: Default::default(),
        },
    };
    (
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
async fn load(service: &ComparisonService, request: LoadRequest) -> ComparisonSession {
    tokio::time::timeout(
        Duration::from_secs(30),
        service
            .load(
                request,
                OperationId {
                    project: 1,
                    operation: 2,
                },
            )
            .operation
            .task,
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap()
}
fn rows(session: &ComparisonSession) -> serde_json::Value {
    serde_json::Value::Array(session.entries.iter().map(|e| {
        let result=e.result.as_ref();
        serde_json::json!({"local":e.local.strip_prefix(session.request.location.root.base()).unwrap().to_string_lossy(),"remote":Path::new(&e.remote).strip_prefix(&session.request.connection.root).unwrap().to_string_lossy(),"status":if e.error.is_some(){"Error"}else if result.unwrap().local.is_none(){"Remote only"}else if result.unwrap().remote.is_none(){"Local only"}else{"Changed"},"decision":result.map_or(Decision::Skip,|r|r.suggestion()).label(),"binary":result.is_some_and(|r|r.binary)})
    }).collect())
}
fn go_rows(request: &LoadRequest) -> serde_json::Value {
    static PROBE: OnceLock<PathBuf> = OnceLock::new();
    let binary = PROBE.get_or_init(|| {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("go.mod").is_file())
            .unwrap()
            .to_path_buf();
        let dir = tempfile::tempdir().unwrap().keep();
        let binary = dir.join("comparison-probe");
        let output = Command::new("go")
            .args(["build", "-o"])
            .arg(&binary)
            .arg("./testdata/comparison-probe")
            .env("GOCACHE", dir.join("cache"))
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
    let mut child = support::Process(
        Command::new(binary)
            .arg(request.location.root.base())
            .arg(&request.connection.root)
            .arg(request.host.port.to_string())
            .env("HOME", home.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let local: Vec<_> = if request.local.is_empty() && request.remote.is_empty() {
        vec![request.location.root.base().to_path_buf()]
    } else {
        request
            .local
            .iter()
            .map(|p| {
                if p.is_absolute() {
                    p.clone()
                } else {
                    request.location.root.base().join(p)
                }
            })
            .collect()
    };
    let mappings = if request.host.mappings.is_empty() {
        &request.location.config.mappings
    } else {
        &request.host.mappings
    };
    let input = serde_json::json!({"local":local,"remote":request.remote,"mappings":mappings,"include_ignored":request.scope.include_ignored});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let status = child.wait().unwrap();
    let mut stdout = String::new();
    let mut stderr = String::new();
    use std::io::Read;
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "{stderr}");
    serde_json::from_str(&stdout).unwrap()
}
#[tokio::test]
async fn real_sftp_comparison_matches_go_for_text_binary_large_missing_and_timestamps() {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    for (name, l, r, lt, rt) in [
        ("same.txt", Some("same"), Some("same"), 100, 101),
        ("metadata.txt", Some("local"), Some("other"), 100, 100), // documented fast path
        ("changed.txt", Some("local\n"), Some("remote\n"), 100, 110),
        ("close.txt", Some("local"), Some("other"), 100, 102),
        ("local.txt", Some("local"), None, 100, 100),
        ("remote.txt", None, Some("remote"), 100, 100),
        ("binary", Some("a\0b"), Some("c\0d"), 100, 101),
        ("empty", Some(""), None, 100, 100),
        ("newline", Some("same\n"), Some("same"), 100, 101),
        ("invalid-utf8", None, None, 100, 101),
    ] {
        if let Some(text) = l {
            write(&local.path().join(name), text.as_bytes(), lt);
        }
        if let Some(text) = r {
            write(&remote.join(name), text.as_bytes(), rt);
        }
    }
    write(&local.path().join("invalid-utf8"), b"\xff\n", 100);
    write(&remote.join("invalid-utf8"), b"\xfe\n", 101);
    let large = vec![b'x'; 2 * 1024 * 1024 + 100];
    for name in ["large-same", "large-changed"] {
        write(&local.path().join(name), &large, 100);
        write(&remote.join(name), &large, 101);
    }
    let mut changed = large.clone();
    changed[2 * 1024 * 1024] = b'y';
    write(&remote.join("large-changed"), &changed, 101);
    let (service, request) = request(&server, local.path(), vec![]).await;
    let expected = go_rows(&request);
    let session = load(&service, request).await;
    assert_eq!(rows(&session), expected);
    assert_eq!(session.scope.pairs, 12);
    assert!(
        !session.entries.iter().any(|e| e.local.ends_with("same.txt")
            || e.local.ends_with("large-same")
            || e.local.ends_with("metadata.txt"))
    );
    assert!(
        session
            .entries
            .iter()
            .any(|e| e.local.ends_with("large-changed") && e.result.as_ref().unwrap().content_diff)
    );
    session.shutdown().await;
}
#[tokio::test]
async fn mappings_ignore_exceptions_and_remote_selections_preserve_scope() {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    fs::create_dir_all(local.path().join("src")).unwrap();
    fs::create_dir_all(remote.join("deploy")).unwrap();
    write(
        &local.path().join(".gitignore"),
        b"src/ignored*\nsrc/cache/\n",
        100,
    );
    write(&local.path().join("outside"), b"unmapped", 100);
    for (name, l, r) in [
        ("local.txt", true, false),
        ("remote.txt", false, true),
        ("ignored-local", true, false),
        ("ignored-remote", false, true),
        ("cache/local", true, false),
        ("cache/remote", false, true),
        (".hidden", true, true),
    ] {
        if l {
            write(&local.path().join("src").join(name), b"local", 100);
        }
        if r {
            write(&remote.join("deploy").join(name), b"remote", 110);
        }
    }
    write(&remote.join("deploy/.git/config"), b"excluded", 100);
    write(
        &remote.join("deploy/.x.drift-tmp-0123456789abcdef0123456789abcdef"),
        b"excluded",
        100,
    );
    let (service, mut request) = request(
        &server,
        local.path(),
        vec![Mapping {
            local: "src".into(),
            remote: "deploy".into(),
        }],
    )
    .await;
    let session = load(&service, request.clone()).await;
    assert_eq!(session.scope.pairs, 3);
    assert_eq!(session.scope.hidden, 1);
    assert!(
        session
            .entries
            .iter()
            .all(|e| e.local.starts_with(local.path().join("src")))
    );
    request.local = vec![PathBuf::from("src")];
    assert_eq!(
        rows(&load(&service, request.clone()).await),
        go_rows(&request)
    );
    request.local.push(PathBuf::from("src/ignored-local"));
    let direct = load(&service, request.clone()).await;
    assert_eq!(direct.scope.explicit_ignored, 1);
    assert_eq!(rows(&direct), go_rows(&request));
    request.local.clear();
    request.remote = vec![remote.join("deploy").to_string_lossy().into_owned()];
    assert_eq!(
        rows(&load(&service, request.clone()).await),
        go_rows(&request)
    );
    request.scope.include_ignored = true;
    let included = load(&service, request.clone()).await;
    assert_eq!(included.scope.pairs, 7);
    assert_eq!(rows(&included), go_rows(&request));
    request.connection.shutdown().await;
}
#[tokio::test]
async fn missing_mapped_local_root_errors_and_cancelled_io_do_not_become_actions() {
    let mut server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(
        &server.dir.path().join("files/deploy/remote.txt"),
        b"remote",
        110,
    );
    let (service, mut request) = request(
        &server,
        local.path(),
        vec![Mapping {
            local: "missing".into(),
            remote: "deploy".into(),
        }],
    )
    .await;
    let session = load(&service, request.clone()).await;
    assert_eq!(session.entries.len(), 1);
    assert_eq!(
        session.entries[0].result.as_ref().unwrap().suggestion(),
        Decision::Download
    );
    write(&outside.path().join("outside"), b"secret", 100);
    fs::create_dir_all(local.path().join("missing")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("outside"),
        local.path().join("missing/remote.txt"),
    )
    .unwrap();
    let session = load(&service, request.clone()).await;
    assert!(session.entries[0].error.is_some());
    assert!(session.entries[0].result.is_none());
    let cancelled = service.load(
        request.clone(),
        OperationId {
            project: 1,
            operation: 3,
        },
    );
    cancelled.operation.cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), cancelled.operation.task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(request.connection.state(), ConnectionState::Closed);
    server.stop();
    request.local = vec![PathBuf::from("missing/remote.txt")];
    assert!(
        service
            .load(
                request,
                OperationId {
                    project: 1,
                    operation: 4
                }
            )
            .operation
            .task
            .await
            .unwrap()
            .is_err()
    );
}
