use super::*;

#[gpui_kit::test]
async fn lost_primary_posix_ack_keeps_unknown_sync_report_and_never_uploads_later_file(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    for marker in ["one-channel", "restricted-rename"] {
        fs::write(server.dir.path().join(marker), "restricted").unwrap();
    }
    let local = tempfile::tempdir().unwrap();
    fs::write(local.path().join("a-replace"), "new local data").unwrap();
    fs::write(local.path().join("b-later"), "must not transfer").unwrap();
    let remote = server.dir.path().join("files");
    fs::write(remote.join("a-replace"), "old remote data").unwrap();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_comparison(&store, local.path(), &server, cx).await;
    fs::write(server.dir.path().join("arm-rename-close-posix"), "armed").unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("ctrl-f", cx);
        w.press("enter", cx);
        w.press("home", cx);
        w.press("u", cx);
        assert!(w.find("sync-confirm").visible());
        w.click("sync-confirm", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
            && !shell.read(cx).browser.read(cx).is_loading()
            && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    shell.read_with(cx, |s, cx| {
        let report = s.comparison.read(cx).sync_result_for_test().unwrap();
        assert!(matches!(
            report.stopped,
            Some(StopReason::ConnectionLost(_))
        ));
        assert!(matches!(report.outcomes[0], ItemOutcome::Unknown(_)));
        assert_eq!(report.outcomes[1], ItemOutcome::Pending);
    });
    assert_eq!(
        fs::read_to_string(remote.join("a-replace")).unwrap(),
        "new local data"
    );
    assert!(!remote.join("b-later").exists());
    cx.update_window(handle, |_, w, cx| {
        w.press("ctrl-enter", cx);
        w.press("f1", cx);
        w.press("ctrl-enter", cx);
        w.press("escape", cx);
    })
    .unwrap();
    let log = fs::read_to_string(server.dir.path().join("sftp-operations")).unwrap();
    assert_eq!(
        log.lines()
            .filter(|line| line.split('\t').next() == Some("posix"))
            .count(),
        1
    );
    assert!(!remote.join("b-later").exists());
    shell.read_with(cx, |s, cx| {
        assert!(matches!(
            s.comparison
                .read(cx)
                .sync_result_for_test()
                .unwrap()
                .outcomes[0],
            ItemOutcome::Unknown(_)
        ))
    });
}
