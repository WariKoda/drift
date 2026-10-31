use super::*;
use crate::sftp_test_support::ftp;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, InputEvent as _, MouseButton, MouseMoveEvent, Pixels, Point,
    TestAppContext, WindowBounds, WindowOptions, point, px, size,
};
use std::{fs, time::Duration};

struct Fixture {
    handle: AnyWindowHandle,
    shell: Entity<Shell>,
    server: ftp::Server,
    local: tempfile::TempDir,
    config: tempfile::TempDir,
    writer: PreferencesWriter,
}

impl Fixture {
    async fn new(cx: &mut TestAppContext) -> Self {
        cx.executor().allow_parking();
        let server = ftp::Server::new(4);
        let local = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        fs::write(local.path().join("keep.txt"), "local unchanged\n").unwrap();
        fs::write(
            server.dir.path().join("files/keep.txt"),
            "remote unchanged\n",
        )
        .unwrap();
        let store = Store::new(config.path().into());
        store.save_host(None, None, server.host()).unwrap();
        let mut second = server.host();
        second.name = "Second host".into();
        store.save_host(None, None, second).unwrap();
        let writer = PreferencesWriter::open(store.clone(), Default::default()).unwrap();
        let (handle, shell) = cx.update(|cx| {
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
                        let service = BrowserService::new().unwrap();
                        Shell::new(
                            w,
                            cx,
                            store,
                            service.clone(),
                            RemoteService::with_options(service, server.options()),
                            local.path().to_path_buf(),
                        )
                        .with_preferences(
                            GuiPreferences::default(),
                            writer.clone(),
                            None,
                            w,
                            cx,
                        )
                    })
                },
            )
            .unwrap()
        });
        let f = Self {
            handle,
            shell,
            server,
            local,
            config,
            writer,
        };
        f.idle(cx).await;
        f
    }

    async fn idle(&self, cx: &mut TestAppContext) {
        cx.wait_for(self.handle, Duration::from_secs(30), |_, cx| {
            let s = self.shell.read(cx);
            s.browser.read(cx).location().is_some()
                && !s.browser.read(cx).is_loading()
                && !s.remote.read(cx).is_loading()
        })
        .await;
    }

    fn unchanged(&self) {
        assert_eq!(
            fs::read_to_string(self.local.path().join("keep.txt")).unwrap(),
            "local unchanged\n"
        );
        assert_eq!(
            fs::read_to_string(self.server.dir.path().join("files/keep.txt")).unwrap(),
            "remote unchanged\n"
        );
        for command in ["RETR", "STOR", "DELE"] {
            assert_eq!(self.server.commands(command), 0, "unexpected {command}");
        }
    }
}

fn pointer_down(w: &mut Window, position: Point<Pixels>, cx: &mut gpui_kit::App) {
    w.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Default::default(),
        }
        .to_platform_input(),
        cx,
    );
    w.render_frame(cx);
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
}

fn pointer_up(w: &mut Window, position: Point<Pixels>, cx: &mut gpui_kit::App) {
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
}

fn option_position(w: &Window, id: &'static str, row: usize) -> Point<Pixels> {
    let bounds = w.find(id).bounds();
    // Pinned native Select: 6px popup gap, 4px list inset, 28px medium rows.
    point(
        bounds.center().x,
        bounds.bottom() + px(10. + 28. * (row as f32 + 0.5)),
    )
}

#[gpui_kit::test]
async fn closed_host_trigger_help_and_settings_restore_live_keyboard_focus(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    for settings in [false, true] {
        cx.update_window(f.handle, |_, w, cx| {
            crate::test_support::settle(w, cx);
            w.click("remote-host-select", cx);
            assert_eq!(w.find("remote-host-select").expanded(), Some(true));
            w.press("escape", cx);
        })
        .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            assert_eq!(w.find("remote-host-select").expanded(), Some(false));
            assert!(f.shell.read(cx).remote.read(cx).host_select_focused(w, cx));
            if settings {
                w.click("settings", cx);
            } else {
                w.press("f1", cx);
            }
        })
        .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            w.press("escape", cx);
        })
        .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            crate::test_support::settle(w, cx);
            assert!(f.shell.read(cx).remote.focus_handle(cx).is_focused(w));
            w.press("ctrl-f", cx);
            assert_eq!(w.find("remote-filter").focused(), Some(true));
            w.input("still useful", cx);
            assert_eq!(w.find("remote-filter").value(), Some("still useful"));
            w.press("ctrl-a", cx);
            w.press("backspace", cx);
        })
        .unwrap();
    }
    f.writer.finish().unwrap();
    assert!(!f.config.path().join("gui.toml").exists());
    assert_eq!(f.server.commands("PASS"), 0);
    f.unchanged();
}

#[gpui_kit::test]
async fn stale_host_popup_pointer_completion_preserves_modal_and_live_focus(
    cx: &mut TestAppContext,
) {
    for route in 0..3 {
        let f = Fixture::new(cx).await;
        // Prove the same native pointer coordinates normally confirm a real host.
        cx.update_window(f.handle, |_, w, cx| {
            crate::test_support::settle(w, cx);
            w.click("remote-host-select", cx);
            let position = option_position(w, "remote-host-select", 0);
            pointer_down(w, position, cx);
            pointer_up(w, position, cx);
        })
        .unwrap();
        f.idle(cx).await;
        assert!(
            f.shell
                .read_with(cx, |s, cx| s.remote.read(cx).has_session())
        );
        cx.update_window(f.handle, |_, w, cx| {
            crate::test_support::settle(w, cx);
            w.click("remote-host-select", cx);
            assert_eq!(w.find("remote-host-select").expanded(), Some(true));
        })
        .unwrap();
        let (focus, session, connection, logins) = cx
            .update_window(f.handle, |_, w, cx| {
                let position = option_position(w, "remote-host-select", 1);
                pointer_down(w, position, cx);
                f.shell.update(cx, |s, cx| match route {
                    0 => s.show_help(&ShowHelp, w, cx),
                    1 => {
                        let project = s.remote.read(cx).project();
                        let mut second = f.server.host();
                        second.name = "Second host".into();
                        second.auth.password = "retired credentials".into();
                        let hosts = vec![f.server.host(), second];
                        s.remote
                            .update(cx, |pane, cx| pane.set_context(project, hosts, cx));
                        s.browser.focus_handle(cx).focus(w, cx);
                    }
                    _ => {
                        s.remote
                            .update(cx, |pane, cx| pane.refresh(&Refresh, w, cx));
                        assert!(s.remote.read(cx).is_loading());
                        s.browser.focus_handle(cx).focus(w, cx);
                    }
                });
                let s = f.shell.read(cx);
                let expected = (
                    w.focused(cx),
                    s.remote.read(cx).session_id(),
                    s.remote.read(cx).connection(),
                    f.server.commands("PASS"),
                );
                pointer_up(w, position, cx);
                expected
            })
            .unwrap();
        // Native option confirmation and focus restoration both run in defer_in.
        cx.update_window(f.handle, |_, w, cx| {
            assert_eq!(w.focused(cx), focus, "route {route}");
            assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), session);
            assert_eq!(f.shell.read(cx).remote.read(cx).connection(), connection);
            assert_eq!(f.server.commands("PASS"), logins);
            w.render_frame(cx);
            if route == 0 {
                w.press("escape", cx);
                assert!(f.shell.read(cx).remote.focus_handle(cx).is_focused(w));
                w.press("ctrl-f", cx);
                assert_eq!(w.find("remote-filter").focused(), Some(true));
            }
        })
        .unwrap();
        f.idle(cx).await;
        f.writer.finish().unwrap();
        f.unchanged();
    }
}

#[gpui_kit::test]
async fn stale_theme_popup_pointer_completion_cannot_save_or_steal_focus(cx: &mut TestAppContext) {
    let f = Fixture::new(cx).await;
    for help in [true, false] {
        cx.update_window(f.handle, |_, w, cx| {
            w.click("settings", cx);
            w.click("theme-select", cx);
            assert_eq!(w.find("theme-select").expanded(), Some(true));
        })
        .unwrap();
        let (focus, retired) = cx
            .update_window(f.handle, |_, w, cx| {
                let retired = f
                    .shell
                    .read(cx)
                    .settings
                    .as_ref()
                    .unwrap()
                    .theme
                    .as_ref()
                    .unwrap()
                    .state
                    .clone();
                let position = option_position(w, "theme-select", 2); // Dark, never Omarchy.
                pointer_down(w, position, cx);
                f.shell.update(cx, |s, cx| {
                    if help {
                        s.show_help(&ShowHelp, w, cx);
                    } else {
                        s.close_settings(w, cx);
                        s.open_settings(w, cx);
                    }
                });
                let focus = w.focused(cx);
                pointer_up(w, position, cx);
                (focus, retired)
            })
            .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            assert_eq!(
                retired.read(cx).selected_value(),
                Some(&"Dark"),
                "native pointer callback must actually run"
            );
            assert_eq!(w.focused(cx), focus);
            assert_eq!(
                f.shell.read(cx).preferences.as_ref().unwrap().read(cx).mode,
                ThemePreference::System
            );
            w.render_frame(cx);
            if help {
                w.press("escape", cx);
            }
            w.click("settings-close", cx);
        })
        .unwrap();
    }
    f.writer.finish().unwrap();
    assert!(!f.config.path().join("gui.toml").exists());
    f.unchanged();
}
