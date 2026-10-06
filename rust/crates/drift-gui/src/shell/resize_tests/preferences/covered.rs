use super::*;

#[gpui_kit::test]
async fn covered_theme_controls_reject_old_pointer_release_and_keys_before_repaint(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    for route in 0..4 {
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            w.click("filter", cx);
            for _ in 0..64 {
                w.press("shift-tab", cx);
                if w.find("theme-dark").focused() == Some(true) {
                    break;
                }
            }
            assert_eq!(w.find("theme-dark").focused(), Some(true));
            let position = w.find("theme-dark").bounds().center();
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
                2 => s.open_hosts(w, cx),
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
                _ => {
                    s.folder_prompt = false;
                }
            });
        })
        .unwrap();
        f.idle(cx).await;
    }
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
    assert!(!f._config.path().join("gui.toml").exists());
}
