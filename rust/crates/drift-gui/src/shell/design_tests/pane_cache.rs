use super::*;
use crate::pane_tabs::PaneView;

#[gpui_kit::test]
async fn independent_preview_caches_survive_filters_copy_their_own_path_and_refresh(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let local_file = local.path().join("a.txt");
    let remote_file = server.dir.path().join("files/a.txt");
    fs::write(&local_file, "local v1").unwrap();
    fs::write(&remote_file, "remote v1").unwrap();
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
        crate::test_support::connect_first(w, cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).remote.read(cx).has_session()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("p", cx);
        f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("p", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        let s = f.shell.read(cx);
        s.local_view == PaneView::Preview
            && s.remote_view == PaneView::Preview
            && !s.local_preview.read(cx).is_loading()
            && !s.remote_preview.read(cx).is_loading()
    })
    .await;
    let (local_id, remote_id, local_path, remote_path, session) = f.shell.read_with(cx, |s, cx| {
        (
            s.local_preview.read(cx).id(),
            s.remote_preview.read(cx).id(),
            s.browser.read(cx).selected().unwrap().to_owned(),
            s.remote.read(cx).selected().unwrap().to_owned(),
            s.remote.read(cx).session_id(),
        )
    });
    cx.update_window(f.handle, |_, w, cx| {
        assert!(w.within("remote-workspace").find("disconnect").visible());
        w.click("local-files-tab", cx);
        w.click("remote-files-tab", cx);
        for filter in ["filter", "remote-filter"] {
            w.click(filter, cx);
            w.input("no-match", cx);
        }
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(f.shell.read(cx).browser.read(cx).selected(), None);
        assert_eq!(f.shell.read(cx).remote.read(cx).selected(), None);
        w.click("local-preview-tab", cx);
        w.click("remote-preview-tab", cx);
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        let s = f.shell.read(cx);
        assert_eq!(s.local_view, PaneView::Preview);
        assert_eq!(s.remote_view, PaneView::Preview);
        assert_eq!(s.local_preview.read(cx).id(), local_id);
        assert_eq!(s.remote_preview.read(cx).id(), remote_id);
        assert_eq!(s.remote.read(cx).session_id(), session);
        for (side, path, text) in [
            (PaneSide::Local, local_path.as_str(), "local v1"),
            (PaneSide::Remote, remote_path.as_str(), "remote v1"),
        ] {
            f.shell
                .read(cx)
                .preview_pane(side)
                .update(cx, |pane, cx| pane.focus(w, cx));
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("unchanged".into()));
            w.press("ctrl-shift-c", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(path)
            );
            w.press("c", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(text)
            );
        }
        assert!(w.within("remote-workspace").find("disconnect").visible());
        let local_preview = f.shell.read(cx).local_preview.clone();
        local_preview.update(cx, |pane, cx| pane.focus(w, cx));
        w.press("p", cx);
        assert_eq!(f.shell.read(cx).local_view, PaneView::Files);
        assert_eq!(f.shell.read(cx).remote_view, PaneView::Preview);
        w.click("remote-files-tab", cx);
        for filter in ["filter", "remote-filter"] {
            w.click(filter, cx);
            w.press("ctrl-a", cx);
            w.press("backspace", cx);
        }
    })
    .unwrap();
    fs::write(&local_file, "local v2").unwrap();
    fs::write(&remote_file, "remote v2").unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("f5", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(f.shell.read(cx).remote_preview.read(cx).id(), remote_id);
        f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.click("local-preview-tab", cx);
        w.click("remote-refresh", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).remote.read(cx).is_loading()
            && !f.shell.read(cx).local_preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.click("remote-preview-tab", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).remote_preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        for (scope, expected) in [
            ("local-workspace", "local v2"),
            ("remote-workspace", "remote v2"),
        ] {
            w.within(scope).click("copy-preview", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(expected)
            );
        }
        assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), session);
        assert!(!f.shell.read(cx).comparison.read(cx).visible());
    })
    .unwrap();
    let connection = f
        .shell
        .read_with(cx, |s, cx| s.remote.read(cx).comparison_target().unwrap().1);
    connection.shutdown().await;
}
