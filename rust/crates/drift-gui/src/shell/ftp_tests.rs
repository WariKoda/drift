use super::*;
use crate::{actions::bind_keys, sftp_test_support::ftp::Server};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn ftp_and_ftps_connect_preview_compare_sync_and_project_switch_use_existing_views(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for tls in [false, true] {
        let server = if tls {
            Server::tls_with_limit("valid", 2)
        } else {
            Server::new(2)
        };
        let local = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        fs::write(server.dir.path().join("files/a-remote"), "remote contents").unwrap();
        fs::write(local.path().join("b-local"), "local contents").unwrap();
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
            !shell.read(cx).remote.read(cx).is_loading()
        })
        .await;
        if tls {
            cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
                shell.read(cx).certificate.is_some()
            })
            .await;
            cx.update_window(handle, |_, w, cx| w.click("certificate-session", cx))
                .unwrap();
            cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
                shell.read(cx).remote.read(cx).has_session()
                    && !shell.read(cx).remote.read(cx).is_loading()
            })
            .await;
            assert!(!config.path().join("trusted-certificates.toml").exists());
        }
        assert!(shell.read_with(cx, |s, cx| s.remote.read(cx).has_session()));
        cx.update_window(handle, |_, w, cx| w.within("remote-pane").click(0usize, cx))
            .unwrap();
        cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
            shell.read(cx).remote_preview && !shell.read(cx).preview.read(cx).is_loading()
        })
        .await;
        cx.update_window(handle, |_, w, cx| {
            w.click("copy-preview", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("remote contents")
            );
            w.click("local-browser", cx);
            w.click("compare-project", cx);
        })
        .unwrap();
        cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
            !shell.read(cx).comparison.read(cx).is_loading()
        })
        .await;
        shell.read_with(cx, |s, cx| {
            assert_eq!(
                s.comparison
                    .read(cx)
                    .session
                    .as_ref()
                    .unwrap()
                    .entries
                    .len(),
                2
            )
        });
        cx.update_window(handle, |_, w, cx| {
            w.click("sync-all", cx);
            w.click("sync-confirm", cx);
        })
        .unwrap();
        cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
            !shell.read(cx).comparison.read(cx).is_loading()
                && !shell.read(cx).browser.read(cx).is_loading()
                && !shell.read(cx).remote.read(cx).is_loading()
        })
        .await;
        assert_eq!(
            fs::read(local.path().join("a-remote")).unwrap(),
            b"remote contents"
        );
        assert_eq!(
            fs::read(server.dir.path().join("files/b-local")).unwrap(),
            b"local contents"
        );
        shell.read_with(cx, |s, cx| {
            assert!(
                s.comparison
                    .read(cx)
                    .session
                    .as_ref()
                    .unwrap()
                    .entries
                    .is_empty()
            )
        });
        cx.update_window(handle, |_, w, cx| {
            shell.update(cx, |s, cx| s.open_project(other.path().into(), w, cx))
        })
        .unwrap();
        cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
            shell
                .read(cx)
                .browser
                .read(cx)
                .location()
                .is_some_and(|l| l.directory == other.path())
        })
        .await;
        assert!(!shell.read_with(cx, |s, cx| s.remote.read(cx).has_session()));
    }
}
