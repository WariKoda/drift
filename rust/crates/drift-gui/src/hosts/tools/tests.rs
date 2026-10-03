use super::*;
use crate::sftp_test_support::ftp::Server;
use drift_app::browser::BrowserService;
use drift_core::tlstrust::Manager;
use fs2::FileExt;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
};
use std::{cell::Cell, fs, rc::Rc, sync::Arc, time::Duration};

fn open_tools(
    store: Store,
    service: RemoteService,
    host: Host,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<HostTools>) {
    cx.update(|cx| {
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
            |w, cx| cx.new(|cx| HostTools::new(store, service, None, host, Mode::Test, w, cx)),
        )
        .unwrap()
    })
}

mod keyboard;

async fn idle(handle: AnyWindowHandle, tools: &Entity<HostTools>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        tools.read(cx).cancel.is_none()
    })
    .await;
}

#[gpui_kit::test]
async fn real_host_test_certificate_decisions_and_reset_conflicts(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    for mode in ["reject", "session", "permanent", "conflict", "stale"] {
        let server = Server::tls("valid");
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        let manager = Arc::new(Manager::with_roots(
            store.clone(),
            rustls::RootCertStore::empty(),
        ));
        let (handle, tools) = cx.update(|cx| {
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
                    cx.new(|cx| {
                        HostTools::new(
                            store.clone(),
                            RemoteService::with_options(
                                BrowserService::new().unwrap(),
                                server.options(),
                            )
                            .with_trust(manager.clone()),
                            None,
                            server.host(),
                            Mode::Test,
                            w,
                            cx,
                        )
                    })
                },
            )
            .unwrap()
        });
        idle(handle, &tools, cx).await;
        let stale = tools.read_with(cx, |tools, _| tools.certificate.as_ref().unwrap().clone());
        let inspected = stale.read_with(cx, |prompt, _| prompt.challenge.clone());
        assert_eq!(server.commands("PASS"), 0);
        match mode {
            "reject" => {
                cx.update_window(handle, |_, w, cx| w.press("enter", cx))
                    .unwrap();
                assert!(tools.read_with(cx, |tools, _| tools.certificate.is_none()
                    && tools.status.contains("rejected")));
                assert!(!config.path().join("trusted-certificates.toml").exists());
            }
            "stale" => {
                cx.update_window(handle, |_, w, cx| {
                    tools.update(cx, |tools, cx| tools.close(&Cancel, w, cx))
                })
                .unwrap();
                stale.update(cx, |_, cx| cx.emit(Decision::Permanent));
                cx.run_until_parked();
                assert!(!config.path().join("trusted-certificates.toml").exists());
                assert_eq!(server.commands("PASS"), 0);
            }
            "conflict" => {
                store
                    .save_trusted_certificate(None, inspected.trust().unwrap())
                    .unwrap();
                cx.update_window(handle, |_, w, cx| {
                    w.press("tab", cx);
                    w.press("tab", cx);
                    w.press("enter", cx);
                })
                .unwrap();
                idle(handle, &tools, cx).await;
                tools.read_with(cx, |tools, cx| {
                    let prompt = tools.certificate.as_ref().unwrap().read(cx);
                    assert!(prompt.error.as_ref().unwrap().contains("changed"));
                    assert!(!prompt.busy);
                });
                assert_eq!(server.commands("PASS"), 0);
                assert!(
                    manager
                        .inspect(inspected.endpoint)
                        .unwrap()
                        .session
                        .is_none()
                );
            }
            _ => {
                cx.update_window(handle, |_, w, cx| {
                    w.press("right", cx);
                    if mode == "permanent" {
                        w.press("l", cx);
                    }
                    w.press("enter", cx);
                    w.press("enter", cx); // Repeated approval while the write/connect is pending.
                })
                .unwrap();
                idle(handle, &tools, cx).await;
                tools.read_with(cx, |tools, _| {
                    assert!(tools.status.contains("successful"), "{}", tools.status);
                    assert!(tools.certificate.is_none());
                });
                assert!(server.commands("PASS") > 0);
                assert_eq!(
                    config.path().join("trusted-certificates.toml").exists(),
                    mode == "permanent"
                );
                cx.update_window(handle, |_, w, cx| {
                    tools.update(cx, |tools, cx| {
                        tools.focus.focus(w, cx);
                        tools.inspect(w, cx);
                    })
                })
                .unwrap();
                idle(handle, &tools, cx).await;
                if mode == "permanent" {
                    let before = store.trusted_certificates().unwrap().remove(0);
                    let mut concurrent = before.clone();
                    concurrent.fingerprint = vec!["EF"; 32].join(":");
                    store
                        .save_trusted_certificate(Some(&before), concurrent.clone())
                        .unwrap();
                    cx.update_window(handle, |_, w, cx| w.press("enter", cx))
                        .unwrap();
                    idle(handle, &tools, cx).await;
                    tools.read_with(cx, |tools, _| {
                        assert!(tools.status.contains("changed"));
                        assert!(tools.reset.is_some());
                    });
                    assert_eq!(store.trusted_certificates().unwrap(), vec![concurrent]);
                    cx.update_window(handle, |_, w, cx| w.press("r", cx))
                        .unwrap();
                    idle(handle, &tools, cx).await;
                }
                let before = server.commands("PASS");
                cx.update_window(handle, |_, w, cx| w.press("enter", cx))
                    .unwrap();
                idle(handle, &tools, cx).await;
                assert_eq!(server.commands("PASS"), before, "reset must not reconnect");
                assert!(store.trusted_certificates().unwrap().is_empty());
                let snapshot = manager.inspect(inspected.endpoint).unwrap();
                assert!(snapshot.session.is_none());
                assert!(snapshot.persistent.is_none());
                cx.update_window(handle, |_, w, cx| w.click("host-test-again", cx))
                    .unwrap();
                idle(handle, &tools, cx).await;
                assert!(tools.read_with(cx, |tools, _| tools.certificate.is_some()));
                assert_eq!(server.commands("PASS"), before);
            }
        }
        assert!(!config.path().join("config.toml").exists());
        assert!(!config.path().join("projects.toml").exists());
        assert!(
            fs::read_dir(server.dir.path().join("files"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}
