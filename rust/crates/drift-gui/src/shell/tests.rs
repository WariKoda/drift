use super::*;
use crate::actions::bind_keys;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn real_nested_navigation_buttons_history_and_input_focus(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    for registered in [false, true] {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("root");
        let child = root.join("child");
        let nested = child.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        if registered {
            store.register("Project", root.clone()).unwrap();
        }
        let (handle, shell) = cx.update(|cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1100.), px(720.)),
                    })),
                    ..Default::default()
                },
                cx,
                |w, cx| {
                    cx.new(|cx| {
                        Shell::new(
                            w,
                            cx,
                            store,
                            BrowserService::new().unwrap(),
                            RemoteService::new(BrowserService::new().unwrap()),
                            root.clone(),
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
        for (control, expected) in [
            ("row", child.clone()),
            ("row", nested.clone()),
            ("up", child.clone()),
            ("back", nested.clone()),
            ("alt-left", child.clone()),
            ("alt-right", nested.clone()),
            ("left", child.clone()),
            ("backspace", root.clone()),
        ] {
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                match control {
                    "row" => w.click(0usize, cx),
                    "up" | "back" => w.click(control, cx),
                    key => w.press(key, cx),
                }
            })
            .unwrap();
            cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                let shell = shell.read(cx);
                !shell.browser.read(cx).is_loading()
                    && shell
                        .browser
                        .read(cx)
                        .location()
                        .is_some_and(|l| l.directory == expected)
            })
            .await;
        }
        cx.update_window(handle, |_, w, cx| {
            w.click("filter", cx);
            w.input("hjl", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            assert_eq!(shell.read(cx).browser.read(cx).len(), 0);
            assert_eq!(
                shell
                    .read(cx)
                    .browser
                    .read(cx)
                    .location()
                    .unwrap()
                    .directory,
                root
            );
            w.click("up", cx);
        })
        .unwrap();
        let expected = if registered {
            root.clone()
        } else {
            parent.path().into()
        };
        cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
            let shell = shell.read(cx);
            !shell.browser.read(cx).is_loading()
                && shell
                    .browser
                    .read(cx)
                    .location()
                    .is_some_and(|l| l.directory == expected)
        })
        .await;
        assert_eq!(
            shell.read_with(cx, |s, cx| s
                .browser
                .read(cx)
                .location()
                .unwrap()
                .root
                .base()
                .to_path_buf()),
            expected
        );
        if !registered {
            // Registration belongs to the displayed folder, even when a
            // wider unregistered capability root was opened by Up.
            cx.update_window(handle, |_, w, cx| {
                w.click("back", cx);
            })
            .unwrap();
            cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                let s = shell.read(cx);
                !s.browser.read(cx).is_loading()
                    && s.browser
                        .read(cx)
                        .location()
                        .is_some_and(|l| l.directory == root)
            })
            .await;
            cx.update_window(handle, |_, w, cx| {
                w.click(0usize, cx);
            })
            .unwrap();
            cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                let s = shell.read(cx);
                !s.browser.read(cx).is_loading()
                    && s.browser
                        .read(cx)
                        .location()
                        .is_some_and(|l| l.directory == child)
            })
            .await;
            cx.update_window(handle, |_, w, cx| {
                w.click("projects", cx);
                w.click("project-name", cx);
                w.input("Nested project", cx);
                w.click("register", cx);
            })
            .unwrap();
            cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                shell
                    .read(cx)
                    .browser
                    .read(cx)
                    .location()
                    .is_some_and(|l| l.slug.is_some())
            })
            .await;
            let registry = Store::new(config.path().into()).registry().unwrap();
            assert_eq!(registry.projects[0].path, child);
        }
    }
}

#[gpui_kit::test]
async fn vanished_directory_keeps_the_previous_location_and_history(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let vanished = root.path().join("vanished");
    fs::create_dir(&vanished).unwrap();
    let config = tempfile::tempdir().unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
            cx.new(|cx| {
                Shell::new(
                    w,
                    cx,
                    Store::new(config.path().into()),
                    BrowserService::new().unwrap(),
                    RemoteService::new(BrowserService::new().unwrap()),
                    root.path().to_path_buf(),
                )
            })
        })
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    fs::remove_dir(vanished).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click(0usize, cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    shell.read_with(cx, |s, cx| {
        assert_eq!(
            s.browser.read(cx).location().unwrap().directory,
            root.path()
        );
        assert!(!s.browser.read(cx).can_back());
        assert!(!s.status.ends_with("entries"));
    });
}
#[gpui_kit::test]
async fn filtering_preview_clipboard_and_panels_preserve_the_browser(cx: &mut TestAppContext) {
    use gpui_kit::Focusable;
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("first.txt"), "first\nsecond").unwrap();
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
                ..WindowOptions::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    Shell::new(
                        window,
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
    cx.update_window(handle, |_, window, cx| {
        window.click("filter", cx);
        window.input("first", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(shell.read(cx).browser.read(cx).len(), 1);
        window.click("projects", cx);
        window.press("escape", cx);
        assert!(!shell.read(cx).projects.read(cx).visible());
        window.click(0usize, cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            shell.read(cx).browser.read(cx).selected(),
            Some("first.txt")
        );
        window.press("space", cx); // Marking must preserve the loaded preview.
        assert_eq!(shell.read(cx).browser.read(cx).marked(), ["first.txt"]);
        window.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("first\nsecond")
        );
        window.click("copy-selection", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("first.txt")
        );
        window.click("hosts", cx);
        assert!(shell.read(cx).hosts.is_some());
        window.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert!(shell.read(cx).hosts.is_none());
        assert_eq!(
            shell.read(cx).browser.read(cx).selected(),
            Some("first.txt")
        );
        assert!(shell.read(cx).browser.focus_handle(cx).is_focused(window));
    })
    .unwrap();
}

#[gpui_kit::test]
async fn real_sftp_connection_preview_and_project_switch_cross_view_boundaries(
    cx: &mut TestAppContext,
) {
    use crate::sftp_test_support as support;
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    fs::write(
        server.dir.path().join("files/remote.txt"),
        "remote contents",
    )
    .unwrap();
    fs::write(
        server.dir.path().join("files/second.txt"),
        "latest remote contents",
    )
    .unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::write(local.path().join("local.txt"), "local contents").unwrap();
    let other = tempfile::tempdir().unwrap();
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
            |window, cx| {
                cx.new(|cx| {
                    let service = BrowserService::new().unwrap();
                    let remote = RemoteService::with_options(service.clone(), server.options());
                    Shell::new(
                        window,
                        cx,
                        store,
                        service,
                        remote,
                        local.path().to_path_buf(),
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
    cx.update_window(handle, |_, window, cx| {
        window.click("remote", cx);
        window.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    assert!(shell.read_with(cx, |shell, cx| shell.remote.read(cx).has_session()));
    cx.update_window(handle, |_, window, cx| {
        window.within("remote-pane").click(0usize, cx);
        window.within("remote-pane").click(1usize, cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).remote_preview && !shell.read(cx).preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.press("space", cx);
        assert_eq!(shell.read(cx).remote.read(cx).marked().len(), 1);
        assert!(shell.read(cx).remote_preview);
        window.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("latest remote contents")
        );
        assert!(shell.read(cx).remote.read(cx).has_session());
        window.click("local-browser", cx);
        window.within("browser-pane").click(0usize, cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.press("space", cx);
        assert_eq!(shell.read(cx).browser.read(cx).marked(), ["local.txt"]);
        window.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("local contents")
        );
        shell.update(cx, |shell, cx| {
            shell.open_project(other.path().into(), window, cx)
        });
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell
            .read(cx)
            .browser
            .read(cx)
            .location()
            .is_some_and(|l| l.directory == other.path())
    })
    .await;
    assert!(!shell.read_with(cx, |shell, cx| shell.remote.read(cx).has_session()));
    assert!(shell.read_with(cx, |shell, cx| shell.browser.read(cx).marked().is_empty()));
    assert!(shell.read_with(cx, |shell, cx| shell.remote.read(cx).marked().is_empty()));
}
