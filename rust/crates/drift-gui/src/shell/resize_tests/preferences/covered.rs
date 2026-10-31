use super::*;

#[gpui_kit::test]
async fn covered_theme_controls_reject_old_pointer_release_and_keys_before_repaint(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    for route in 0..5 {
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            assert!(w.try_find("gui-preferences").is_none());
            w.click("settings", cx);
            for _ in 0..64 {
                w.press("tab", cx);
                if w.find("theme-select").focused() == Some(true) {
                    break;
                }
            }
            assert_eq!(w.find("theme-select").focused(), Some(true));
            let position = w.find("theme-select").bounds().center();
            let mode = cx.theme().mode;
            let state = browser_state(f.shell.read(cx), cx);
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
            f.shell.update(cx, |s, cx| match route {
                0 => s.show_help(&ShowHelp, w, cx),
                1 => s.toolbar_event(&ToolbarEvent::Projects, w, cx),
                2 => {
                    s.close_settings(w, cx);
                    s.toolbar_event(&ToolbarEvent::Hosts, w, cx);
                }
                3 => {
                    s.toolbar_event(&ToolbarEvent::Settings, w, cx);
                    s.toolbar_event(&ToolbarEvent::Settings, w, cx);
                }
                _ => {
                    s.folder_prompt = true;
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
            for key in ["enter", "space"] {
                w.dispatch_keystroke(gpui_kit::Keystroke::parse(key).unwrap(), cx);
            }
            assert_eq!(cx.theme().mode, mode);
            assert_eq!(w.focused(cx), focus);
            assert_eq!(browser_state(f.shell.read(cx), cx), state);
            f.shell.update(cx, |s, cx| match route {
                0 => s.close_help(w, cx),
                1 => s.projects.update(cx, |projects, cx| projects.close(cx)),
                2 => s
                    .hosts
                    .as_ref()
                    .unwrap()
                    .update(cx, |_, cx| cx.emit(HostEvent::Close)),
                3 => {}
                _ => {
                    s.folder_prompt = false;
                }
            });
            f.shell.update(cx, |s, cx| {
                if s.settings.is_some() {
                    s.close_settings(w, cx);
                }
                s.focus_workspace(w, cx);
            });
        })
        .unwrap();
        f.idle(cx).await;
    }
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
    assert!(!f._config.path().join("gui.toml").exists());
}

#[gpui_kit::test]
async fn open_native_theme_menu_rejects_confirm_after_settings_serial_or_overlay_changes(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    for route in 0..3 {
        let (mode, before) = cx
            .update_window(f.handle, |_, w, cx| {
                w.click("settings", cx);
                w.click("theme-select", cx);
                w.press("down", cx); // Omarchy is only highlighted: never load the user's source.
                w.press("down", cx);
                assert_eq!(w.find("theme-select").expanded(), Some(true));
                let mode = cx.theme().mode;
                let before = browser_state(f.shell.read(cx), cx);
                f.shell.update(cx, |s, cx| match route {
                    0 => {
                        s.close_settings(w, cx);
                        s.open_settings(w, cx);
                    }
                    1 => s.show_help(&ShowHelp, w, cx),
                    _ => s.folder_prompt = true,
                });
                crate::test_support::enter_without_repaint(w, cx);
                (mode, before)
            })
            .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            assert_eq!(cx.theme().mode, mode, "route {route}");
            assert_eq!(
                f.shell.read(cx).preferences.as_ref().unwrap().read(cx).mode,
                ThemePreference::System
            );
            assert_eq!(browser_state(f.shell.read(cx), cx), before);
            f.shell.update(cx, |s, cx| {
                match route {
                    1 => s.close_help(w, cx),
                    2 => s.folder_prompt = false,
                    _ => {}
                }
                s.close_settings(w, cx);
                s.focus_workspace(w, cx);
            });
        })
        .unwrap();
    }
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
    assert!(!f._config.path().join("gui.toml").exists());
}
