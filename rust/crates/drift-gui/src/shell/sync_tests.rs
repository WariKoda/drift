use super::*;
use crate::{actions::bind_keys, sftp_test_support as support};
use drift_app::sync::{ItemOutcome, StopReason};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, os::unix::fs::symlink, time::Duration};

#[gpui_kit::test]
async fn selected_sync_deletion_errors_and_cancel_are_visible_without_reusing_a_stale_comparison(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    fs::write(remote.join("a-delete"), b"delete this remote file").unwrap();
    fs::write(local.path().join("b-local"), b"keep this local file").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1400.), px(900.)),
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
                        store,
                        service.clone(),
                        RemoteService::with_options(service, server.options()),
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
    cx.update_window(handle, |_, w, cx| {
        w.click("remote", cx);
        w.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.click("comparison-action", cx); // Download -> Delete remote.
        shell.read(cx).comparison.focus_handle(cx).focus(w, cx);
        w.press("s", cx);
        w.press("escape", cx); // Dismiss only the confirmation.
    })
    .unwrap();
    assert!(remote.join("a-delete").exists());
    assert!(shell.read_with(cx, |s, cx| s.comparison.read(cx).visible()));
    cx.update_window(handle, |_, w, cx| {
        w.click("sync-selected", cx);
        w.click("sync-confirm", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
            && !shell.read(cx).remote.read(cx).is_loading()
            && !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    assert!(!remote.join("a-delete").exists());
    assert!(!remote.join("b-local").exists());
    shell.read_with(cx, |s, cx| {
        let pane = s.comparison.read(cx);
        let report = pane.sync_result_for_test().unwrap();
        assert_eq!(
            report.outcomes,
            vec![ItemOutcome::Completed, ItemOutcome::Skipped]
        );
        assert_eq!(pane.session.as_ref().unwrap().entries.len(), 1);
    });
    cx.update_window(handle, |_, w, cx| {
        w.click("sync-selected", cx);
        w.click("sync-confirm", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    assert_eq!(
        fs::read(remote.join("b-local")).unwrap(),
        b"keep this local file"
    );
    fs::write(remote.join("fail-local"), b"remote data").unwrap();
    cx.update_window(handle, |_, w, cx| w.click("comparison-refresh", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    fs::write(outside.path().join("secret"), b"keep secret").unwrap();
    symlink(
        outside.path().join("secret"),
        local.path().join("fail-local"),
    )
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("sync-selected", cx);
        w.click("sync-confirm", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    shell.read_with(cx, |s, cx| {
        let pane = s.comparison.read(cx);
        assert!(matches!(
            pane.sync_result_for_test().unwrap().outcomes[0],
            ItemOutcome::Failed(_)
        ));
        assert!(pane.session.as_ref().unwrap().entries[0].error.is_some());
    });
    assert_eq!(
        fs::read(outside.path().join("secret")).unwrap(),
        b"keep secret"
    );
    fs::write(local.path().join("z-large"), vec![b'x'; 16 * 1024 * 1024]).unwrap();
    cx.update_window(handle, |_, w, cx| w.click("comparison-refresh", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    // Keep the real transfer active until Cancel has been delivered and a new
    // frame has rendered. A fast server could previously finish between clicks.
    fs::write(server.dir.path().join("slow-data"), "").unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("sync-all", cx);
        w.click("sync-confirm", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).comparison.read(cx).is_loading()
            && server.dir.path().join("write-started").exists()
    })
    .await;
    cx.update_window(handle, |_, w, cx| w.click("comparison-back", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
            && !shell.read(cx).remote.read(cx).has_session()
    })
    .await;
    shell.read_with(cx, |s, cx| {
        let pane = s.comparison.read(cx);
        assert!(pane.visible());
        assert_eq!(
            pane.sync_result_for_test().unwrap().stopped,
            Some(StopReason::Cancelled)
        );
    });
    assert!(!remote.join("z-large").exists());
    cx.update_window(handle, |_, w, cx| w.click("comparison-back", cx))
        .unwrap();
    assert!(!shell.read_with(cx, |s, cx| s.comparison.read(cx).visible()));
}

#[gpui_kit::test]
async fn direct_transfer_keys_require_a_source_and_confirmation(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    fs::write(local.path().join("a-both"), "local").unwrap();
    fs::write(remote.join("a-both"), "remote").unwrap();
    fs::write(local.path().join("b-local"), "upload").unwrap();
    fs::write(remote.join("c-remote"), "download").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1400.), px(900.)),
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
                        store,
                        service.clone(),
                        RemoteService::with_options(service, server.options()),
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
    cx.update_window(handle, |_, w, cx| {
        w.click("remote", cx);
        w.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.press("end", cx); // Remote only: upload has no source.
        w.press("u", cx);
        w.press("ctrl-enter", cx);
        assert!(!shell.read(cx).comparison.read(cx).is_loading());
        w.press("d", cx);
        w.press("escape", cx);
        assert!(shell.read(cx).comparison.read(cx).visible());
        w.press("home", cx);
        w.press("down", cx); // Local only: download has no source.
        w.press("d", cx);
        w.press("ctrl-enter", cx);
        assert!(!shell.read(cx).comparison.read(cx).is_loading());
        w.press("u", cx);
    })
    .unwrap();
    assert!(!remote.join("b-local").exists());
    assert!(!local.path().join("c-remote").exists());
    cx.update_window(handle, |_, w, cx| w.press("ctrl-enter", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
            && !shell.read(cx).browser.read(cx).is_loading()
            && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    assert_eq!(fs::read(remote.join("b-local")).unwrap(), b"upload");
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).comparison.focus_handle(cx).focus(w, cx);
        w.press("end", cx);
        w.press("tab", cx); // Direct download also works while viewing the diff.
        w.press("d", cx);
    })
    .unwrap();
    assert!(!local.path().join("c-remote").exists());
    cx.update_window(handle, |_, w, cx| w.press("cmd-enter", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
            && !shell.read(cx).browser.read(cx).is_loading()
            && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    assert_eq!(
        fs::read(local.path().join("c-remote")).unwrap(),
        b"download"
    );
    assert_eq!(fs::read(local.path().join("a-both")).unwrap(), b"local");
    assert_eq!(fs::read(remote.join("a-both")).unwrap(), b"remote");
}
