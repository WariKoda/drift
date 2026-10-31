use super::*;

#[gpui_kit::test]
async fn marked_counts_stay_below_each_list_through_filter_scroll_and_resize(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir(local.path().join("child")).unwrap();
    fs::create_dir(server.dir.path().join("files/child")).unwrap();
    for index in 0..100 {
        let name = format!("file-{index:03}.txt");
        fs::write(local.path().join(&name), "local unchanged").unwrap();
        fs::write(
            server.dir.path().join("files").join(&name),
            "remote unchanged",
        )
        .unwrap();
    }
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let f = Fixture::new(1400., 900., store, local.path(), Some(server.options()), cx).await;
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
        assert_eq!(w.find("local-selection-status").label(), Some("0 marked"));
        assert_eq!(w.find("remote-selection-status").label(), Some("0 marked"));
        f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        for key in ["home", "space", "down", "space"] {
            w.press(key, cx);
        }
        f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        for key in ["home", "space", "down", "space"] {
            w.press(key, cx);
        }
        assert_eq!(f.shell.read(cx).browser.read(cx).marked().len(), 2);
        assert_eq!(f.shell.read(cx).remote.read(cx).marked().len(), 2);
        w.click("filter", cx);
        w.input("file-00", cx);
        w.click("remote-filter", cx);
        w.input("file-00", cx);
        for (width, height, font) in [(1400., 900., 16.), (900., 600., 16.), (1400., 1000., 24.)] {
            w.resize(size(px(width), px(height)));
            w.bounds_changed(cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            w.render_frame(cx);
            for (status, list, workspace) in [
                (
                    "local-selection-status",
                    "local-files-pane",
                    "local-workspace",
                ),
                (
                    "remote-selection-status",
                    "remote-files-pane",
                    "remote-workspace",
                ),
            ] {
                let footer = w.find(status);
                assert_eq!(footer.label(), Some("2 marked"));
                assert!(footer.visible());
                let bounds = footer.bounds();
                assert!(bounds.top() >= w.find(list).bounds().bottom(), "{status}");
                assert!(
                    bounds.bottom() <= w.find(workspace).bounds().bottom(),
                    "{status}"
                );
                w.scroll(
                    list,
                    gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-2000.))),
                    cx,
                );
                w.render_frame(cx);
                assert_eq!(w.find(status).bounds(), bounds);
                assert_eq!(w.find(status).label(), Some("2 marked"));
            }
        }
        f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("v", cx);
        f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("v", cx);
        assert_eq!(
            w.find("local-selection-status").label(),
            Some("2 marked · range active")
        );
        assert_eq!(
            w.find("remote-selection-status").label(),
            Some("2 marked · range active")
        );
        assert!(!f.shell.read(cx).comparison.read(cx).visible());
    })
    .unwrap();
    let session = f.shell.read_with(cx, |shell, cx| {
        shell.remote.read(cx).comparison_target().unwrap().1
    });
    session.shutdown().await;
    assert_eq!(
        fs::read_to_string(local.path().join("file-000.txt")).unwrap(),
        "local unchanged"
    );
    assert_eq!(
        fs::read_to_string(server.dir.path().join("files/file-000.txt")).unwrap(),
        "remote unchanged"
    );
}
