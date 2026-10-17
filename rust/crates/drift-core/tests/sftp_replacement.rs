//! Replacement policy exercised over real SSH/SFTP sockets and real files.
#[path = "sftp_replacement/source_read.rs"]
mod source_read;
mod support;

use drift_core::{
    error::{Error, Result},
    remote::{self, ConnectionState, RemoteClient},
    staging::is_staging_name,
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

const OLD: &[u8] = b"previous deployment\n";
const NEW: &[u8] = b"replacement deployment\n";
const MODE: u32 = 0o640;

fn fixture(markers: &[&str]) -> support::Server {
    let server = support::Server::new(false);
    for marker in markers.iter().copied().chain(["one-channel"]) {
        fs::write(server.dir.path().join(marker), b"armed\n").unwrap();
    }
    fs::write(server.dir.path().join("files/source"), NEW).unwrap();
    server
}

fn target(server: &support::Server, existing: bool) -> String {
    let path = server.dir.path().join("files/deployed");
    if existing {
        fs::write(&path, OLD).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(MODE)).unwrap();
    }
    path.to_str().unwrap().to_owned()
}

async fn connect(server: &support::Server) -> Arc<dyn RemoteClient> {
    remote::connect(server.host(), server.options(), CancellationToken::new())
        .await
        .unwrap()
}

async fn upload(server: &support::Server, client: &dyn RemoteClient, path: &str) -> Result<()> {
    let source = client
        .open(server.dir.path().join("files/source").to_str().unwrap())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), client.upload(path, source))
        .await
        .expect("real SFTP upload did not finish")
}

fn operations(server: &support::Server) -> String {
    fs::read_to_string(server.dir.path().join("sftp-operations")).unwrap()
}

fn count(log: &str, operation: &str) -> usize {
    log.lines()
        .filter(|line| line.split('\t').next() == Some(operation))
        .count()
}

fn single_channel(server: &support::Server) {
    assert_eq!(
        fs::read_to_string(server.dir.path().join("channels")).unwrap(),
        "accepted\n",
        "the primary channel must handle extensions without even attempting a second"
    );
}

fn contents(path: &str, bytes: &[u8], mode: Option<u32>) {
    assert_eq!(fs::read(path).unwrap(), bytes);
    if let Some(mode) = mode {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
}

fn no_stages(directory: &Path) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        assert!(
            !is_staging_name(entry.file_name().to_str().unwrap()),
            "staging file remained: {:?}",
            entry.path()
        );
        if entry.file_type().unwrap().is_dir() {
            no_stages(&entry.path());
        }
    }
}

fn never_delete_target(log: &str, path: &str) {
    assert!(
        !log.lines().any(|line| line == format!("remove\t{path}")),
        "replacement must not delete the old target: {log}"
    );
}

fn stage_used(log: &str, path: &str) {
    let target = Path::new(path);
    let base = target.file_name().unwrap().to_str().unwrap();
    let stages: Vec<_> = log
        .lines()
        .filter_map(|line| line.strip_prefix("open\t"))
        .filter(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_staging_name)
        })
        .collect();
    assert_eq!(stages.len(), 1, "exactly one real staged upload: {log}");
    let stage = Path::new(stages[0]);
    assert_eq!(stage.parent(), target.parent());
    assert!(
        stage
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(&format!(".{base}.drift-tmp-"))
    );
}

#[tokio::test]
async fn primary_posix_replaces_when_standard_rename_refuses_existing_target() {
    let server = fixture(&["restricted-rename"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    upload(&server, client.as_ref(), &path).await.unwrap();
    contents(&path, NEW, Some(MODE));
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert_eq!(count(&log, "posix"), 1);
    assert_eq!(count(&log, "standard"), 0);
    assert!(log.contains("rename-status\tposix\t0\n"));
    stage_used(&log, &path);
    never_delete_target(&log, &path);
    single_channel(&server);
    client.shutdown().await;
}

#[tokio::test]
async fn new_target_uses_standard_rename_without_advertised_posix() {
    let server = fixture(&["restricted-rename", "no-posix"]);
    let path = target(&server, false);
    let client = connect(&server).await;
    upload(&server, client.as_ref(), &path).await.unwrap();
    contents(&path, NEW, None);
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert_eq!(count(&log, "posix"), 0);
    assert_eq!(count(&log, "standard"), 1);
    assert!(log.contains("rename-status\tstandard\t0\n"));
    stage_used(&log, &path);
    single_channel(&server);
    client.shutdown().await;
}

#[tokio::test]
async fn missing_posix_and_no_replace_standard_preserve_target_and_clean_stage() {
    let server = fixture(&["restricted-rename", "no-posix"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    let error = upload(&server, client.as_ref(), &path).await.unwrap_err();
    assert!(!matches!(error, Error::Connection(_)), "{error}");
    contents(&path, OLD, Some(MODE));
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert_eq!(count(&log, "posix"), 0);
    assert_eq!(count(&log, "standard"), 1);
    stage_used(&log, &path);
    assert_eq!(count(&log, "remove"), 1, "only the stage is cleaned");
    never_delete_target(&log, &path);
    single_channel(&server);
    assert_eq!(*client.state().borrow(), ConnectionState::Connected);
    client.shutdown().await;
}

#[tokio::test]
async fn explicit_posix_unsupported_falls_back_to_permitted_standard_replace() {
    // No restricted-rename: this filesystem deliberately permits replacement by
    // standard Rename while the advertised POSIX operation is disabled by policy.
    let server = fixture(&["posix-unsupported"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    upload(&server, client.as_ref(), &path).await.unwrap();
    contents(&path, NEW, Some(MODE));
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert_eq!(count(&log, "posix"), 1);
    assert_eq!(count(&log, "standard"), 1);
    assert!(log.contains("rename-status\tposix\t8\n"));
    assert!(log.contains("rename-status\tstandard\t0\n"));
    stage_used(&log, &path);
    never_delete_target(&log, &path);
    single_channel(&server);
    client.shutdown().await;
}

#[tokio::test]
async fn explicit_posix_unsupported_cannot_bypass_standard_no_replace_policy() {
    let server = fixture(&["restricted-rename", "posix-unsupported"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    assert!(upload(&server, client.as_ref(), &path).await.is_err());
    contents(&path, OLD, Some(MODE));
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert_eq!(count(&log, "posix"), 1);
    assert_eq!(count(&log, "standard"), 1);
    assert!(log.contains("rename-status\tposix\t8\n"));
    assert_eq!(count(&log, "remove"), 1);
    never_delete_target(&log, &path);
    single_channel(&server);
    client.shutdown().await;
}

#[tokio::test]
async fn posix_permission_denial_never_falls_back_to_standard_rename() {
    let server = fixture(&["restricted-rename", "deny-posix-rename"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    let error = upload(&server, client.as_ref(), &path).await.unwrap_err();
    assert!(
        matches!(error, Error::Io(ref e) if e.kind() == std::io::ErrorKind::PermissionDenied),
        "{error}"
    );
    contents(&path, OLD, Some(MODE));
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert_eq!(count(&log, "posix"), 1);
    assert_eq!(count(&log, "standard"), 0);
    assert!(log.contains("rename-status\tposix\t3\n"));
    assert_eq!(count(&log, "remove"), 1);
    never_delete_target(&log, &path);
    single_channel(&server);
    client.shutdown().await;
}

#[tokio::test]
async fn posix_ack_loss_after_commit_is_connection_error_not_standard_retry() {
    let server = fixture(&["restricted-rename", "arm-rename-close-posix"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    let error = upload(&server, client.as_ref(), &path).await.unwrap_err();
    assert!(matches!(error, Error::Connection(_)), "{error}");
    client.shutdown().await;
    // These are direct reads from the daemon's real filesystem, not client cache.
    contents(&path, NEW, Some(MODE));
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert!(log.contains("ack-dropped\tposix\n"));
    assert_eq!(count(&log, "posix"), 1);
    assert_eq!(count(&log, "standard"), 0);
    stage_used(&log, &path);
    never_delete_target(&log, &path);
    single_channel(&server);
}

#[tokio::test]
async fn standard_ack_loss_after_commit_is_connection_error_without_retry() {
    let server = fixture(&["no-posix", "arm-rename-close-standard"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    let error = upload(&server, client.as_ref(), &path).await.unwrap_err();
    assert!(matches!(error, Error::Connection(_)), "{error}");
    client.shutdown().await;
    contents(&path, NEW, Some(MODE));
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert!(log.contains("ack-dropped\tstandard\n"));
    assert_eq!(count(&log, "posix"), 0);
    assert_eq!(count(&log, "standard"), 1);
    stage_used(&log, &path);
    never_delete_target(&log, &path);
    single_channel(&server);
}

#[tokio::test]
async fn connection_loss_before_posix_commit_preserves_old_target_without_retry() {
    let server = fixture(&["restricted-rename", "arm-rename-before-posix"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    let error = upload(&server, client.as_ref(), &path).await.unwrap_err();
    assert!(matches!(error, Error::Connection(_)), "{error}");
    client.shutdown().await;
    contents(&path, OLD, Some(MODE));
    let log = operations(&server);
    assert!(log.contains("before-commit\tposix\n"));
    assert!(!log.contains("rename-status\tposix\t0\n"));
    assert_eq!(count(&log, "posix"), 1);
    assert_eq!(count(&log, "standard"), 0);
    // A socket loss can leave an interrupted stage; its hard-excluded name is
    // still required. Cleanup cannot be acknowledged on the dead transport.
    stage_used(&log, &path);
    never_delete_target(&log, &path);
    single_channel(&server);
}

#[tokio::test]
async fn directory_and_symlink_targets_are_rejected_before_any_rename() {
    let server = fixture(&["restricted-rename"]);
    let root = server.dir.path().join("files");
    fs::create_dir(root.join("directory")).unwrap();
    fs::write(root.join("directory/keep"), OLD).unwrap();
    fs::write(root.join("referent"), OLD).unwrap();
    fs::set_permissions(root.join("referent"), fs::Permissions::from_mode(MODE)).unwrap();
    symlink("referent", root.join("symlink")).unwrap();
    let client = connect(&server).await;
    for name in ["directory", "symlink"] {
        let path = root.join(name);
        let error = upload(&server, client.as_ref(), path.to_str().unwrap())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Invalid(_)), "{error}");
    }
    assert_eq!(fs::read(root.join("directory/keep")).unwrap(), OLD);
    assert_eq!(
        fs::read_link(root.join("symlink")).unwrap(),
        Path::new("referent")
    );
    contents(root.join("referent").to_str().unwrap(), OLD, Some(MODE));
    no_stages(&root);
    let log = operations(&server);
    assert_eq!(count(&log, "posix"), 0);
    assert_eq!(count(&log, "standard"), 0);
    assert_eq!(count(&log, "remove"), 0);
    single_channel(&server);
    client.shutdown().await;
}

#[tokio::test]
async fn real_source_close_socket_failure_cleans_destination_stage_before_commit() {
    let source_server = fixture(&[]);
    let source_client = connect(&source_server).await;
    let source = source_client
        .open(
            source_server
                .dir
                .path()
                .join("files/source")
                .to_str()
                .unwrap(),
        )
        .await
        .unwrap();
    // Only this independent source connection loses its socket. The destination
    // remains reachable to clean the real stage after copy reaches source EOF.
    fs::write(source_server.dir.path().join("drop-on-close"), b"armed\n").unwrap();

    let server = fixture(&["restricted-rename"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    let error = tokio::time::timeout(Duration::from_secs(10), client.upload(&path, source))
        .await
        .expect("source CLOSE socket failure did not finish")
        .unwrap_err();
    assert!(matches!(error, Error::Connection(_)), "{error}");
    contents(&path, OLD, Some(MODE));
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert_eq!(count(&log, "posix"), 0);
    assert_eq!(count(&log, "standard"), 0);
    assert_eq!(count(&log, "remove"), 1);
    stage_used(&log, &path);
    never_delete_target(&log, &path);
    assert!(
        count(&log, "write") > 0,
        "real source bytes must reach the stage"
    );
    assert!(source_server.dir.path().join("close-dropped").is_file());
    single_channel(&server);
    single_channel(&source_server);
    client.shutdown().await;
    source_client.shutdown().await;
}

#[tokio::test]
async fn multi_chunk_native_stream_short_final_read_eof_and_preview_threshold() {
    let server = fixture(&["restricted-rename"]);
    let root = server.dir.path().join("files");
    // More than a preview and multiple 32 KiB chunks, with a short final chunk.
    let bytes: Vec<u8> = (0..1024 * 1024 + 137).map(|i| (i % 251) as u8).collect();
    fs::write(root.join("source"), &bytes).unwrap();
    let path = target(&server, true);
    let client = connect(&server).await;
    upload(&server, client.as_ref(), &path).await.unwrap();
    contents(&path, &bytes, Some(MODE));
    no_stages(&root);
    assert_eq!(
        client.read_limited(&path, bytes.len()).await.unwrap(),
        bytes
    );
    assert!(client.read_limited(&path, bytes.len() - 1).await.is_err());
    assert!(client.read_limited(&path, 1024 * 1024).await.is_err());
    let mut source = client.open(&path).await.unwrap();
    let mut streamed = Vec::new();
    let mut chunk = [0u8; 5003];
    loop {
        let read = source.read(&mut chunk).await.unwrap();
        if read == 0 {
            break;
        }
        streamed.extend_from_slice(&chunk[..read]);
    }
    assert_eq!(source.read(&mut chunk).await.unwrap(), 0);
    source.close().await.unwrap();
    assert_eq!(streamed, bytes);
    fs::write(root.join("empty"), b"").unwrap();
    assert!(
        client
            .read_limited(root.join("empty").to_str().unwrap(), 0)
            .await
            .unwrap()
            .is_empty()
    );
    let log = operations(&server);
    assert!(
        count(&log, "read") > 32,
        "must exercise multiple real READs"
    );
    assert!(
        count(&log, "write") > 32,
        "must exercise multiple real WRITEs"
    );
    assert_eq!(count(&log, "posix"), 1);
    assert_eq!(count(&log, "standard"), 0);
    stage_used(&log, &path);
    never_delete_target(&log, &path);
    single_channel(&server);
    client.shutdown().await;
}
