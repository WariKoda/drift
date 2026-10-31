use super::*;

#[gpui_kit::test]
async fn hoisted_native_filters_keep_independent_editing_focus_and_values_across_preview(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote_file = server.dir.path().join("files/keep-remote.txt");
    fs::write(&remote_file, "remote unchanged").unwrap();
    fs::write(server.dir.path().join("files/other.txt"), "other remote").unwrap();
    fs::write(local.path().join("keep-local.txt"), "local unchanged").unwrap();
    fs::write(local.path().join("other.txt"), "other local").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let f = Fixture::new(
        1400.,
        1100.,
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
    let local_focus = cx
        .update_window(f.handle, |_, w, cx| {
            w.click("filter", cx);
            w.input("local", cx);
            assert_eq!(w.find("filter").value(), Some("local"));
            w.focused(cx).unwrap()
        })
        .unwrap();
    let remote_focus = cx
        .update_window(f.handle, |_, w, cx| {
            assert_eq!(f.shell.read(cx).browser.read(cx).len(), 1);
            assert_eq!(f.shell.read(cx).remote_view, PaneView::Files);
            w.click("remote-filter", cx);
            assert_eq!(w.find("remote-filter").focused(), Some(true));
            w.input("missing", cx);
            assert_eq!(w.find("remote-filter").value(), Some("missing"));
            w.focused(cx).unwrap()
        })
        .unwrap();
    assert_ne!(
        local_focus, remote_focus,
        "filters must keep separate native input entities"
    );
    cx.update_window(f.handle, |_, w, cx| {
        assert!(f.shell.read(cx).remote.read(cx).selected().is_none());
        assert_eq!(w.find("filter").value(), Some("local"));
        w.press("ctrl-a", cx);
        w.input("remote", cx);
        assert_eq!(w.find("remote-filter").value(), Some("remote"));
    })
    .unwrap();
    let (browser_id, session, connection) = cx
        .update_window(f.handle, |_, w, cx| {
            let remote = f.shell.read(cx).remote.clone();
            remote.focus_handle(cx).focus(w, cx);
            w.press("home", cx);
            assert_eq!(f.shell.read(cx).browser.read(cx).len(), 1);
            assert_eq!(
                remote.read(cx).selected(),
                Some(remote_file.to_str().unwrap())
            );
            (
                f.shell.read(cx).browser.read(cx).id(),
                remote.read(cx).session_id(),
                remote.read(cx).connection(),
            )
        })
        .unwrap();
    for (local_preview, remote_preview) in
        [(true, false), (false, true), (true, true), (false, false)]
    {
        cx.update_window(f.handle, |_, w, cx| {
            if local_preview && f.shell.read(cx).local_view == PaneView::Files {
                w.click("local-files-tab", cx);
                w.press("home", cx);
            }
            w.click(
                if local_preview {
                    "local-preview-tab"
                } else {
                    "local-files-tab"
                },
                cx,
            );
            w.click(
                if remote_preview {
                    "remote-preview-tab"
                } else {
                    "remote-files-tab"
                },
                cx,
            );
        })
        .unwrap();
        cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
            f.shell.read(cx).local_view
                == if local_preview {
                    PaneView::Preview
                } else {
                    PaneView::Files
                }
                && f.shell.read(cx).remote_view
                    == if remote_preview {
                        PaneView::Preview
                    } else {
                        PaneView::Files
                    }
                && !f.shell.read(cx).local_preview.read(cx).is_loading()
                && !f.shell.read(cx).remote_preview.read(cx).is_loading()
        })
        .await;
        cx.update_window(f.handle, |_, w, cx| {
            assert_eq!(
                f.shell.read(cx).local_view,
                if local_preview {
                    PaneView::Preview
                } else {
                    PaneView::Files
                }
            );
            assert_eq!(
                f.shell.read(cx).remote_view,
                if remote_preview {
                    PaneView::Preview
                } else {
                    PaneView::Files
                }
            );
            for (width, font) in [(1400., 24.), (420., 24.), (1400., 16.)] {
                w.resize(size(px(width), px(1600.)));
                w.bounds_changed(cx);
                Theme::update(cx, |theme| theme.font_size = px(font));
                crate::test_support::settle(w, cx);
                assert_eq!(w.find("filter").value(), Some("local"));
                assert_eq!(w.find("remote-filter").value(), Some("remote"));
                for (id, focus, disabled) in [
                    ("filter", &local_focus, local_preview),
                    ("remote-filter", &remote_focus, remote_preview),
                ] {
                    w.click(id, cx);
                    if disabled {
                        assert!(
                            !focus.is_focused(w),
                            "preview filter must remain disabled: {id}"
                        );
                    } else {
                        assert_eq!(w.focused(cx), Some(focus.clone()));
                    }
                }
                let shell = f.shell.read(cx);
                assert_eq!(shell.browser.read(cx).id(), browser_id);
                assert_eq!(shell.remote.read(cx).session_id(), session);
                assert_eq!(shell.remote.read(cx).connection(), connection);
            }
        })
        .unwrap();
    }
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote-filter", cx);
        w.press("ctrl-a", cx);
        w.input("keep-remote", cx);
        assert_eq!(w.find("filter").value(), Some("local"));
        w.click("filter", cx);
        w.press("ctrl-a", cx);
        w.input("keep-local", cx);
        assert_eq!(w.find("remote-filter").value(), Some("keep-remote"));
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(w.find("filter").value(), Some("keep-local"));
        assert_eq!(w.find("remote-filter").value(), Some("keep-remote"));
        assert_eq!(f.shell.read(cx).browser.read(cx).len(), 1);
        assert_eq!(
            f.shell.read(cx).remote.read(cx).selected(),
            Some(remote_file.to_str().unwrap())
        );
    })
    .unwrap();
    let connection = f.shell.read_with(cx, |shell, cx| {
        shell.remote.read(cx).comparison_target().unwrap().1
    });
    connection.shutdown().await;
    assert_eq!(fs::read_to_string(remote_file).unwrap(), "remote unchanged");
    assert_eq!(
        fs::read_to_string(local.path().join("keep-local.txt")).unwrap(),
        "local unchanged"
    );
}
