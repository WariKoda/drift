use super::*;
use crate::actions::bind_keys;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn keyboard_preview_toggle_copy_focus_and_help_preserve_browser_state(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("first.txt"), "first\nsecond\nthird").unwrap();
    fs::write(dir.path().join("second.txt"), "other").unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1200.), px(800.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    Shell::new(
                        w,
                        cx,
                        Store::new(config.path().into()),
                        BrowserService::new().unwrap(),
                        RemoteService::new(BrowserService::new().unwrap()),
                        dir.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("home", cx);
        w.press("/", cx);
        w.input("first", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("enter", cx);
        assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
        assert_eq!(
            shell.read(cx).browser.read(cx).selected(),
            Some("first.txt")
        );
        assert!(!shell.read(cx).preview.read(cx).is_loading());
        w.press("space", cx);
        w.press("p", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).preview.read(cx).is_loading()
    })
    .await;
    let original = shell.read_with(cx, |s, cx| {
        (
            s.browser.entity_id(),
            s.preview.entity_id(),
            s.remote.entity_id(),
            s.browser.read(cx).id(),
        )
    });
    cx.update_window(handle, |_, w, cx| {
        w.press("c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("first\nsecond\nthird")
        );
        w.press("ctrl-shift-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("first.txt")
        );
        w.press("menu", cx);
        assert!(w.find("popup-menu").visible());
        w.press("f1", cx);
        w.press("ctrl-alt-p", cx);
        w.press("p", cx);
        w.press("c", cx);
        assert_eq!(w.find("popup-menu").focused(), Some(true));
        assert!(shell.read(cx).help.is_none());
        assert!(!shell.read(cx).show_remote);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("first.txt")
        );
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("?", cx);
        assert!(w.try_find("shortcut-help").is_some());
        w.press("s", cx);
        w.press("p", cx);
        assert!(!shell.read(cx).comparison.read(cx).visible());
        assert!(!shell.read(cx).show_remote);
        w.press("escape", cx);
        assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
        assert_eq!(shell.read(cx).browser.read(cx).marked(), ["first.txt"]);
        w.press("ctrl-alt-p", cx);
        assert!(!shell.read(cx).browser.focus_handle(cx).is_focused(w));
        w.press("ctrl-a", cx);
        w.press("ctrl-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("first\nsecond\nthird")
        );
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(shell.read(cx).show_remote);
        assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
        assert_eq!(shell.read(cx).browser.read(cx).marked(), ["first.txt"]);
        w.press("p", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        let stale = shell.read(cx).preview.read(cx).id();
        w.press("p", cx);
        assert!(shell.read(cx).show_remote);
        let preview = shell.read(cx).preview.clone();
        preview.update(cx, |_, cx| cx.emit(PreviewEvent::Loaded(stale)));
        assert_eq!(
            shell.read(cx).browser.read(cx).selected(),
            Some("first.txt")
        );
        assert_eq!(shell.read(cx).browser.read(cx).len(), 1);
        assert!(!shell.read(cx).comparison.read(cx).visible());
    })
    .unwrap();
    shell.read_with(cx, |s, cx| {
        assert_eq!(
            (
                s.browser.entity_id(),
                s.preview.entity_id(),
                s.remote.entity_id(),
                s.browser.read(cx).id()
            ),
            original
        );
    });
    assert_eq!(
        fs::read_to_string(dir.path().join("first.txt")).unwrap(),
        "first\nsecond\nthird"
    );
}

#[gpui_kit::test]
async fn remote_keyboard_preview_restore_retains_connection_marks_and_files(
    cx: &mut TestAppContext,
) {
    use crate::sftp_test_support as support;
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let remote_file = server.dir.path().join("files/remote.txt");
    fs::write(&remote_file, "remote keyboard contents").unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("local.txt"), "local keyboard contents").unwrap();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1200.), px(800.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    let service = BrowserService::new().unwrap();
                    let remote = RemoteService::with_options(service.clone(), server.options());
                    Shell::new(w, cx, store, service, remote, dir.path().to_path_buf())
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("home", cx);
        w.press("space", cx);
        w.press("@", cx);
        w.press("ctrl-f", cx);
        for _ in 0..32 {
            w.press("shift-tab", cx);
            if w.find(("connect-host", 0usize)).focused() == Some(true) {
                break;
            }
        }
        assert_eq!(w.find(("connect-host", 0usize)).focused(), Some(true));
        w.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).remote.read(cx).has_session() && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    let connection = shell.read_with(cx, |s, cx| s.remote.read(cx).session_id());
    cx.update_window(handle, |_, w, cx| {
        w.press("/", cx);
        w.input("remote.txt", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("down", cx);
        assert!(shell.read(cx).remote.focus_handle(cx).is_focused(w));
        w.press("home", cx);
        w.press("space", cx);
        w.press("p", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).remote_preview && !shell.read(cx).preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.press("c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("remote keyboard contents")
        );
        w.press("?", cx);
        w.press("escape", cx);
        assert!(shell.read(cx).remote.focus_handle(cx).is_focused(w));
        w.press("p", cx);
        assert!(!shell.read(cx).remote_preview);
        assert_eq!(shell.read(cx).remote.read(cx).session_id(), connection);
        assert_eq!(
            shell.read(cx).remote.read(cx).marked(),
            [remote_file.to_string_lossy().into_owned()]
        );
        assert_eq!(shell.read(cx).browser.read(cx).marked(), ["local.txt"]);
        assert_eq!(
            shell.read(cx).remote.read(cx).selected(),
            remote_file.to_str()
        );
        assert!(shell.read(cx).remote.focus_handle(cx).is_focused(w));
        assert!(!shell.read(cx).comparison.read(cx).visible());
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(remote_file).unwrap(),
        "remote keyboard contents"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("local.txt")).unwrap(),
        "local keyboard contents"
    );
}

#[gpui_kit::test]
async fn help_from_editable_filter_restores_typing_and_scrolls_without_running_commands(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("pc.txt"), "content").unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(600.), px(360.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    Shell::new(
                        w,
                        cx,
                        Store::new(config.path().into()),
                        BrowserService::new().unwrap(),
                        RemoteService::new(BrowserService::new().unwrap()),
                        dir.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.press("/", cx);
        w.input("p", cx);
        let filter_focus = w.focused(cx).unwrap();
        w.press("f1", cx);
        assert!(w.try_find("shortcut-help").is_some());
        w.press("end", cx);
        assert!(shell.read(cx).help.is_some());
        w.press("escape", cx);
        assert!(filter_focus.is_focused(w));
        w.input("c", cx);
        assert_eq!(shell.read(cx).browser.read(cx).len(), 1);
        assert!(!shell.read(cx).preview.read(cx).is_loading());
        w.press("down", cx);
        assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
        w.press("f1", cx);
        w.press("tab", cx);
        w.press("enter", cx);
        assert!(shell.read(cx).help.is_none());
        assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
        w.press("/", cx);
        let preview_id = shell.read(cx).preview.read(cx).id();
        w.press("escape", cx);
        assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
        assert_eq!(w.find("filter").value(), Some("pc"));
        assert_eq!(shell.read(cx).preview.read(cx).id(), preview_id);
        w.press("f", cx);
        assert!(shell.read(cx).browser.read(cx).is_loading());
        w.press("ctrl-escape", cx);
        assert!(!shell.read(cx).browser.read(cx).is_loading());
        assert!(!shell.read(cx).comparison.read(cx).visible());
    })
    .unwrap();
}
