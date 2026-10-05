use super::*;

#[gpui_kit::test]
async fn malformed_remote_preview_terminates_connection_before_commands_behind_help(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    fs::write(server.dir.path().join("one-channel"), "limited").unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::write(local.path().join("source"), "local comparison data").unwrap();
    let target = server.dir.path().join("files/source");
    fs::write(&target, "remote preview data").unwrap();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_comparison(&store, local.path(), &server, cx).await;
    cx.update_window(handle, |_, w, cx| w.click("comparison-back", cx))
        .unwrap();
    fs::write(server.dir.path().join("arm-empty-read"), "armed").unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("remote-filter", cx);
        w.press("enter", cx);
        w.press("home", cx);
        w.press("enter", cx);
        w.press("f1", cx);
        assert!(shell.read(cx).help.is_some());
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).remote.read(cx).has_session()
            && !shell.read(cx).remote.read(cx).is_loading()
            && !shell.read(cx).preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert!(shell.read(cx).help.is_some());
        w.press("u", cx);
        w.press("ctrl-enter", cx);
        assert!(!shell.read(cx).comparison.read(cx).is_loading());
        assert!(shell.read(cx).help.is_some());
        w.press("escape", cx);
    })
    .unwrap();
    assert!(!shell.read_with(cx, |s, cx| s.remote.read(cx).has_session()));
    assert_eq!(fs::read_to_string(&target).unwrap(), "remote preview data");
    assert_eq!(
        fs::read_to_string(local.path().join("source")).unwrap(),
        "local comparison data"
    );
}
