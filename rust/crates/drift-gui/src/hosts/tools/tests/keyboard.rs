use super::*;

fn approve_permanently(
    handle: AnyWindowHandle,
    tools: &Entity<HostTools>,
    cx: &mut TestAppContext,
) {
    let prompt = tools.read_with(cx, |tools, _| tools.certificate.clone().unwrap());
    let permanent = Rc::new(Cell::new(false));
    let _subscription = cx.update(|cx| {
        cx.subscribe(&prompt, {
            let permanent = permanent.clone();
            move |_, decision: &Decision, _| {
                assert!(matches!(decision, Decision::Permanent));
                permanent.set(true);
            }
        })
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(prompt.read(cx).focus_handle(cx).is_focused(w));
        assert_eq!(
            w.find("certificate-permanent").label(),
            Some("Trust permanently")
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("right", cx);
        w.press("right", cx);
        w.press("enter", cx);
    })
    .unwrap();
    // Emitted decisions flush after the update; no foreground task has been polled yet.
    // Button selection is styling-only, so observe the real decision, not private prompt state.
    assert!(
        permanent.get(),
        "keyboard must choose Permanent, not Session"
    );
}

#[gpui_kit::test]
async fn busy_permanent_approval_rejects_by_escape_and_keyboard_without_retry(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for key in ["escape", "enter"] {
        let server = Server::tls("valid");
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        let stored = server.host();
        store.save_host(None, None, stored.clone()).unwrap();
        let mut draft = stored.clone();
        draft.name = "Unsaved initiating draft".into();
        let manager = Arc::new(Manager::with_roots(
            store.clone(),
            rustls::RootCertStore::empty(),
        ));
        let (handle, tools) = open_tools(
            store.clone(),
            RemoteService::with_options(BrowserService::new().unwrap(), server.options())
                .with_trust(manager.clone()),
            draft.clone(),
            cx,
        );
        idle(handle, &tools, cx).await;
        let prompt = tools.read_with(cx, |tools, _| tools.certificate.clone().unwrap());
        let challenge = prompt.read_with(cx, |prompt, _| prompt.challenge.clone());
        let closed = Rc::new(Cell::new(false));
        let _subscription = cx.update(|cx| {
            let closed = closed.clone();
            cx.subscribe(&tools, move |_, _: &Closed, _| closed.set(true))
        });
        let lock = fs::File::open(config.path().join("write.lock")).unwrap();
        lock.lock_exclusive().unwrap();
        approve_permanently(handle, &tools, cx);
        let (operation, cancel) = tools.read_with(cx, |tools, cx| {
            assert!(tools.writing);
            assert!(prompt.read(cx).busy);
            let cancel = tools.cancel.clone().unwrap();
            assert!(!cancel.is_cancelled());
            (tools.operation, cancel)
        });
        cx.update_window(handle, |_, w, cx| {
            // Store contention fails without waiting; its foreground result stays queued here.
            w.press("enter", cx);
            assert_eq!(tools.read(cx).operation, operation);
            assert!(tools.read(cx).writing);
            assert!(prompt.read(cx).busy);
            assert!(!cancel.is_cancelled());
            if key == "enter" {
                w.press("right", cx); // Permanent -> Reject.
                assert_eq!(w.find("certificate-reject").label(), Some("Reject"));
            }
            assert!(tools.read(cx).cancel.is_some());
            assert!(tools.read(cx).writing);
            w.press(key, cx);
        })
        .unwrap();
        assert!(cancel.is_cancelled());
        tools.read_with(cx, |tools, _| {
            assert_eq!(tools.operation, operation + 1);
            assert!(tools.cancel.is_none());
            assert!(!tools.writing);
            assert!(tools.certificate.is_none());
            assert!(tools.subscription.is_none());
            assert!(tools.host == draft);
            assert!(tools.status.contains("already saved trust is not undone"));
        });
        cx.update_window(handle, |_, w, cx| {
            assert!(tools.read(cx).focus.is_focused(w))
        })
        .unwrap();
        FileExt::unlock(&lock).unwrap();
        // A real subsequent inspection must not be taken over by the queued trust result.
        cx.update_window(handle, |_, w, cx| {
            tools.update(cx, |tools, cx| tools.inspect(w, cx));
        })
        .unwrap();
        idle(handle, &tools, cx).await;
        prompt.update(cx, |_, cx| cx.emit(Decision::Permanent));
        cx.run_until_parked();
        tools.read_with(cx, |tools, _| {
            assert!(tools.host == draft);
            assert_eq!(tools.status, "Review certificate trust before resetting");
            assert!(tools.certificate.is_none());
        });
        assert!(!closed.get());
        assert_eq!(server.commands("PASS"), 0);
        assert_eq!(
            server.commands("AUTH"),
            1,
            "Reject must not start another test"
        );
        assert!(store.trusted_certificates().unwrap().is_empty());
        assert!(
            manager
                .inspect(challenge.endpoint)
                .unwrap()
                .session
                .is_none()
        );
        assert!(store.global().unwrap().hosts == vec![stored]);
    }
}

#[gpui_kit::test]
async fn rejecting_a_committed_approval_discards_success_without_rolling_back_trust(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = Server::tls("valid");
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let draft = server.host();
    let (handle, tools) = open_tools(
        store.clone(),
        RemoteService::with_options(BrowserService::new().unwrap(), server.options()).with_trust(
            Arc::new(Manager::with_roots(
                store.clone(),
                rustls::RootCertStore::empty(),
            )),
        ),
        draft.clone(),
        cx,
    );
    idle(handle, &tools, cx).await;
    let expected = tools.read_with(cx, |tools, cx| {
        tools
            .certificate
            .as_ref()
            .unwrap()
            .read(cx)
            .challenge
            .clone()
    });
    approve_permanently(handle, &tools, cx);
    let (operation, cancel) = tools.read_with(cx, |tools, _| {
        (tools.operation, tools.cancel.clone().unwrap())
    });
    let committed = cx
        .update_window(handle, |_, w, cx| {
            // Only the real worker runs here, not GPUI's queued completion callback.
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !config.path().join("trusted-certificates.toml").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "trust write did not commit"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(tools.read(cx).writing);
            assert!(tools.read(cx).certificate.as_ref().unwrap().read(cx).busy);
            assert!(!cancel.is_cancelled());
            let entries = store.trusted_certificates().unwrap();
            assert_eq!(entries.len(), 1);
            let committed = entries.into_iter().next().unwrap();
            assert_eq!(committed.endpoint(), expected.endpoint);
            assert_eq!(committed.fingerprint, expected.fingerprint);
            assert_eq!(committed.problems, expected.problems);
            w.press("escape", cx);
            committed
        })
        .unwrap();
    assert!(cancel.is_cancelled());
    tools.read_with(cx, |tools, _| {
        assert_eq!(tools.operation, operation + 1);
        assert!(!tools.writing);
        assert!(tools.cancel.is_none());
        assert!(tools.certificate.is_none());
        assert!(tools.status.contains("already saved trust is not undone"));
    });
    cx.update_window(handle, |_, w, cx| {
        tools.update(cx, |tools, cx| tools.inspect(w, cx));
    })
    .unwrap();
    idle(handle, &tools, cx).await;
    cx.run_until_parked();
    tools.read_with(cx, |tools, _| {
        assert!(tools.host == draft);
        assert_eq!(tools.status, "Review certificate trust before resetting");
        assert_eq!(
            tools.reset.as_ref().unwrap().persistent.as_ref(),
            Some(&committed)
        );
        assert!(tools.certificate.is_none());
    });
    assert_eq!(store.trusted_certificates().unwrap(), vec![committed]);
    assert_eq!(server.commands("AUTH"), 1);
    assert_eq!(
        server.commands("PASS"),
        0,
        "stale approval success must not retry"
    );
}

#[gpui_kit::test]
async fn trust_reset_enter_preserves_native_buttons_and_default_and_y_confirmation(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for target in ["reload", "test", "return", "reset", "default", "y"] {
        let server = Server::tls("valid");
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        let (handle, tools) = open_tools(
            store.clone(),
            RemoteService::with_options(BrowserService::new().unwrap(), server.options())
                .with_trust(Arc::new(Manager::with_roots(
                    store.clone(),
                    rustls::RootCertStore::empty(),
                ))),
            server.host(),
            cx,
        );
        idle(handle, &tools, cx).await;
        approve_permanently(handle, &tools, cx);
        idle(handle, &tools, cx).await;
        cx.update_window(handle, |_, w, cx| {
            tools.update(cx, |tools, cx| tools.inspect(w, cx));
        })
        .unwrap();
        idle(handle, &tools, cx).await;
        let before = server.commands("PASS");
        let closed = Rc::new(Cell::new(false));
        let _subscription = cx.update(|cx| {
            let closed = closed.clone();
            cx.subscribe(&tools, move |_, _: &Closed, _| closed.set(true))
        });
        let (tabs, button) = match target {
            "reset" => (1, "host-trust-reset-confirm"),
            "reload" => (2, "host-trust-reload"),
            "test" => (3, "host-test-again"),
            "return" | "y" => (4, "host-tools-close"),
            _ => (0, ""),
        };
        cx.update_window(handle, |_, w, cx| {
            tools.read(cx).focus.clone().focus(w, cx);
            for _ in 0..tabs {
                w.press("tab", cx);
            }
            if tabs > 0 {
                assert_eq!(w.find(button).focused(), Some(true), "{target}");
            }
            let operation = tools.read(cx).operation;
            w.press(if target == "y" { "y" } else { "enter" }, cx);
            if target != "return" {
                assert_eq!(tools.read(cx).operation, operation + 1, "{target}");
                // This update does not yield to the foreground completion callback.
                let cancel = tools.read(cx).cancel.clone().unwrap();
                for key in ["y", "r", "enter"] {
                    assert!(tools.read(cx).cancel.is_some(), "{target}: {key}");
                    assert!(!cancel.is_cancelled(), "{target}: {key}");
                    assert_eq!(
                        tools.read(cx).writing,
                        matches!(target, "reset" | "default" | "y"),
                        "{target}: {key}"
                    );
                    w.press(key, cx);
                    assert_eq!(tools.read(cx).operation, operation + 1, "{target}: {key}");
                    assert!(tools.read(cx).cancel.is_some(), "{target}: {key}");
                }
            }
        })
        .unwrap();
        idle(handle, &tools, cx).await;
        assert_eq!(closed.get(), target == "return");
        assert_eq!(
            store.trusted_certificates().unwrap().is_empty(),
            matches!(target, "reset" | "default" | "y"),
            "{target}"
        );
        match target {
            "reload" => assert_eq!(
                tools.read_with(cx, |tools, _| tools.status.clone()),
                "Review certificate trust before resetting"
            ),
            "test" => assert!(tools.read_with(cx, |tools, _| tools.status.contains("successful"))),
            _ => {}
        }
        assert_eq!(
            server.commands("PASS"),
            // A fresh FTP client logs in each of its four pooled control connections.
            before + 4 * usize::from(target == "test"),
            "{target}"
        );
    }
}

#[gpui_kit::test]
async fn real_sftp_host_test_button_enter_retests_without_trust_reset(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = crate::sftp_test_support::Server::new(false);
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let draft = server.host();
    let (handle, tools) = open_tools(
        store.clone(),
        RemoteService::with_options(BrowserService::new().unwrap(), server.options()),
        draft.clone(),
        cx,
    );
    idle(handle, &tools, cx).await;
    cx.update_window(handle, |_, w, cx| {
        tools.read(cx).focus.clone().focus(w, cx);
        w.press("tab", cx);
        assert_eq!(w.find("host-test-again").focused(), Some(true));
        let operation = tools.read(cx).operation;
        w.press("enter", cx);
        assert_eq!(tools.read(cx).operation, operation + 1);
    })
    .unwrap();
    idle(handle, &tools, cx).await;
    tools.read_with(cx, |tools, _| {
        assert!(tools.host == draft);
        assert!(tools.status.contains("successful"), "{}", tools.status);
    });
    assert!(store.trusted_certificates().unwrap().is_empty());
    assert!(!config.path().join("config.toml").exists());
}
