use super::*;

#[gpui_kit::test]
async fn settings_theme_choices_save_and_preserve_the_real_browser_session(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    let (before, focus) = cx
        .update_window(f.handle, |_, w, cx| {
            w.click("local-files-tab", cx);
            f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
            w.press("home", cx);
            w.press("space", cx);
            let before = browser_state(f.shell.read(cx), cx);
            let focus = w.focused(cx);
            w.click("settings", cx);
            assert!(w.find("settings-view").visible());
            assert!(w.find("workspace-toolbar").visible());
            (before, focus)
        })
        .unwrap();
    for (choice, expected) in [("Dark", ThemeMode::Dark), ("Light", ThemeMode::Light)] {
        crate::test_support::theme(f.handle, choice, cx);
        cx.update_window(f.handle, |_, _, cx| {
            assert_eq!(cx.theme().mode, expected);
            assert_eq!(browser_state(f.shell.read(cx), cx), before);
        })
        .unwrap();
    }
    cx.update_window(f.handle, |_, w, cx| {
        w.press("f1", cx);
        assert!(f.shell.read(cx).help.is_some());
        w.press("escape", cx);
        assert!(f.shell.read(cx).settings.is_some());
        assert!(w.find("settings-view").visible());
        w.click("settings-close", cx);
        assert!(f.shell.read(cx).settings.is_none());
        assert_eq!(browser_state(f.shell.read(cx), cx), before);
        assert_eq!(w.focused(cx), focus);
    })
    .unwrap();
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
    assert_eq!(
        Store::new(f._config.path().into())
            .gui_preferences()
            .unwrap()
            .theme,
        ThemePreference::Light
    );
    assert_eq!(
        fs::read_to_string(f.local.path().join("keep-00.txt")).unwrap(),
        "local 0\n"
    );
    assert_eq!(
        fs::read_to_string(f.server.dir.path().join("files/keep-00.txt")).unwrap(),
        "remote 0\n"
    );
}

#[gpui_kit::test]
async fn native_theme_picker_default_cancel_and_system_choice_persist_without_loading_omarchy(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("settings", cx);
        let picker = w.find("theme-select");
        assert_eq!(picker.role(), Some(gpui_kit::Role::ComboBox));
        assert_eq!(picker.label(), Some("Theme"));
        assert_eq!(picker.value(), Some("System"));
        assert_ne!(picker.disabled(), Some(true));
        w.click("theme-select", cx);
        w.press("down", cx);
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(
            f.shell.read(cx).settings.is_some(),
            "Escape dismisses the menu, not Settings"
        );
        assert_eq!(w.find("theme-select").expanded(), Some(false));
        assert_eq!(w.find("theme-select").value(), Some("System"));
        assert_eq!(
            f.shell.read(cx).preferences.as_ref().unwrap().read(cx).mode,
            ThemePreference::System
        );
    })
    .unwrap();
    crate::test_support::theme(f.handle, "Light", cx);
    cx.update_window(f.handle, |_, w, cx| {
        w.click("settings-close", cx);
        w.click("settings", cx);
        assert_eq!(w.find("theme-select").value(), Some("Light"));
    })
    .unwrap();
    crate::test_support::theme(f.handle, "System", cx);
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
    assert_eq!(
        Store::new(f._config.path().into())
            .gui_preferences()
            .unwrap()
            .theme,
        ThemePreference::System
    );
}

#[gpui_kit::test]
async fn settings_returns_to_the_underlying_management_view_and_home_leaves_it(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    cx.update_window(f.handle, |_, w, cx| w.click("projects", cx))
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    let focus = cx
        .update_window(f.handle, |_, w, cx| {
            w.input("retained query", cx);
            let focus = w.focused(cx);
            w.click("settings", cx);
            focus
        })
        .unwrap();
    crate::test_support::theme(f.handle, "Dark", cx);
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
        w.press("escape", cx);
        assert!(f.shell.read(cx).settings.is_none());
        assert!(f.shell.read(cx).projects.read(cx).visible());
        assert_eq!(w.find("project-filter").value(), Some("retained query"));
        assert_eq!(w.focused(cx), focus);
        w.click("settings", cx);
        w.click("projects", cx);
        assert!(f.shell.read(cx).settings.is_none());
        assert!(f.shell.read(cx).projects.read(cx).visible());
        assert!(w.try_find("remote-pane-tabs").is_none());
        assert!(w.try_find("workspace-footer").is_none());
        w.click("home", cx);
        w.click("hosts", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell
            .read(cx)
            .hosts
            .as_ref()
            .is_some_and(|h| h.read(cx).can_leave())
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        let hosts = f.shell.read(cx).hosts.clone();
        let session = f.shell.read(cx).remote.read(cx).session_id();
        w.click("settings", cx);
        assert_eq!(
            w.within("workspace-toolbar").find("hosts").label(),
            Some("Hosts")
        );
        assert!(
            !f.shell.read(cx).toolbar_state(cx).hosts_open,
            "Settings is the visible screen"
        );
        assert!(w.try_find("local-files-tab").is_none());
        assert!(w.try_find("remote-files-tab").is_none());
        w.click("settings-close", cx);
        assert!(f.shell.read(cx).settings.is_none());
        assert_eq!(f.shell.read(cx).hosts, hosts);
        w.click("settings", cx);
        w.click("home", cx);
        assert!(f.shell.read(cx).browser_screen_active(cx));
        assert!(f.shell.read(cx).hosts.is_none());
        assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), session);
    })
    .unwrap();
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
}

#[gpui_kit::test]
async fn settings_close_after_remote_preview_disconnect_restores_a_visible_browser(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    cx.update_window(f.handle, |_, w, cx| {
        f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("p", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).remote_view == PaneView::Preview
            && !f.shell.read(cx).remote_preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.press("ctrl-alt-p", cx);
        assert!(!f.shell.read(cx).browser.focus_handle(cx).is_focused(w));
        let preview_focus = w.focused(cx);
        w.click("settings", cx);
        assert!(f.shell.read(cx).settings.is_some());
        assert_ne!(w.focused(cx), preview_focus);
        let remote = f.shell.read(cx).remote.clone();
        remote.update(cx, |remote, cx| remote.disconnect(cx));
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(f.shell.read(cx).settings.is_some());
        assert_eq!(f.shell.read(cx).remote_view, PaneView::Files);
        assert!(!f.shell.read(cx).remote.read(cx).has_session());
        w.click("settings-close", cx);
        assert!(f.shell.read(cx).remote.focus_handle(cx).is_focused(w));
        assert!(w.within("remote-workspace").find("remote-pane").visible());
        w.press("ctrl-f", cx);
        assert_eq!(w.find("remote-filter").focused(), Some(true));
    })
    .unwrap();
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
}

#[gpui_kit::test]
async fn settings_footer_fits_short_windows_and_old_frames_cannot_close_a_new_view(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    cx.update_window(f.handle, |_, w, cx| {
        let before = browser_state(f.shell.read(cx), cx);
        w.click("settings", cx);
        for (width, height) in [(640., 480.), (420., 600.)] {
            w.resize(size(px(width), px(height)));
            w.bounds_changed(cx);
            w.render_frame(cx);
            let close = w.find("settings-close");
            assert!(close.visible());
            assert!(close.bounds().right() <= px(width) && close.bounds().bottom() <= px(height));
            assert!(w.find("settings-body").bounds().size.height > px(0.));
            for key in ["ctrl-f", "f5", "ctrl-escape"] {
                w.press(key, cx);
            }
            assert_eq!(browser_state(f.shell.read(cx), cx), before);
            assert!(f.shell.read(cx).settings.is_some());
        }
        let position = w.find("settings-close").bounds().center();
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
        f.shell.update(cx, |shell, cx| {
            shell.toolbar_event(&ToolbarEvent::Settings, w, cx);
            shell.toolbar_event(&ToolbarEvent::Settings, w, cx);
        });
        let serial = f.shell.read(cx).settings_serial;
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
        assert!(f.shell.read(cx).settings.is_some());
        assert_eq!(f.shell.read(cx).settings_serial, serial);
        assert_eq!(w.focused(cx), focus);
        assert_eq!(browser_state(f.shell.read(cx), cx), before);
        w.render_frame(cx);
        w.click("settings-close", cx);
    })
    .unwrap();
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
}
