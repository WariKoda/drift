use super::*;

#[gpui_kit::test]
async fn local_cancel_preserves_the_remote_preview_after_queued_selection_events(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("local.txt"), "local unchanged").unwrap();
    let remote_path = server.dir.path().join("files/remote.txt");
    fs::write(&remote_path, "remote unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let f = Fixture::new(
        1400.,
        1000.,
        store,
        local.path(),
        Some(server.options()),
        cx,
    )
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote-files-tab", cx);
        crate::test_support::connect_first(w, cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).remote.read(cx).has_session()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("p", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).remote_view == crate::pane_tabs::PaneView::Preview
            && !f.shell.read(cx).remote_preview.read(cx).is_loading()
    })
    .await;
    let (preview_id, session) = cx
        .update_window(f.handle, |_, w, cx| {
            let preview_id = f.shell.read(cx).remote_preview.read(cx).id();
            let session = f.shell.read(cx).remote.read(cx).session_id();
            let browser = f.shell.read(cx).browser.clone();
            browser.update(cx, |browser, cx| browser.refresh(&Refresh, w, cx));
            assert!(browser.read(cx).is_loading());
            w.click("cancel", cx);
            assert!(!browser.read(cx).is_loading());
            (preview_id, session)
        })
        .unwrap();
    // Local cancellation emits selection changes; check again after those effects run.
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        let shell = f.shell.read(cx);
        assert_eq!(shell.remote_view, crate::pane_tabs::PaneView::Preview);
        assert_eq!(shell.remote.read(cx).session_id(), session);
        assert_eq!(shell.remote_preview.read(cx).id(), preview_id);
        let preview = shell.remote_preview.clone();
        assert!(preview.update(cx, |preview, cx| preview.copy_text(cx)));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("remote unchanged")
        );
        assert!(
            w.within("remote-workspace")
                .find("preview-header")
                .visible()
        );
        let right = w.within("browser-split").find("split-second").bounds();
        let header = w.within("remote-workspace").find("preview-header").bounds();
        assert!(header.left() >= right.left() && header.right() <= right.right());
        let browser = f.shell.read(cx).browser.clone();
        browser.update(cx, |browser, cx| browser.refresh(&Refresh, w, cx));
        assert!(browser.read(cx).is_loading());
        w.press("ctrl-escape", cx);
        assert!(!browser.read(cx).is_loading());
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(
            f.shell.read(cx).remote_view,
            crate::pane_tabs::PaneView::Preview
        );
        assert_eq!(f.shell.read(cx).remote_preview.read(cx).id(), preview_id);
        assert!(
            w.within("remote-workspace")
                .find("preview-header")
                .visible()
        );
        assert!(w.find("browser-pane").visible());
        assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), session);
    })
    .unwrap();
    let connection = f.shell.read_with(cx, |shell, cx| {
        shell.remote.read(cx).comparison_target().unwrap().1
    });
    connection.shutdown().await;
    assert_eq!(
        fs::read_to_string(local.path().join("local.txt")).unwrap(),
        "local unchanged"
    );
    assert_eq!(fs::read_to_string(remote_path).unwrap(), "remote unchanged");
}
