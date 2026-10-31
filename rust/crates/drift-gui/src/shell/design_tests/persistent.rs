use super::*;

#[gpui_kit::test]
async fn persistent_toolbar_switches_management_lists_preserving_the_browser(
    cx: &mut TestAppContext,
) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("keep.txt"), "unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.register("Review", local.path().into()).unwrap();
    let f = Fixture::new(1200., 900., store, local.path(), None, cx).await;
    let original = cx
        .update_window(f.handle, |_, w, cx| {
            w.press("home", cx);
            w.press("space", cx);
            let shell = f.shell.read(cx);
            let state = (
                shell.browser.clone(),
                (shell.local_preview.clone(), shell.remote_preview.clone()),
                shell.remote.clone(),
                shell.browser.read(cx).id(),
            );
            w.click("projects", cx);
            state
        })
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(f.shell.read(cx).projects.read(cx).visible());
        let toolbar = w.find("workspace-toolbar").bounds();
        assert_eq!(toolbar.top(), px(0.));
        for (id, label) in CONTROLS {
            assert_eq!(w.find(id).label(), Some(label));
            assert!(w.find(id).visible());
            assert!(w.find(id).bounds().bottom() <= toolbar.bottom());
        }
        assert!(w.find("project-close").bounds().top() >= toolbar.bottom());
        toolbar_scopes::assert_browser_scopes_absent(w);
        w.click("home", cx);
        w.click("hosts", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell
            .read(cx)
            .hosts
            .as_ref()
            .is_some_and(|hosts| hosts.read(cx).can_leave())
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!f.shell.read(cx).projects.read(cx).visible());
        assert!(
            w.find("hosts-close").bounds().top() >= w.find("workspace-toolbar").bounds().bottom()
        );
        toolbar_scopes::assert_browser_scopes_absent(w);
        w.click("home", cx);
        assert!(f.shell.read(cx).hosts.is_none());
        w.click("projects", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("home", cx);
        w.click("local-files-tab", cx);
        let shell = f.shell.read(cx);
        assert!(shell.browser_screen_active(cx));
        assert_eq!(shell.browser, original.0);
        assert_eq!(
            (shell.local_preview.clone(), shell.remote_preview.clone()),
            original.1
        );
        assert_eq!(shell.remote, original.2);
        assert_eq!(shell.browser.read(cx).id(), original.3);
        assert_eq!(shell.browser.read(cx).marked(), ["keep.txt"]);
        assert!(shell.browser.focus_handle(cx).is_focused(w));
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(local.path().join("keep.txt")).unwrap(),
        "unchanged"
    );
}

#[gpui_kit::test]
async fn persistent_toolbar_cannot_discard_forms_or_act_behind_help(cx: &mut TestAppContext) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("keep.txt"), "unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.register("Review", local.path().into()).unwrap();
    let f = Fixture::new(1200., 900., store.clone(), local.path(), None, cx).await;
    cx.update_window(f.handle, |_, w, cx| w.click("projects", cx))
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("project-new", cx);
        w.input("Draft ä Ω", cx);
        let browser_id = f.shell.read(cx).browser.read(cx).id();
        assert_eq!(w.find("project-edit-name").value(), Some("Draft ä Ω"));
        toolbar_scopes::assert_browser_scopes_absent(w);
        for (id, _) in CONTROLS {
            w.click(id, cx);
        }
        for event in [
            ToolbarEvent::Projects,
            ToolbarEvent::Hosts,
            ToolbarEvent::Home,
            ToolbarEvent::Browser(BrowserCommand::Refresh),
            ToolbarEvent::Browser(BrowserCommand::Hidden),
        ] {
            f.shell
                .update(cx, |shell, cx| shell.toolbar_event(&event, w, cx));
        }
        assert!(f.shell.read(cx).projects.read(cx).visible());
        assert!(f.shell.read(cx).hosts.is_none());
        assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
        assert_eq!(w.find("project-edit-name").value(), Some("Draft ä Ω"));
        w.press("f1", cx);
        assert!(f.shell.read(cx).help.is_some());
        toolbar_scopes::assert_browser_scopes_absent(w);
        for (id, _) in CONTROLS {
            assert!(w.find(id).visible());
            w.click(id, cx);
        }
        assert!(f.shell.read(cx).help.is_some());
        w.press("escape", cx);
        assert_eq!(w.find("project-edit-name").value(), Some("Draft ä Ω"));
        w.click("project-close", cx);
        assert!(w.try_find("project-edit-name").is_none());
        w.click("home", cx);
        w.click("hosts", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell
            .read(cx)
            .hosts
            .as_ref()
            .is_some_and(|hosts| hosts.read(cx).can_leave())
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("host-new", cx);
        w.input("Unsaved host", cx);
        for (id, _) in CONTROLS {
            w.click(id, cx);
        }
        assert!(f.shell.read(cx).hosts.is_some());
        assert!(!f.shell.read(cx).projects.read(cx).visible());
        assert_eq!(w.find("host-name").value(), Some("Unsaved host"));
        toolbar_scopes::assert_browser_scopes_absent(w);
        w.press("escape", cx);
        assert!(w.try_find("host-name").is_none());
        w.click("home", cx);
        w.click("local-files-tab", cx);
        assert!(f.shell.read(cx).browser_screen_active(cx));
    })
    .unwrap();
    assert_eq!(store.registry().unwrap().projects.len(), 1);
    assert!(store.runtime(Some("review")).unwrap().hosts.is_empty());
    assert_eq!(
        fs::read_to_string(local.path().join("keep.txt")).unwrap(),
        "unchanged"
    );
}

#[gpui_kit::test]
async fn persistent_toolbar_preserves_comparison_and_never_bypasses_pending_consent(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("sample.txt"), "local contents").unwrap();
    fs::write(
        server.dir.path().join("files/sample.txt"),
        "remote contents",
    )
    .unwrap();
    let store = Store::new(config.path().into());
    store.register("Review", local.path().into()).unwrap();
    store
        .save_host(Some("review"), None, server.host())
        .unwrap();
    let f = Fixture::new(
        1400.,
        1000.,
        store,
        local.path(),
        Some(server.options()),
        cx,
    )
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote-files-tab", cx);
        crate::test_support::connect_first(w, cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).remote.read(cx).has_session()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).comparison.read(cx).visible()
            && !f.shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    let (comparison, session_id, browser_id) = f.shell.read_with(cx, |shell, cx| {
        (
            shell.comparison.clone(),
            shell.remote.read(cx).session_id(),
            shell.browser.read(cx).id(),
        )
    });
    cx.update_window(f.handle, |_, w, cx| {
        assert!(w.find("workspace-toolbar").visible());
        w.click("projects", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("projects", cx);
        assert!(f.shell.read(cx).comparison.focus_handle(cx).is_focused(w));
        toolbar_scopes::assert_browser_scopes_absent(w);
        for event in [
            ToolbarEvent::Hosts,
            ToolbarEvent::Browser(BrowserCommand::Refresh),
        ] {
            f.shell
                .update(cx, |shell, cx| shell.toolbar_event(&event, w, cx));
        }
        assert!(f.shell.read(cx).hosts.is_none());
        assert!(f.shell.read(cx).comparison.focus_handle(cx).is_focused(w));
        w.click("sync-all", cx);
        assert!(!f.shell.read(cx).comparison.read(cx).can_leave());
        for (id, _) in CONTROLS {
            w.click(id, cx);
        }
        assert!(w.find("sync-confirm").visible());
        toolbar_scopes::assert_browser_scopes_absent(w);
        assert!(f.shell.read(cx).hosts.is_none());
        assert!(!f.shell.read(cx).projects.read(cx).visible());
        assert_eq!(f.shell.read(cx).comparison, comparison);
        assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), session_id);
        assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
        assert!(!f.shell.read(cx).comparison.read(cx).is_loading());
        w.click("sync-dismiss", cx);
        assert!(f.shell.read(cx).comparison.read(cx).can_leave());
        w.click("home", cx);
        assert!(f.shell.read(cx).browser_screen_active(cx));
        assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), session_id);
        assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
    })
    .unwrap();
    let connection = f.shell.read_with(cx, |shell, cx| {
        shell.remote.read(cx).comparison_target().unwrap().1
    });
    connection.shutdown().await;
    assert_eq!(
        fs::read_to_string(local.path().join("sample.txt")).unwrap(),
        "local contents"
    );
    assert_eq!(
        fs::read_to_string(server.dir.path().join("files/sample.txt")).unwrap(),
        "remote contents"
    );
}
