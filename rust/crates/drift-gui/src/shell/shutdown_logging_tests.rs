use super::*;
use crate::{actions::bind_keys, sftp_test_support::ftp};
use drift_app::logging::{Logger, Options};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn closing_the_last_window_drains_real_transfer_outcomes_before_the_logger(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = ftp::Server::new(4);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let log_path = config.path().join("diagnostics.log");
    let logger = Logger::open(Options {
        path: log_path.clone(),
        debug: true,
    })
    .unwrap();
    let service = BrowserService::new().unwrap().with_logger(logger.clone());
    let store = Store::new(config.path().join("drift"));
    store.save_host(None, None, server.host()).unwrap();
    fs::write(
        local.path().join("large.txt"),
        b"SHUTDOWN-CONTENT-CANARY\n".repeat(512 * 1024),
    )
    .unwrap();
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
                    Shell::new(
                        w,
                        cx,
                        store,
                        service.clone(),
                        RemoteService::with_options(service.clone(), server.options()),
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
        shell.read(cx).remote.read(cx).has_session() && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).comparison.read(cx).session.is_some()
            && !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    server.flag("slow-stor", "");
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).comparison.focus_handle(cx).focus(w, cx);
        w.press("s", cx);
        assert!(w.try_find("sync-confirm").is_some());
        w.press(
            if cfg!(target_os = "macos") {
                "cmd-enter"
            } else {
                "ctrl-enter"
            },
            cx,
        );
    })
    .unwrap();
    let files = server.dir.path().join("files");
    cx.wait_for(handle, Duration::from_secs(20), |_, _| {
        server.commands("STOR") == 1
            && fs::read_dir(&files).unwrap().any(|entry| {
                let entry = entry.unwrap();
                drift_core::staging::is_staging_name(&entry.file_name().to_string_lossy())
                    && entry.metadata().unwrap().len() > 0
            })
    })
    .await;
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
    drop(shell);
    cx.update(|_| {});
    // Same post-GUI order as main: await cancellation/transport cleanup, then drain logs.
    service.wait_for_shutdown();
    logger.info("test exit", &[]);
    logger.finish().unwrap();
    let text = fs::read_to_string(log_path).unwrap();
    for event in ["sync.item_unknown", "sync.stopped", "sync.finished"] {
        assert!(text.contains(event), "lost terminal event {event}: {text}");
        assert!(text.find(event).unwrap() < text.find("test exit").unwrap());
    }
    assert!(!text.contains("SHUTDOWN-CONTENT-CANARY"));
    assert_eq!(server.commands("STOR"), 1);
    assert!(!files.join("large.txt").exists());
}
