use super::*;
use crate::{actions::bind_keys, sftp_test_support::ftp::Server};
use drift_app::sync::ItemOutcome;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{
    fs,
    time::{Duration, UNIX_EPOCH},
};

#[gpui_kit::test]
async fn ftps_reject_conflict_and_project_switch_preserve_certificate_dialog_identity(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for mode in ["reject", "conflict", "project", "permanent", "comparison"] {
        let server = Server::tls("valid");
        let local = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        if mode == "comparison" {
            fs::write(local.path().join("a-confirmed"), b"delete").unwrap();
            fs::write(local.path().join("b-target"), b"old local").unwrap();
            fs::write(server.dir.path().join("files/b-target"), b"new remote").unwrap();
            fs::File::open(local.path().join("b-target"))
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(100)))
                .unwrap();
        }
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
            shell.read(cx).certificate.is_some()
        })
        .await;
        assert_eq!(server.commands("PASS"), 0);
        let inspected = shell.read_with(cx, |s, cx| {
            s.certificate.as_ref().unwrap().2.read(cx).challenge.clone()
        });
        match mode {
            "reject" => {
                cx.update_window(handle, |_, w, cx| w.press("escape", cx))
                    .unwrap();
                assert!(shell.read_with(cx, |s, cx| s.certificate.is_none()
                    && !s.remote.read(cx).has_session()));
                assert!(!config.path().join("trusted-certificates.toml").exists());
            }
            "conflict" => {
                // A second real process/store write after inspection must retain the dialog.
                store
                    .save_trusted_certificate(None, inspected.trust().unwrap())
                    .unwrap();
                cx.update_window(handle, |_, w, cx| w.click("certificate-permanent", cx))
                    .unwrap();
                cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
                    shell
                        .read(cx)
                        .certificate
                        .as_ref()
                        .is_some_and(|(_, _, p)| p.read(cx).error.is_some())
                })
                .await;
                shell.read_with(cx, |s, cx| {
                    let p = s.certificate.as_ref().unwrap().2.read(cx);
                    assert_eq!(p.challenge, inspected);
                    assert!(!p.busy);
                    assert!(p.error.as_ref().unwrap().contains("changed"));
                    assert!(!s.remote.read(cx).has_session());
                });
                assert_eq!(server.commands("PASS"), 0);
                cx.update_window(handle, |_, w, cx| w.click("certificate-reject", cx))
                    .unwrap();
            }
            "project" => {
                let stale = shell.read_with(cx, |s, _| s.certificate.as_ref().unwrap().2.clone());
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
                stale.update(cx, |_, cx| {
                    cx.emit(crate::certificates::Decision::Permanent)
                });
                assert!(shell.read_with(cx, |s, cx| s.certificate.is_none()
                    && !s.remote.read(cx).has_session()));
                assert!(!config.path().join("trusted-certificates.toml").exists());
                assert_eq!(server.commands("PASS"), 0);
            }
            "permanent" => {
                cx.update_window(handle, |_, w, cx| w.click("certificate-permanent", cx))
                    .unwrap();
                cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
                    shell.read(cx).certificate.is_none()
                        && shell.read(cx).remote.read(cx).has_session()
                        && !shell.read(cx).remote.read(cx).is_loading()
                })
                .await;
                assert_eq!(
                    store.trusted_certificates().unwrap()[0].fingerprint,
                    inspected.fingerprint
                );
                // Changing only the protected data socket must interrupt the current session
                // and ask again; it cannot be accepted by the earlier endpoint exception.
                server.flag("rotate-data", "");
                cx.update_window(handle, |_, w, cx| w.click("remote-refresh", cx))
                    .unwrap();
                cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
                    shell.read(cx).certificate.is_some()
                })
                .await;
                shell.read_with(cx, |s, cx| {
                    assert!(!s.remote.read(cx).has_session());
                    let p = s.certificate.as_ref().unwrap().2.read(cx);
                    assert!(
                        p.challenge
                            .problems
                            .contains(&drift_core::tlstrust::Problem::CertificateChanged)
                    );
                    assert_ne!(p.challenge.fingerprint, inspected.fingerprint);
                });
                cx.update_window(handle, |_, w, cx| w.click("certificate-reject", cx))
                    .unwrap();
            }
            "comparison" => {
                cx.update_window(handle, |_, w, cx| w.click("certificate-session", cx))
                    .unwrap();
                cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
                    shell.read(cx).remote.read(cx).has_session()
                        && !shell.read(cx).remote.read(cx).is_loading()
                })
                .await;
                cx.update_window(handle, |_, w, cx| w.click("compare-project", cx))
                    .unwrap();
                cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
                    shell.read(cx).comparison.read(cx).session.is_some()
                        && !shell.read(cx).comparison.read(cx).is_loading()
                })
                .await;
                server.flag("rotate-data", "");
                cx.update_window(handle, |_, w, cx| {
                    w.click("comparison-action", cx); // Local-only Upload -> Delete local.
                    w.click("sync-all", cx);
                    w.click("sync-confirm", cx);
                })
                .unwrap();
                cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
                    shell.read(cx).certificate.is_some()
                        && !shell.read(cx).comparison.read(cx).is_loading()
                })
                .await;
                shell.read_with(cx, |s, cx| {
                    let report = s.comparison.read(cx).sync_result_for_test().unwrap();
                    assert_eq!(report.outcomes[0], ItemOutcome::Completed);
                    assert!(matches!(report.outcomes[1], ItemOutcome::Failed(_)));
                });
                let attempts = server.commands("RETR");
                server.flag("rotate-cert", ""); // The explicit pinned retry now uses the inspected new certificate.
                cx.update_window(handle, |_, w, cx| w.click("certificate-session", cx))
                    .unwrap();
                cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
                    shell.read(cx).certificate.is_none()
                        && shell.read(cx).remote.read(cx).has_session()
                        && !shell.read(cx).remote.read(cx).is_loading()
                })
                .await;
                assert_eq!(server.commands("RETR"), attempts); // No automatic comparison or download.
                assert_eq!(
                    fs::read(local.path().join("b-target")).unwrap(),
                    b"old local"
                );
                assert!(!local.path().join("a-confirmed").exists());
                cx.update_window(handle, |_, w, cx| {
                    w.press("e", cx); // Retained failures are inspectable after reconnect.
                    assert!(shell.read(cx).comparison.read(cx).errors_visible_for_test());
                    w.press("escape", cx);
                    assert!(shell.read(cx).comparison.read(cx).visible());
                    assert!(!shell.read(cx).comparison.read(cx).errors_visible_for_test());
                    w.press("escape", cx);
                })
                .unwrap();
                assert!(!shell.read_with(cx, |s, cx| s.comparison.read(cx).visible())); // Focus returned to the visible report.
            }
            _ => unreachable!(),
        }
    }
}
