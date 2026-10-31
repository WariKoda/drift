use super::*;
use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent};

pub(super) fn assert_workspace_toolbar_scopes(w: &mut Window) {
    let global = w.find("workspace-toolbar").bounds();
    let local_slot = w.find("local-aligned-tabs").bounds();
    let remote_slot = w.find("remote-aligned-tabs").bounds();
    assert_eq!(local_slot.top(), remote_slot.top());
    assert_eq!(local_slot.bottom(), remote_slot.bottom());
    let natural = w
        .find("local-pane-tabs")
        .bounds()
        .size
        .height
        .max(w.find("remote-pane-tabs").bounds().size.height);
    // The slot adds its 1px bottom border to the taller row, rounded up to whole pixels.
    let blank = local_slot.size.height - px(1.) - natural;
    assert!(
        blank >= px(0.) && blank < px(1.),
        "tabs slot must follow the taller natural row: slot={local_slot:?}, natural={natural:?}"
    );
    for (workspace_id, side, slot_id, nav, filter) in [
        (
            "local-workspace",
            "split-first",
            "local-aligned-tabs",
            "local-navigation-toolbar",
            "local-browser-header",
        ),
        (
            "remote-workspace",
            "split-second",
            "remote-aligned-tabs",
            "remote-navigation-toolbar",
            "remote-browser-header",
        ),
    ] {
        let pane = w.within("browser-split").find(side).bounds();
        let workspace = w.find(workspace_id).bounds();
        assert!(workspace.left() >= pane.left() && workspace.right() <= pane.right());
        assert!(workspace.top() >= global.bottom());
        let navigation = w.within(workspace_id).find(nav).bounds();
        let header = w.within(workspace_id).find(filter).bounds();
        for bounds in [navigation, header] {
            assert!(
                bounds.left() >= workspace.left() && bounds.right() <= workspace.right(),
                "{workspace_id}: {bounds:?} outside {workspace:?}"
            );
            assert!(bounds.top() >= workspace.top() && bounds.bottom() <= workspace.bottom());
        }
        let tabs_id = if workspace_id == "local-workspace" {
            "local-pane-tabs"
        } else {
            "remote-pane-tabs"
        };
        let slot = w.within(workspace_id).find(slot_id).bounds();
        assert_eq!(slot.top(), workspace.top());
        assert!(slot.left() >= workspace.left() && slot.right() <= workspace.right());
        let tabs = w.within(slot_id).find(tabs_id).bounds();
        assert_eq!(tabs.top(), slot.top(), "{tabs_id} starts the slot");
        assert!(tabs.left() >= slot.left() && tabs.right() <= slot.right());
        assert!(
            tabs.bottom() <= slot.bottom() - px(1.),
            "{tabs_id} must stay above the slot border: {tabs:?} in {slot:?}"
        );
        assert_eq!(navigation.top(), slot.bottom());
        assert_eq!(header.top(), navigation.bottom());
    }
    let remote_workspace = w.find("remote-workspace").bounds();
    let tabs = w
        .within("remote-workspace")
        .find("remote-pane-tabs")
        .bounds();
    let connection = w
        .within("remote-pane-tabs")
        .find("remote-connection-controls")
        .bounds();
    let select = w
        .within("remote-connection-controls")
        .find("remote-host-select")
        .bounds();
    let disconnect = w
        .within("remote-connection-controls")
        .find("disconnect")
        .bounds();
    // Horizontal containment in narrow panes is checked by
    // `remote_connection_row_fits_narrow_panes_with_real_sftp`.
    assert!(connection.left() >= remote_workspace.left());
    for (id, bounds) in [
        ("remote-connection-controls", connection),
        ("remote-host-select", select),
        ("disconnect", disconnect),
    ] {
        assert!(
            bounds.top() >= tabs.top() && bounds.bottom() <= tabs.bottom(),
            "{id}: {bounds:?} outside tabs {tabs:?}"
        );
    }
    assert!(
        select.right() <= disconnect.left(),
        "host select precedes disconnect: {select:?}, {disconnect:?}"
    );
    let footer = w.find("workspace-footer").bounds();
    assert!(footer.top() >= w.find("browser-split").bounds().bottom());
    assert!(
        w.within("workspace-footer")
            .try_find("workspace-compare")
            .is_some()
    );
    for scope in ToolbarScope::ALL {
        let header = w.find(scope.id()).bounds();
        for &(id, label) in scope.controls() {
            let item = w.within(scope.id()).find(id);
            assert_eq!(item.label(), Some(label));
            let bounds = item.bounds();
            assert!(
                bounds.top() >= header.top() && bounds.bottom() <= header.bottom(),
                "{id}: {bounds:?}"
            );
            // Local navigation is a single scrolling row, not a wrapping action group.
            if scope != ToolbarScope::Local {
                assert!(item.visible(), "{id}");
                assert!(
                    bounds.left() >= header.left() && bounds.right() <= header.right(),
                    "{id}: {bounds:?}, header: {header:?}"
                );
            }
            for other in ToolbarScope::ALL {
                if other != scope {
                    assert!(
                        w.within(other.id()).try_find(id).is_none(),
                        "{id} in {}",
                        other.id()
                    );
                }
            }
            for pane in ["local-workspace", "remote-workspace"] {
                if scope != ToolbarScope::Local || pane != "local-workspace" {
                    assert!(w.within(pane).try_find(id).is_none(), "{id} in {pane}");
                }
            }
        }
    }
    let nav = w.find("remote-navigation-toolbar").bounds();
    for (id, label) in workspace_headers::REMOTE_NAV_CONTROLS {
        let item = w.within("remote-navigation-toolbar").find(id);
        assert_eq!(item.label(), Some(label));
        assert!(item.bounds().top() >= nav.top() && item.bounds().bottom() <= nav.bottom());
        for scope in ToolbarScope::ALL {
            assert!(
                w.within(scope.id()).try_find(id).is_none(),
                "{id} in {}",
                scope.id()
            );
        }
    }
    assert!(
        w.within("remote-navigation-toolbar")
            .within("remote-cancel-slot")
            .try_find("remote-cancel")
            .is_some()
    );
}

pub(in crate::shell) fn assert_browser_scopes_absent(w: &mut Window) {
    for id in [
        "local-workspace",
        "remote-workspace",
        "local-pane-tabs",
        "remote-pane-tabs",
        "local-files-tab",
        "local-preview-tab",
        "remote-files-tab",
        "remote-preview-tab",
        "local-navigation-toolbar",
        "remote-navigation-toolbar",
        "local-browser-header",
        "remote-browser-header",
        "remote-connection-controls",
        "remote-host-select",
        "remote-cancel-slot",
        "workspace-footer",
        "workspace-compare",
        "filter",
        "remote-filter",
    ] {
        assert!(
            w.try_find(id).is_none(),
            "{id} must be absent over a covered browser"
        );
    }
    for (id, _) in LOCAL_CONTROLS
        .into_iter()
        .chain(COMPARE_CONTROLS)
        .chain(workspace_headers::REMOTE_NAV_CONTROLS)
    {
        assert!(
            w.try_find(id).is_none(),
            "{id} must be absent over a covered browser"
        );
    }
    for (id, label) in CONTROLS {
        assert_eq!(w.within("workspace-toolbar").find(id).label(), Some(label));
        assert!(w.find(id).visible(), "{id}");
    }
}

#[gpui_kit::test]
async fn remote_workspace_header_survives_preview_scaling_and_management_with_real_sftp(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote_file = server.dir.path().join("files/remote.txt");
    fs::write(&remote_file, "remote unchanged").unwrap();
    fs::write(local.path().join("local.txt"), "local unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
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
        assert_eq!(f.shell.read(cx).local_view, PaneView::Files);
        assert_eq!(f.shell.read(cx).remote_view, PaneView::Files);
        for (width, font) in [(1400., 16.), (1400., 24.), (640., 24.), (420., 24.)] {
            w.resize(size(px(width), px(1600.)));
            w.bounds_changed(cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            crate::test_support::settle(w, cx);
            assert_workspace_toolbar_scopes(w);
            let pane = w.within("remote-workspace").find("remote-pane").bounds();
            assert!(pane.top() >= w.find("remote-aligned-filter").bounds().bottom());
            assert!(pane.size.height > px(0.));
            assert!(
                w.within("remote-workspace")
                    .try_find("preview-header")
                    .is_none()
            );
        }
        w.resize(size(px(1400.), px(1000.)));
        w.bounds_changed(cx);
        Theme::update(cx, |theme| theme.font_size = px(16.));
        w.click("remote-files-tab", cx);
        crate::test_support::connect_first(w, cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).remote.read(cx).has_session()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    let (session, connection, browser_id, marks) = cx
        .update_window(f.handle, |_, w, cx| {
            let remote = f.shell.read(cx).remote.clone();
            remote.focus_handle(cx).focus(w, cx);
            w.press("home", cx);
            w.press("space", cx);
            let session = remote.read(cx).session_id();
            let connection = remote.read(cx).connection();
            let browser_id = f.shell.read(cx).browser.read(cx).id();
            let marks = remote.read(cx).marked();
            assert_eq!(marks.len(), 1);
            for (width, font) in [(1400., 16.), (1400., 24.), (640., 24.), (420., 24.)] {
                w.resize(size(px(width), px(1600.)));
                w.bounds_changed(cx);
                Theme::update(cx, |theme| theme.font_size = px(font));
                crate::test_support::settle(w, cx);
                assert_workspace_toolbar_scopes(w);
                let pane = w.within("remote-workspace").find("remote-pane").bounds();
                assert!(pane.top() >= w.find("remote-aligned-filter").bounds().bottom());
                assert!(pane.size.height > px(0.));
                assert_eq!(remote.read(cx).session_id(), session);
                assert_eq!(remote.read(cx).connection(), connection);
                assert_eq!(remote.read(cx).marked(), marks);
                assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
            }
            w.resize(size(px(1400.), px(1000.)));
            w.bounds_changed(cx);
            Theme::update(cx, |theme| theme.font_size = px(16.));
            w.click("remote-preview-tab", cx);
            (session, connection, browser_id, marks)
        })
        .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(f.shell.read(cx).remote_view, PaneView::Preview);
        assert_workspace_toolbar_scopes(w);
        assert!(
            w.within("remote-workspace")
                .find("preview-header")
                .visible()
        );
        assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), session);
        assert_eq!(f.shell.read(cx).remote.read(cx).connection(), connection);
        assert_eq!(f.shell.read(cx).remote.read(cx).marked(), marks);
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
        assert!(w.try_find("remote-workspace").is_none());
        assert_browser_scopes_absent(w);
        w.click("home", cx);
        w.render_frame(cx);
        assert_workspace_toolbar_scopes(w);
        assert_eq!(f.shell.read(cx).remote_view, PaneView::Files);
        w.click("remote-files-tab", cx);
        w.render_frame(cx);
        assert_workspace_toolbar_scopes(w);
        assert!(w.within("remote-workspace").find("remote-pane").visible());
        assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), session);
        assert_eq!(f.shell.read(cx).remote.read(cx).connection(), connection);
        assert_eq!(f.shell.read(cx).remote.read(cx).marked(), marks);
        assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
        assert!(!f.shell.read(cx).comparison.read(cx).visible());
    })
    .unwrap();
    let connection = f.shell.read_with(cx, |shell, cx| {
        shell.remote.read(cx).comparison_target().unwrap().1
    });
    connection.shutdown().await;
    assert_eq!(fs::read_to_string(remote_file).unwrap(), "remote unchanged");
    assert_eq!(
        fs::read_to_string(local.path().join("local.txt")).unwrap(),
        "local unchanged"
    );
    assert!(!local.path().join("remote.txt").exists());
    assert!(!server.dir.path().join("files/local.txt").exists());
}

#[gpui_kit::test]
async fn remote_header_rejects_old_pointer_release_across_settings_and_projects(
    cx: &mut TestAppContext,
) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("keep.txt"), "unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.register("Review", local.path().into()).unwrap();
    let f = Fixture::new(1400., 1000., store, local.path(), None, cx).await;
    for settings in [true, false] {
        for round_trip in [false, true] {
            for id in ["remote-files-tab", "hosts"] {
                // The global Hosts button stays live over Settings and after a round trip;
                // only the Projects screen has to reject its old release.
                if id == "hosts" && (settings || round_trip) {
                    continue;
                }
                cx.update_window(f.handle, |_, w, cx| {
                    w.render_frame(cx);
                    let position = w
                        .within(if id == "hosts" {
                            "workspace-toolbar"
                        } else {
                            "remote-pane-tabs"
                        })
                        .find(id)
                        .bounds()
                        .center();
                    let browser_id = f.shell.read(cx).browser.read(cx).id();
                    let connection = f.shell.read(cx).remote.read(cx).connection();
                    w.dispatch_event(
                        MouseDownEvent {
                            button: MouseButton::Left,
                            position,
                            modifiers: Default::default(),
                            click_count: 1,
                            first_mouse: false,
                        }
                        .to_platform_input(),
                        cx,
                    );
                    // Transition without repaint: the release still targets the old native button.
                    f.shell.update(cx, |shell, cx| {
                        let event = if settings {
                            ToolbarEvent::Settings
                        } else {
                            ToolbarEvent::Projects
                        };
                        shell.toolbar_event(&event, w, cx);
                        if round_trip {
                            if settings {
                                shell.toolbar_event(&event, w, cx);
                            } else {
                                // Use the real Close projects handler, which also permits
                                // cancelling its pending load and respects form/write guards.
                                shell.projects.update(cx, |projects, cx| {
                                    projects.cancel_view(&Cancel, w, cx)
                                });
                            }
                        }
                    });
                    let focus = w.focused(cx);
                    w.dispatch_event(
                        MouseUpEvent {
                            button: MouseButton::Left,
                            position,
                            modifiers: Default::default(),
                            click_count: 1,
                        }
                        .to_platform_input(),
                        cx,
                    );
                    assert_eq!(
                        f.shell.read(cx).remote_view,
                        PaneView::Files,
                        "stale {id}: settings={settings}, round_trip={round_trip}"
                    );
                    assert!(
                        f.shell.read(cx).hosts.is_none(),
                        "stale {id}: settings={settings}, round_trip={round_trip}"
                    );
                    assert_eq!(w.focused(cx), focus);
                    assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
                    assert_eq!(f.shell.read(cx).remote.read(cx).connection(), connection);
                    if !round_trip {
                        w.render_frame(cx);
                        assert_browser_scopes_absent(w);
                    } else {
                        assert!(
                            f.shell.read(cx).browser_screen_active(cx),
                            "{id}: settings={settings}, round_trip={round_trip}"
                        );
                    }
                })
                .unwrap();
                cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
                    !f.shell.read(cx).projects.read(cx).is_loading()
                })
                .await;
                cx.update_window(f.handle, |_, w, cx| {
                    if !round_trip {
                        if settings {
                            w.click("settings-close", cx);
                        } else {
                            w.click("home", cx);
                        }
                    }
                    assert!(f.shell.read(cx).browser_screen_active(cx));
                })
                .unwrap();
            }
        }
    }
    assert_eq!(
        fs::read_to_string(local.path().join("keep.txt")).unwrap(),
        "unchanged"
    );
}

#[gpui_kit::test]
async fn scoped_cancel_buttons_do_not_cancel_the_other_real_browser_operation(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("local.txt"), "local unchanged").unwrap();
    let remote_file = server.dir.path().join("files/remote.txt");
    fs::write(&remote_file, "remote unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
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
    for (id, local_loading, remote_loading) in [
        ("cancel", false, true),
        ("remote-cancel", true, false),
        ("cancel", true, true),
        ("remote-cancel", true, true),
        ("ctrl-escape", true, true),
    ] {
        if !f
            .shell
            .read_with(cx, |shell, cx| shell.remote.read(cx).has_session())
        {
            cx.update_window(f.handle, |_, w, cx| {
                crate::test_support::connect_first(w, cx)
            })
            .unwrap();
            cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
                f.shell.read(cx).remote.read(cx).has_session()
                    && !f.shell.read(cx).remote.read(cx).is_loading()
            })
            .await;
        }
        let session = f
            .shell
            .read_with(cx, |shell, cx| shell.remote.read(cx).session_id());
        cx.update_window(f.handle, |_, w, cx| {
            let browser = f.shell.read(cx).browser.clone();
            let remote = f.shell.read(cx).remote.clone();
            if local_loading {
                browser.update(cx, |browser, cx| browser.refresh(&Refresh, w, cx));
            }
            if remote_loading {
                remote.update(cx, |remote, cx| remote.refresh(&Refresh, w, cx));
            }
            assert_eq!(browser.read(cx).is_loading(), local_loading);
            assert_eq!(remote.read(cx).is_loading(), remote_loading);
            let browser_id = browser.read(cx).id();
            // The real commands are queued, not replaced with fabricated in-flight state.
            w.render_frame(cx);
            if id == "ctrl-escape" {
                browser.focus_handle(cx).focus(w, cx);
                w.press(id, cx);
            } else {
                w.click(id, cx);
            }
            assert_eq!(
                browser.read(cx).is_loading(),
                local_loading && id == "remote-cancel",
                "{id}"
            );
            assert_eq!(
                remote.read(cx).is_loading(),
                remote_loading && id == "cancel",
                "{id}"
            );
            if id == "remote-cancel" {
                assert_eq!(browser.read(cx).id(), browser_id);
            }
            if id == "cancel" || !remote_loading {
                assert_eq!(remote.read(cx).session_id(), session);
            } else {
                assert!(!remote.read(cx).has_session());
            }
        })
        .unwrap();
        cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
            !f.shell.read(cx).browser.read(cx).is_loading()
                && !f.shell.read(cx).remote.read(cx).is_loading()
        })
        .await;
    }
    assert!(
        !f.shell
            .read_with(cx, |shell, cx| shell.remote.read(cx).has_session())
    );
    assert_eq!(
        fs::read_to_string(local.path().join("local.txt")).unwrap(),
        "local unchanged"
    );
    assert_eq!(fs::read_to_string(remote_file).unwrap(), "remote unchanged");
}
