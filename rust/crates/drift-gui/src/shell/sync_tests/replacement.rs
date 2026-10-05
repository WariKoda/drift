use super::*;

#[gpui_kit::test]
async fn native_confirm_uses_single_channel_atomic_replace_and_keeps_refreshed_comparison(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    fs::write(server.dir.path().join("one-channel"), "limited").unwrap();
    fs::write(
        server.dir.path().join("restricted-rename"),
        "refuse existing destinations",
    )
    .unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::write(local.path().join("data.txt"), "new に🦀e\u{301} data  \n").unwrap();
    let target = server.dir.path().join("files/data.txt");
    fs::write(&target, "old remote data").unwrap();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_comparison(&store, local.path(), &server, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.press("ctrl-f", cx);
        w.press("enter", cx);
        w.press("home", cx);
        w.press("u", cx);
        assert!(w.find("sync-confirm").visible());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old remote data");
        w.press("f1", cx);
        w.press("ctrl-enter", cx);
        assert!(!shell.read(cx).comparison.read(cx).is_loading());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old remote data");
        w.press("escape", cx);
        w.click("sync-confirm", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
            && !shell.read(cx).browser.read(cx).is_loading()
            && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "new に🦀e\u{301} data  \n"
    );
    shell.read_with(cx, |s, cx| {
        assert_eq!(
            s.comparison
                .read(cx)
                .sync_result_for_test()
                .unwrap()
                .outcomes[0],
            ItemOutcome::Completed
        );
        assert!(s.remote.read(cx).has_session());
    });
    assert!(
        fs::read_dir(server.dir.path().join("files"))
            .unwrap()
            .all(|entry| !drift_core::staging::is_staging_name(
                &entry.unwrap().file_name().to_string_lossy()
            ))
    );
}

#[gpui_kit::test]
async fn native_confirm_reports_missing_server_atomic_support_without_removing_old_target(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    for marker in ["one-channel", "restricted-rename", "no-posix"] {
        fs::write(server.dir.path().join(marker), "restricted").unwrap();
    }
    let local = tempfile::tempdir().unwrap();
    fs::write(local.path().join("data.txt"), "new local data").unwrap();
    let target = server.dir.path().join("files/data.txt");
    fs::write(&target, "old remote data").unwrap();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_comparison(&store, local.path(), &server, cx).await;
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
    assert_eq!(fs::read_to_string(&target).unwrap(), "old remote data");
    shell.read_with(cx, |s, cx| {
        assert!(matches!(
            s.comparison
                .read(cx)
                .sync_result_for_test()
                .unwrap()
                .outcomes[0],
            ItemOutcome::Failed(_)
        ));
        assert!(s.remote.read(cx).has_session());
    });
    assert!(
        fs::read_dir(server.dir.path().join("files"))
            .unwrap()
            .all(|entry| !drift_core::staging::is_staging_name(
                &entry.unwrap().file_name().to_string_lossy()
            ))
    );
}
