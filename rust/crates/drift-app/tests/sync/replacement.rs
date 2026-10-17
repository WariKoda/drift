use super::*;

fn restrictive(server: &support::Server, posix: bool) {
    fs::write(server.dir.path().join("one-channel"), "limited").unwrap();
    fs::write(
        server.dir.path().join("restricted-rename"),
        "refuse existing destinations",
    )
    .unwrap();
    if !posix {
        fs::write(server.dir.path().join("no-posix"), "unsupported").unwrap();
    }
}

#[tokio::test]
async fn single_channel_posix_replacement_preserves_permissions_session_and_refresh() {
    let server = support::Server::new(false);
    restrictive(&server, true);
    let local = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    let content = "Japanese に🦀e\u{301} and spaces  \n".repeat(8192);
    write(&local.path().join("a-replace"), content.as_bytes(), 100);
    write(&remote.join("a-replace"), b"old remote", 110);
    fs::set_permissions(remote.join("a-replace"), fs::Permissions::from_mode(0o640)).unwrap();
    write(&local.path().join("b-new"), b"new file", 100);
    let (sync, comparison, request) = setup(&server, local.path()).await;
    let session = compare(&comparison, request.clone()).await;
    let report = run(&sync, &session, &[Decision::Upload; 2]).await;
    assert!(report.stopped.is_none());
    assert_eq!(report.outcomes, vec![ItemOutcome::Completed; 2]);
    assert_eq!(
        fs::read(remote.join("a-replace")).unwrap(),
        content.as_bytes()
    );
    assert_eq!(
        fs::metadata(remote.join("a-replace"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_no_staging(&remote);
    assert_eq!(request.connection.state(), ConnectionState::Connected);
    let refreshed = compare(&comparison, request.clone()).await;
    assert!(refreshed.entries.iter().all(|entry| {
        entry.error.is_none()
            && entry
                .result
                .as_ref()
                .is_some_and(|result| !result.differs())
    }));
    request.connection.shutdown().await;
}

#[tokio::test]
async fn genuinely_unavailable_atomic_replacement_fails_without_moving_the_old_target() {
    let server = support::Server::new(false);
    restrictive(&server, false);
    let local = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    write(&local.path().join("a-replace"), b"new local", 100);
    write(&remote.join("a-replace"), b"old remote", 110);
    fs::set_permissions(remote.join("a-replace"), fs::Permissions::from_mode(0o640)).unwrap();
    write(&local.path().join("b-new"), b"new file", 100);
    let (sync, comparison, request) = setup(&server, local.path()).await;
    let session = compare(&comparison, request.clone()).await;
    let report = run(&sync, &session, &[Decision::Upload; 2]).await;
    assert!(report.stopped.is_none());
    assert!(matches!(report.outcomes[0], ItemOutcome::Failed(_)));
    assert_eq!(report.outcomes[1], ItemOutcome::Completed);
    assert_eq!(fs::read(remote.join("a-replace")).unwrap(), b"old remote");
    assert_eq!(
        fs::metadata(remote.join("a-replace"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(fs::read(remote.join("b-new")).unwrap(), b"new file");
    assert_no_staging(&remote);
    assert_eq!(request.connection.state(), ConnectionState::Connected);
    request.connection.shutdown().await;
}
