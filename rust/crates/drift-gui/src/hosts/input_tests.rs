use super::*;
mod seeds;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, ClipboardItem, ElementId, EntityInputHandler as _, TestAppContext,
    WindowBounds, WindowOptions, point, size,
};
use std::{fs, time::Duration};

const COMPOSING: &str = "Finish text composition before saving.";
const FORBIDDEN: &str = "Input rejected: control characters and line separators are not allowed.";

struct HostInputWindow(Entity<HostManager>);
impl Render for HostInputWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p_4().flex().child(self.0.clone())
    }
}

async fn fixture(store: &Store, cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<HostManager>) {
    cx.executor().allow_parking();
    let (handle, owner) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(860.), px(420.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    HostInputWindow(cx.new(|cx| {
                        let service = BrowserService::new().unwrap();
                        HostManager::new(
                            store.clone(),
                            service.clone(),
                            RemoteService::new(service),
                            Some("project".into()),
                            w,
                            cx,
                        )
                    }))
                })
            },
        )
        .unwrap()
    });
    cx.simulate_window_resize(handle, size(px(860.), px(420.)));
    let manager = owner.read_with(cx, |owner, _| owner.0.clone());
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        manager.read(cx).cancel.is_none()
    })
    .await;
    frame(handle, cx);
    (handle, manager)
}

fn frame(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
}
fn press(handle: AnyWindowHandle, key: &str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, cx| w.press(key, cx))
        .unwrap();
    frame(handle, cx);
}
fn tab_to(handle: AnyWindowHandle, id: impl Into<ElementId>, cx: &mut TestAppContext) {
    let id = id.into();
    for _ in 0..128 {
        press(handle, "tab", cx);
        if cx
            .update_window(handle, |_, w, _| w.find(id.clone()).focused() == Some(true))
            .unwrap()
        {
            cx.update_window(handle, |_, w, _| {
                let bounds = w.find(id.clone()).bounds();
                let viewport = w.find("host-details").bounds();
                assert!(w.find(id.clone()).visible());
                assert!(bounds.top() >= viewport.top() - px(1.));
                assert!(bounds.bottom() <= viewport.bottom() + px(1.));
            })
            .unwrap();
            return;
        }
    }
    panic!("Native Tab did not reach {id:?}");
}
fn paste(handle: AnyWindowHandle, text: &str, cx: &mut TestAppContext) {
    cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(text.into())));
    press(
        handle,
        if cfg!(target_os = "macos") {
            "cmd-v"
        } else {
            "ctrl-v"
        },
        cx,
    );
}
fn mark(input: &Entity<InputState>, w: &mut Window, cx: &mut App) {
    input.update(cx, |state, cx| {
        state.set_value("A🦀Z", w, cx);
        state.replace_and_mark_text_in_range(Some(1..3), "に🦀", Some(1..3), w, cx);
        assert_eq!(state.value(), "Aに🦀Z");
        assert_eq!(state.selected_range(), 4..8);
        assert_eq!(state.marked_text_range(w, cx), Some(1..4));
    });
}

fn original() -> Host {
    Host {
        name: "keep".into(),
        hostname: "example.test".into(),
        protocol: "ftp".into(),
        root_path: "/srv".into(),
        auth: drift_core::config::Auth {
            password: "A🦀Z".into(),
            ..Default::default()
        },
        mappings: (0..4)
            .map(|index| Mapping {
                local: format!("src/{index}"),
                remote: format!("deploy/{index}"),
            })
            .collect(),
        ..Default::default()
    }
}

#[gpui_kit::test]
async fn host_clipboard_rejects_whole_bulk_without_changing_native_state_or_undo(
    cx: &mut TestAppContext,
) {
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let host = original();
    store
        .save_host(Some("project"), None, host.clone())
        .unwrap();
    let before = fs::read(config.path().join("projects/project.toml")).unwrap();
    let (handle, manager) = fixture(&store, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    let fields = manager.read_with(cx, |m, _| m.form.as_ref().unwrap().fields.clone());
    for (id, input) in [
        (ElementId::from("host-name"), fields[NAME].clone()),
        (ElementId::from("password"), fields[PASSWORD].clone()),
        (
            ElementId::from(("map-local", 0usize)),
            manager.read_with(cx, |m, _| m.form.as_ref().unwrap().mappings[0].0.clone()),
        ),
    ] {
        tab_to(handle, id.clone(), cx);
        cx.update_window(handle, |_, w, cx| {
            input.update(cx, |s, cx| {
                s.set_value("A🦀Z", w, cx);
                s.set_selected_range(
                    if id == ElementId::from("password") {
                        0..6
                    } else {
                        1..5
                    },
                    cx,
                );
            });
        })
        .unwrap();
        frame(handle, cx);
        let unicode = " 日本🦀e\u{301} ";
        paste(handle, unicode, cx);
        assert_eq!(
            input.read_with(cx, |s, _| s.value()),
            if id == ElementId::from("password") {
                unicode.to_owned()
            } else {
                format!("A{unicode}Z")
            }
        );
        cx.update_window(handle, |_, _, cx| {
            input.update(cx, |s, cx| s.set_selected_range(1..4, cx))
        })
        .unwrap();
        frame(handle, cx);
        for control in (0..=31).chain(127..=159).chain([0x2028, 0x2029]) {
            let payload = format!(
                "secret-before{}secret-after",
                char::from_u32(control).unwrap()
            );
            let snapshot = cx
                .update_window(handle, |_, w, cx| {
                    let s = input.read(cx);
                    let m = manager.read(cx);
                    (
                        s.value(),
                        s.selected_range(),
                        s.cursor_position(),
                        s.scroll_offset(),
                        s.presentation().is_masked(),
                        w.focused(cx),
                        m.cursor.clone(),
                        m.operation,
                        m.form.as_ref().unwrap().reveal.scroll_handle().offset(),
                    )
                })
                .unwrap();
            paste(handle, &payload, cx);
            cx.update_window(handle, |_, w, cx| {
                let s = input.read(cx);
                let m = manager.read(cx);
                assert_eq!(
                    (
                        s.value(),
                        s.selected_range(),
                        s.cursor_position(),
                        s.scroll_offset(),
                        s.presentation().is_masked(),
                        w.focused(cx),
                        m.cursor.clone(),
                        m.operation,
                        m.form.as_ref().unwrap().reveal.scroll_handle().offset()
                    ),
                    snapshot
                );
                assert!(m.tools.is_none() && m.cancel.is_none() && !m.writing);
                assert!(!m.status.contains("secret-before"));
                assert_eq!(m.form.as_ref().unwrap().fields, fields);
                if id == ElementId::from("password") {
                    assert!(s.presentation().is_masked());
                    assert!(
                        !w.find(id.clone())
                            .value()
                            .unwrap_or_default()
                            .contains("日本")
                    );
                }
            })
            .unwrap();
        }
        press(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-z"
            } else {
                "ctrl-z"
            },
            cx,
        );
        assert_eq!(
            input.read_with(cx, |s, _| s.value()),
            "A🦀Z",
            "rejection added an undo entry"
        );
    }
    assert_eq!(
        fs::read(config.path().join("projects/project.toml")).unwrap(),
        before
    );
    assert!(store.project("project").unwrap().hosts == vec![host]);
    assert!(!config.path().join("drift.log").exists());
}

#[gpui_kit::test]
async fn host_real_composition_escape_and_enter_keep_native_entities_and_focus(
    cx: &mut TestAppContext,
) {
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.save_host(Some("project"), None, original()).unwrap();
    let (handle, manager) = fixture(&store, cx).await;
    let query = manager.read_with(cx, |m, _| m.query.clone());
    cx.update_window(handle, |_, w, cx| mark(&query, w, cx))
        .unwrap();
    frame(handle, cx);
    press(handle, "enter", cx);
    cx.update_window(handle, |_, w, cx| {
        assert!(query.focus_handle(cx).is_focused(w));
        assert_eq!(query.read(cx).value(), "Aに🦀Z");
        assert!(
            query
                .update(cx, |s, cx| s.marked_text_range(w, cx))
                .is_none()
        );
        query.update(cx, |s, cx| s.set_value("", w, cx));
    })
    .unwrap();
    press(handle, "down", cx);
    press(handle, "e", cx);
    let fields = manager.read_with(cx, |m, _| m.form.as_ref().unwrap().fields.clone());
    let input = fields[NAME].clone();
    cx.update_window(handle, |_, w, cx| mark(&input, w, cx))
        .unwrap();
    frame(handle, cx);
    let before = cx
        .update_window(handle, |_, w, cx| {
            let state = input.read(cx);
            (
                state.selected_range(),
                state.scroll_offset(),
                state.cursor_position(),
                w.focused(cx),
            )
        })
        .unwrap();
    paste(handle, "secret-preedit\nnot-a-second-line", cx);
    cx.update_window(handle, |_, w, cx| {
        let state = input.read(cx);
        assert_eq!(state.value(), "Aに🦀Z");
        assert_eq!(
            (
                state.selected_range(),
                state.scroll_offset(),
                state.cursor_position(),
                w.focused(cx)
            ),
            before
        );
        assert_eq!(
            input.update(cx, |s, cx| s.marked_text_range(w, cx)),
            Some(1..4)
        );
    })
    .unwrap();
    press(handle, "escape", cx);
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(manager.read(cx).form.as_ref().unwrap().fields, fields);
        assert_eq!(input.read(cx).value(), "Aに🦀Z");
        assert!(input.focus_handle(cx).is_focused(w));
        assert!(
            input
                .update(cx, |s, cx| s.marked_text_range(w, cx))
                .is_none()
        );
        mark(&input, w, cx);
    })
    .unwrap();
    frame(handle, cx);
    press(handle, "enter", cx);
    cx.update_window(handle, |_, w, cx| {
        assert!(input.focus_handle(cx).is_focused(w));
        assert_eq!(input.read(cx).value(), "Aに🦀Z");
        assert!(
            input
                .update(cx, |s, cx| s.marked_text_range(w, cx))
                .is_none()
        );
        assert!(manager.read(cx).cancel.is_none());
    })
    .unwrap();
    tab_to(handle, "host-cancel", cx);
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.find("host-cancel").label(), Some("Cancel"))
    })
    .unwrap();
    press(handle, "enter", cx);
    assert!(manager.read_with(cx, |m, _| m.form.is_none()));
    press(handle, "e", cx);
    cx.update_window(handle, |_, w, cx| {
        let input = manager.read(cx).form.as_ref().unwrap().fields[NAME].clone();
        mark(&input, w, cx);
    })
    .unwrap();
    frame(handle, cx);
    press(handle, "escape", cx);
    assert!(manager.read_with(cx, |m, _| m.form.is_some()));
    press(handle, "escape", cx);
    assert!(manager.read_with(cx, |m, _| m.form.is_none()));
    assert!(store.project("project").unwrap().hosts == vec![original()]);
}

#[gpui_kit::test]
async fn host_save_and_test_block_focused_blurred_and_offscreen_mapping_composition(
    cx: &mut TestAppContext,
) {
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.save_host(Some("project"), None, original()).unwrap();
    let before = fs::read(config.path().join("projects/project.toml")).unwrap();
    let (handle, manager) = fixture(&store, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    tab_to(handle, ("map-local", 0usize), cx);
    let input = manager.read_with(cx, |m, _| m.form.as_ref().unwrap().mappings[0].0.clone());
    let operation = manager.read_with(cx, |m, _| m.operation);
    cx.update_window(handle, |_, w, cx| mark(&input, w, cx))
        .unwrap();
    frame(handle, cx);
    for blurred in [false, true] {
        if blurred {
            press(handle, "tab", cx);
            cx.update_window(handle, |_, w, _| {
                assert_eq!(w.find(("map-remote", 0usize)).focused(), Some(true))
            })
            .unwrap();
        }
        for key in ["ctrl-s", "cmd-s"] {
            press(handle, key, cx);
            cx.update_window(handle, |_, w, cx| {
                let m = manager.read(cx);
                assert_eq!(m.status, COMPOSING);
                assert_eq!(m.operation, operation);
                assert!(m.tools.is_none() && m.cancel.is_none() && !m.writing);
                assert_eq!(input.read(cx).value(), "Aに🦀Z");
                assert_eq!(input.focus_handle(cx).is_focused(w), !blurred);
                assert_eq!(
                    input.update(cx, |s, cx| s.marked_text_range(w, cx)),
                    Some(1..4)
                );
            })
            .unwrap();
        }
    }
    cx.simulate_window_resize(handle, size(px(860.), px(180.)));
    frame(handle, cx);
    for id in ["host-save", "host-test-draft"] {
        tab_to(handle, id, cx);
        cx.update_window(handle, |_, w, cx| {
            assert!(!w.find(("map-local", 0usize)).visible());
            w.click(id, cx);
        })
        .unwrap();
        frame(handle, cx);
        manager.read_with(cx, |m, _| {
            assert_eq!(m.status, COMPOSING);
            assert_eq!(m.operation, operation);
            assert!(m.tools.is_none() && m.cancel.is_none() && m.form.is_some());
        });
    }
    assert_eq!(
        fs::read(config.path().join("projects/project.toml")).unwrap(),
        before
    );
    assert!(store.project("project").unwrap().hosts == vec![original()]);
}

#[gpui_kit::test]
async fn host_hidden_secret_controls_block_save_and_test_without_echo_or_rewrite(
    cx: &mut TestAppContext,
) {
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.save_host(Some("project"), None, original()).unwrap();
    let (handle, manager) = fixture(&store, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    let password = manager.read_with(cx, |m, _| m.form.as_ref().unwrap().fields[PASSWORD].clone());
    let operation = manager.read_with(cx, |m, _| m.operation);
    tab_to(handle, "password", cx);
    cx.update_window(handle, |_, w, cx| mark(&password, w, cx))
        .unwrap();
    frame(handle, cx);
    tab_to(handle, "protocol-sftp", cx);
    press(handle, "enter", cx);
    tab_to(handle, "auth-agent", cx);
    press(handle, "enter", cx);
    cx.update_window(handle, |_, w, _| assert!(w.try_find("password").is_none()))
        .unwrap();
    for id in ["host-save", "host-test-draft"] {
        tab_to(handle, id, cx);
        cx.update_window(handle, |_, w, cx| {
            w.click(id, cx);
            assert_eq!(
                password.update(cx, |s, cx| s.marked_text_range(w, cx)),
                Some(1..4)
            );
            assert!(!password.focus_handle(cx).is_focused(w));
            let m = manager.read(cx);
            assert_eq!(m.status, COMPOSING);
            assert_eq!(m.operation, operation);
            assert!(m.tools.is_none() && m.cancel.is_none());
        })
        .unwrap();
    }
    // Native set_value strips CR/LF; raw LF paste is covered above.
    let hostile = " password-must-not-echo\u{2029}second-line ";
    cx.update_window(handle, |_, w, cx| {
        password.update(cx, |s, cx| {
            s.unmark_text(w, cx);
            s.set_value(hostile, w, cx);
        })
    })
    .unwrap();
    for id in ["host-save", "host-test-draft"] {
        tab_to(handle, id, cx);
        cx.update_window(handle, |_, w, cx| w.click(id, cx))
            .unwrap();
        frame(handle, cx);
        manager.read_with(cx, |m, cx| {
            assert_eq!(m.status, FORBIDDEN);
            assert!(!m.status.contains("password-must-not-echo"));
            assert_eq!(m.operation, operation);
            assert!(m.tools.is_none() && m.cancel.is_none() && m.form.is_some());
            assert_eq!(password.read(cx).value(), hostile);
            assert!(password.read(cx).presentation().is_masked());
        });
    }
    assert!(store.project("project").unwrap().hosts == vec![original()]);
}
