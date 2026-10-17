use super::*;

#[tokio::test]
async fn lost_rename_ack_preserves_unknown_report_and_committed_bytes_without_retry() {
    for posix in [true, false] {
        let server = support::Server::new(false);
        fs::write(server.dir.path().join("one-channel"), "limited").unwrap();
        if posix {
            fs::write(server.dir.path().join("restricted-rename"), "restricted").unwrap();
        } else {
            fs::write(server.dir.path().join("no-posix"), "unsupported").unwrap();
        }
        let local = tempfile::tempdir().unwrap();
        let remote = server.dir.path().join("files");
        write(&local.path().join("a-replace"), b"new local", 100);
        write(&remote.join("a-replace"), b"old remote", 110);
        write(&local.path().join("b-later"), b"must not transfer", 100);
        let (sync, comparison, request) = setup(&server, local.path()).await;
        let session = compare(&comparison, request.clone()).await;
        fs::write(
            server.dir.path().join(if posix {
                "arm-rename-close-posix"
            } else {
                "arm-rename-close-standard"
            }),
            "armed",
        )
        .unwrap();
        let report = run(&sync, &session, &[Decision::Upload; 2]).await;
        assert!(matches!(
            report.stopped,
            Some(StopReason::ConnectionLost(_))
        ));
        assert!(matches!(report.outcomes[0], ItemOutcome::Unknown(_)));
        assert_eq!(report.outcomes[1], ItemOutcome::Pending);
        assert_eq!(fs::read(remote.join("a-replace")).unwrap(), b"new local");
        assert!(!remote.join("b-later").exists());
        assert_ne!(request.connection.state(), ConnectionState::Connected);
        assert!(sync.run(&session, &[Decision::Upload; 2], id(4)).is_err());
        let log = fs::read_to_string(server.dir.path().join("sftp-operations")).unwrap();
        let operation = if posix { "posix" } else { "standard" };
        assert_eq!(
            log.lines()
                .filter(|line| line.split('\t').next() == Some(operation))
                .count(),
            1
        );
    }
}
