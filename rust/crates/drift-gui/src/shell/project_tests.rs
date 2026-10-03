use super::*;
use crate::{actions::bind_keys, sftp_test_support::ftp::Server};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn startup_dashboard_does_not_mark_opened_and_returns_to_original_directory(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let cwd = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.register("Shop", root.path().into()).unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1200.), px(900.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    let service = BrowserService::new().unwrap();
                    Shell::new(
                        w,
                        cx,
                        store.clone(),
                        service.clone(),
                        RemoteService::new(service),
                        StartOptions {
                            directory: cwd.path().into(),
                            dashboard: true,
                            no_dashboard: false,
                            explicit_directory: false,
                        },
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).projects.read(cx).visible()
    })
    .await;
    assert!(
        store
            .registry()
            .unwrap()
            .find("shop")
            .unwrap()
            .opened_at
            .is_none()
    );
    assert!(shell.read_with(cx, |s, cx| s.browser.read(cx).location().is_none()));
    cx.update_window(handle, |_, w, cx| w.press("escape", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    assert!(
        shell.read_with(cx, |s, cx| s.browser.read(cx).location().unwrap().directory
            == cwd.path())
    );
    cx.update_window(handle, |_, w, cx| w.click("projects", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        !shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.press("down", cx);
        w.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell
            .read(cx)
            .browser
            .read(cx)
            .location()
            .is_some_and(|l| l.slug.as_deref() == Some("shop"))
    })
    .await;
    assert!(
        store
            .registry()
            .unwrap()
            .find("shop")
            .unwrap()
            .opened_at
            .is_some()
    );
}

#[gpui_kit::test]
async fn active_project_move_and_removal_close_real_remote_sessions(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = Server::new(4);
    let root = tempfile::tempdir().unwrap();
    let moved = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(root.path().join("keep"), "original").unwrap();
    fs::write(moved.path().join("new"), "moved").unwrap();
    let store = Store::new(config.path().into());
    store.register("Shop", root.path().into()).unwrap();
    store.save_host(Some("shop"), None, server.host()).unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1400.), px(1000.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    let service = BrowserService::new().unwrap();
                    Shell::new(
                        w,
                        cx,
                        store.clone(),
                        service.clone(),
                        RemoteService::with_options(service, server.options()),
                        root.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.click("remote", cx);
        w.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).remote.read(cx).has_session() && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| w.click("projects", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        !shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    let session = shell.read_with(cx, |s, cx| s.remote.read(cx).session_id());
    let browser_id = shell.read_with(cx, |s, cx| s.browser.read(cx).id());
    cx.update_window(handle, |_, w, cx| w.click("project-close", cx))
        .unwrap();
    assert_eq!(
        shell.read_with(cx, |s, cx| s.remote.read(cx).session_id()),
        session
    );
    cx.update_window(handle, |_, w, cx| w.click("projects", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        !shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| w.click("project-open-shop", cx))
        .unwrap();
    assert_eq!(
        shell.read_with(cx, |s, cx| s.remote.read(cx).session_id()),
        session
    );
    assert_eq!(
        shell.read_with(cx, |s, cx| s.browser.read(cx).id()),
        browser_id
    );
    cx.update_window(handle, |_, w, cx| w.click("projects", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        !shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.click("project-edit-shop", cx);
        let projects = shell.read(cx).projects.clone();
        projects.update(cx, |p, cx| {
            p.set_form_path(moved.path().to_str().unwrap(), w, cx)
        });
        w.click("project-save", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell
            .read(cx)
            .browser
            .read(cx)
            .location()
            .is_some_and(|l| l.directory == moved.path())
            && !shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    assert!(!shell.read_with(cx, |s, cx| s.remote.read(cx).has_session()));
    cx.update_window(handle, |_, w, cx| {
        w.click("project-close", cx);
        w.click("remote", cx);
        w.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).remote.read(cx).has_session() && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| w.click("projects", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        !shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.click("project-delete-shop", cx);
        w.click("project-delete-confirm", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell
            .read(cx)
            .browser
            .read(cx)
            .location()
            .is_some_and(|l| l.slug.is_none())
            && !shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    assert!(!config.path().join("projects/shop.toml").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("keep")).unwrap(),
        "original"
    );
    assert_eq!(
        fs::read_to_string(moved.path().join("new")).unwrap(),
        "moved"
    );
}
