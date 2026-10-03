use super::*;
use crate::{actions::bind_keys, sftp_test_support as support};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn ftps_challenge_dismisses_pending_connect_help_and_reject_restores_keyboard(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::ftp::Server::tls("valid");
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote_file = server.dir.path().join("files/remote.txt");
    fs::write(&remote_file, "remote contents").unwrap();
    fs::write(local.path().join("local.txt"), "local contents").unwrap();
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
                        store.clone(),
                        service.clone(),
                        RemoteService::with_options(service, server.options()).with_trust(
                            Arc::new(Manager::with_roots(
                                store.clone(),
                                rustls::RootCertStore::empty(),
                            )),
                        ),
                        local.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(15), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
            && !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    let browser_id = shell.read_with(cx, |s, cx| s.browser.read(cx).id());
    cx.update_window(handle, |_, w, cx| {
        w.press("home", cx);
        w.press("space", cx);
        w.press("@", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(shell.read(cx).show_remote);
        assert!(!shell.read(cx).remote_preview);
        assert!(!shell.read(cx).remote.read(cx).is_loading());
        assert!(!shell.read(cx).remote.read(cx).has_session());
        w.render_frame(cx);
        assert!(w.find(("connect-host", 0usize)).visible());
    })
    .unwrap();
    let (invoking_focus, help_focus) = cx
        .update_window(handle, |_, w, cx| {
            w.click(("connect-host", 0usize), cx);
            assert!(shell.read(cx).remote.read(cx).is_loading());
            let invoking_focus = w.focused(cx).unwrap();
            assert!(shell.read(cx).remote.focus_handle(cx).is_focused(w));
            // Do not yield to the queued real handshake completion before opening help.
            w.press("f1", cx);
            assert!(w.find("shortcut-help").visible());
            let help_focus = w.focused(cx).unwrap();
            assert_ne!(help_focus, invoking_focus);
            (invoking_focus, help_focus)
        })
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(shell.read(cx).help.is_some());
        assert!(shell.read(cx).certificate.is_none());
        assert!(shell.read(cx).remote.read(cx).is_loading());
        assert!(help_focus.is_focused(w));
    })
    .unwrap();
    // Wait for completion, then fail immediately if it was not a certificate challenge.
    cx.wait_for(handle, Duration::from_secs(15), |_, cx| {
        !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let s = shell.read(cx);
        let prompt = &s.certificate.as_ref().expect("real FTPS challenge").2;
        assert!(s.help.is_none());
        assert!(w.try_find("shortcut-help").is_none());
        assert_eq!(
            prompt.read(cx).previous_focus(),
            Some(invoking_focus.clone())
        );
        assert!(!help_focus.is_focused(w));
        assert_ne!(w.focused(cx), Some(invoking_focus.clone()));
        assert!(w.focused(cx).is_some());
        assert!(!s.remote.read(cx).has_session());
        w.press("escape", cx);
    })
    .unwrap();
    // Reject is emitted by the child prompt; its root subscription runs after the update.
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let s = shell.read(cx);
        assert!(
            s.certificate.is_none(),
            "Escape must reach the certificate prompt"
        );
        assert!(s.help.is_none());
        assert!(s.show_remote);
        assert!(!s.remote_preview);
        assert!(!s.remote.read(cx).has_session());
        assert!(!s.remote.read(cx).is_loading());
        assert!(invoking_focus.is_focused(w));
        assert!(s.remote.focus_handle(cx).is_focused(w));
        assert!(!help_focus.is_focused(w));
        assert_eq!(s.browser.read(cx).id(), browser_id);
        assert_eq!(s.browser.read(cx).marked(), ["local.txt"]);
        assert!(!s.comparison.read(cx).visible());
        assert!(!s.preview.read(cx).is_loading());
        w.press("/", cx);
        assert_eq!(w.find("remote-filter").focused(), Some(true));
        w.input("keyboard still works", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(
            w.find("remote-filter").value(),
            Some("keyboard still works")
        );
        assert!(shell.read(cx).certificate.is_none());
        assert!(shell.read(cx).help.is_none());
        assert_eq!(shell.read(cx).browser.read(cx).marked(), ["local.txt"]);
    })
    .unwrap();
    assert_eq!(server.commands("AUTH"), 1);
    for command in ["PASS", "RETR", "STOR", "DELE"] {
        assert_eq!(server.commands(command), 0, "unexpected {command}");
    }
    assert!(store.trusted_certificates().unwrap().is_empty());
    assert!(!config.path().join("trusted-certificates.toml").exists());
    assert_eq!(fs::read_to_string(remote_file).unwrap(), "remote contents");
    assert_eq!(
        fs::read_to_string(local.path().join("local.txt")).unwrap(),
        "local contents"
    );
    assert!(!local.path().join("remote.txt").exists());
    assert!(!server.dir.path().join("files/local.txt").exists());
}

#[gpui_kit::test]
async fn help_restores_invoking_hosts_filter_after_real_sftp_disconnect(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote_file = server.dir.path().join("files/remote.txt");
    fs::write(&remote_file, "remote contents").unwrap();
    fs::write(local.path().join("local.txt"), "local contents").unwrap();
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
                    let remote = RemoteService::with_options(service.clone(), server.options());
                    Shell::new(
                        w,
                        cx,
                        store.clone(),
                        service,
                        remote,
                        local.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(15), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
            && !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    let browser_id = shell.read_with(cx, |s, cx| s.browser.read(cx).id());
    cx.update_window(handle, |_, w, cx| {
        w.press("home", cx);
        w.press("space", cx);
        w.press("@", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(shell.read(cx).show_remote);
        assert!(!shell.read(cx).remote_preview);
        w.click(("connect-host", 0usize), cx);
        assert!(shell.read(cx).remote.read(cx).is_loading());
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(15), |_, cx| {
        !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    let (session_id, connection) = shell.read_with(cx, |s, cx| {
        assert!(
            s.remote.read(cx).has_session(),
            "real SFTP connect must succeed"
        );
        (
            s.remote.read(cx).session_id().unwrap(),
            s.remote.read(cx).connection(),
        )
    });
    cx.update_window(handle, |_, w, cx| {
        assert!(shell.read(cx).remote.focus_handle(cx).is_focused(w));
        w.press("home", cx);
        w.press("space", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(
            shell.read(cx).remote.read(cx).marked(),
            [remote_file.to_string_lossy().into_owned()]
        );
        w.press("shift-h", cx);
    })
    .unwrap();
    let hosts_id = shell.read_with(cx, |s, _| {
        assert!(s.show_remote);
        assert!(!s.remote_preview);
        s.hosts
            .as_ref()
            .expect("hosts must open before help")
            .entity_id()
    });
    cx.wait_for(handle, Duration::from_secs(15), |w, cx| {
        assert!(shell.read(cx).hosts.is_some());
        w.try_find(("host-edit", 0usize))
            .is_some_and(|button| button.disabled() != Some(true))
    })
    .await;
    let (hosts_focus, help_focus) = cx
        .update_window(handle, |_, w, cx| {
            assert_eq!(w.find("hosts-filter").focused(), Some(true));
            let hosts_focus = w.focused(cx).unwrap();
            w.press("f1", cx);
            assert!(w.find("shortcut-help").visible());
            let help_focus = w.focused(cx).unwrap();
            assert_ne!(hosts_focus, help_focus);
            (hosts_focus, help_focus)
        })
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let s = shell.read(cx);
        assert!(s.help.is_some());
        assert!(help_focus.is_focused(w));
        assert_eq!(s.remote.read(cx).session_id(), Some(session_id));
        assert_eq!(s.browser.read(cx).marked(), ["local.txt"]);
        let remote = s.remote.clone();
        // Exercise a real session teardown, not a fabricated connection-loss event.
        remote.update(cx, |remote, cx| remote.disconnect(cx));
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let s = shell.read(cx);
        assert!(s.help.is_some());
        assert!(help_focus.is_focused(w));
        assert_eq!(s.hosts.as_ref().unwrap().entity_id(), hosts_id);
        assert_eq!(s.remote.read(cx).connection(), connection + 1);
        assert!(!s.remote.read(cx).has_session());
        assert!(s.show_remote);
        assert!(!s.remote_preview);
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(shell.read(cx).help.is_none());
        assert_eq!(shell.read(cx).hosts.as_ref().unwrap().entity_id(), hosts_id);
        assert!(
            hosts_focus.is_focused(w),
            "restore the invoking filter, not hidden browser focus"
        );
        assert_eq!(w.find("hosts-filter").focused(), Some(true));
        w.input("Test", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(w.find("hosts-filter").value(), Some("Test"));
        assert!(hosts_focus.is_focused(w));
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let s = shell.read(cx);
        assert!(
            s.hosts.is_none(),
            "second Escape must reach hosts and close it"
        );
        assert!(s.help.is_none());
        assert!(s.certificate.is_none());
        assert!(s.browser.focus_handle(cx).is_focused(w));
        assert_eq!(s.browser.read(cx).id(), browser_id);
        assert_eq!(s.browser.read(cx).marked(), ["local.txt"]);
        assert!(!s.remote.read(cx).has_session());
        assert!(!s.remote.read(cx).is_loading());
        assert!(s.remote.read(cx).marked().is_empty());
        assert!(s.show_remote);
        assert!(!s.remote_preview);
        assert!(!s.comparison.read(cx).visible());
        assert!(!s.preview.read(cx).is_loading());
    })
    .unwrap();
    assert!(store.global().unwrap().hosts == vec![server.host()]);
    assert_eq!(fs::read_to_string(remote_file).unwrap(), "remote contents");
    assert_eq!(
        fs::read_to_string(local.path().join("local.txt")).unwrap(),
        "local contents"
    );
    assert!(!local.path().join("remote.txt").exists());
    assert!(!server.dir.path().join("files/local.txt").exists());
}
