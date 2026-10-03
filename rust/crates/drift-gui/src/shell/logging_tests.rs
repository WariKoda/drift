use super::*;
use drift_app::logging::{Logger, Options};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn logging_open_warning_remains_visible_in_browser_and_management(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let blocked = config.path().join("blocked");
    fs::write(&blocked, "not a directory").unwrap();
    let error = Logger::open(Options {
        path: blocked.join("log"),
        debug: false,
    })
    .err()
    .unwrap();
    let warning = format!("Could not open log file: {error}");
    let logger = Logger::disabled(Some(warning.clone()));
    let service = BrowserService::new().unwrap().with_logger(logger);
    let store = Store::new(config.path().join("drift"));
    let server = crate::sftp_test_support::ftp::Server::tls_with_limit("valid", 4);
    store.save_host(None, None, server.host()).unwrap();
    let remote =
        RemoteService::with_options(service.clone(), server.options()).with_trust(Arc::new(
            Manager::with_roots(store.clone(), rustls::RootCertStore::empty()),
        ));
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
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
                cx.new(|cx| Shell::new(w, cx, store, service, remote, root.path().to_path_buf()))
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(shell.read(cx).logging_warning.as_ref(), Some(&warning));
        w.render_frame(cx);
        assert!(w.find("logging-warning").visible());
        w.press("shift-p", cx);
        assert!(shell.read(cx).projects.read(cx).visible());
        assert!(w.find("logging-warning").visible());
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(!shell.read(cx).projects.read(cx).visible());
        w.press("shift-h", cx);
        assert!(shell.read(cx).hosts.is_some());
        assert!(w.find("logging-warning").visible());
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(shell.read(cx).hosts.is_none());
        assert!(w.find("logging-warning").visible());
        assert_eq!(shell.read(cx).logging_warning.as_ref(), Some(&warning));
        w.click("remote", cx);
        w.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).certificate.is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert!(w.find("logging-warning").visible());
        w.click("certificate-session", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).remote.read(cx).has_session()
            && shell.read(cx).certificate.is_none()
            && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).comparison.read(cx).session.is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert!(shell.read(cx).comparison.read(cx).visible());
        assert!(w.find("logging-warning").visible());
        assert_eq!(shell.read(cx).logging_warning.as_ref(), Some(&warning));
    })
    .unwrap();
}

#[cfg(target_os = "linux")]
#[gpui_kit::test]
async fn background_write_failure_reaches_the_open_window_without_replacing_session_status(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let logger = Logger::open(Options {
        path: "/dev/full".into(),
        debug: false,
    })
    .unwrap();
    let service = BrowserService::new().unwrap().with_logger(logger.clone());
    let store = Store::new(config.path().into());
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
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
                        service.clone(),
                        RemoteService::new(service),
                        root.path().to_path_buf(),
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
    let status = shell.read_with(cx, |shell, _| shell.status.clone());
    logger.info("real write failure", &[]);
    cx.wait_for(handle, Duration::from_secs(10), |_, cx| {
        shell.read(cx).logging_warning.is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(shell.read(cx).status, status);
        assert!(
            shell
                .read(cx)
                .logging_warning
                .as_ref()
                .unwrap()
                .contains("Could not write log file /dev/full")
        );
        w.render_frame(cx);
        assert!(w.find("logging-warning").visible());
        assert!(shell.read(cx).browser.read(cx).location().is_some());
    })
    .unwrap();
    assert!(logger.finish().is_err());
}
