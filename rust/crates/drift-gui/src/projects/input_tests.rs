use super::*;
mod seeds;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, App, Bounds, ClipboardItem, EntityInputHandler as _, TestAppContext,
    WindowBounds, WindowOptions, point, size,
};
use std::{cell::RefCell, fs, rc::Rc, time::Duration};

const COMPOSING: &str = "Finish text composition before saving.";
const FORBIDDEN: &str = "Input rejected: control characters and line separators are not allowed.";

struct ProjectInputWindow(Entity<ProjectsPanel>);
impl Render for ProjectInputWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_3()
            .flex()
            .flex_col()
            .child(self.0.clone())
    }
}

async fn fixture(
    store: &Store,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<ProjectsPanel>) {
    cx.executor().allow_parking();
    let (handle, owner) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(380.), px(180.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    ProjectInputWindow(cx.new(|cx| {
                        let mut panel = ProjectsPanel::new(
                            store.clone(),
                            BrowserService::new().unwrap(),
                            w,
                            cx,
                        );
                        panel.show(w, cx);
                        panel
                    }))
                })
            },
        )
        .unwrap()
    });
    cx.simulate_window_resize(handle, size(px(380.), px(180.)));
    let panel = owner.read_with(cx, |owner, _| owner.0.clone());
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        panel.read(cx).cancel.is_none()
    })
    .await;
    frame(handle, cx);
    (handle, panel)
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
fn tab_to(handle: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) {
    for _ in 0..24 {
        press(handle, "tab", cx);
        if cx
            .update_window(handle, |_, w, _| w.find(id).focused() == Some(true))
            .unwrap()
        {
            cx.update_window(handle, |_, w, _| {
                let target = w.find(id);
                assert!(target.visible());
                let viewport_id = if matches!(id, "project-save" | "project-close")
                    && w.try_find("project-dialog").is_some()
                {
                    "project-form-footer"
                } else {
                    "project-details"
                };
                if let Some(viewport) = w.try_find(viewport_id) {
                    assert!(target.bounds().top() >= viewport.bounds().top() - px(1.));
                    assert!(target.bounds().bottom() <= viewport.bounds().bottom() + px(1.));
                }
            })
            .unwrap();
            return;
        }
    }
    panic!("Native Tab did not reach {id}");
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
    input.update(cx, |s, cx| {
        s.set_value("A🦀Z", w, cx);
        s.replace_and_mark_text_in_range(Some(1..3), "に🦀", Some(1..3), w, cx);
        assert_eq!(s.value(), "Aに🦀Z");
        assert_eq!(s.selected_range(), 4..8);
        assert_eq!(s.marked_text_range(w, cx), Some(1..4));
    });
}

#[gpui_kit::test]
async fn project_clipboard_rejects_entire_bulk_and_keeps_utf8_selection_scroll_and_undo(
    cx: &mut TestAppContext,
) {
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let original = store.register("Keep", root.path().into()).unwrap();
    let before = fs::read(config.path().join("projects.toml")).unwrap();
    let (handle, panel) = fixture(&store, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    let (name, path) = panel.read_with(cx, |p, _| {
        let form = p.form.as_ref().unwrap();
        (form.name.clone(), form.path.clone())
    });
    for (id, input) in [
        ("project-edit-name", name.clone()),
        ("project-edit-path", path.clone()),
    ] {
        tab_to(handle, id, cx);
        cx.update_window(handle, |_, w, cx| {
            input.update(cx, |s, cx| {
                s.set_value("A🦀Z", w, cx);
                s.set_selected_range(1..5, cx);
            })
        })
        .unwrap();
        frame(handle, cx);
        let unicode = " 日本🦀e\u{301} ";
        paste(handle, unicode, cx);
        assert_eq!(
            input.read_with(cx, |s, _| s.value()),
            format!("A{unicode}Z")
        );
        cx.update_window(handle, |_, _, cx| {
            input.update(cx, |s, cx| s.set_selected_range(1..4, cx))
        })
        .unwrap();
        frame(handle, cx);
        for control in (0..=31).chain(127..=159).chain([0x2028, 0x2029]) {
            let payload = format!(
                "private-before{}private-after",
                char::from_u32(control).unwrap()
            );
            let snapshot = cx
                .update_window(handle, |_, w, cx| {
                    let s = input.read(cx);
                    let p = panel.read(cx);
                    (
                        s.value(),
                        s.selected_range(),
                        s.cursor_position(),
                        s.scroll_offset(),
                        w.focused(cx),
                        p.cursor.clone(),
                        p.operation,
                        p.details_reveal.scroll_handle().offset(),
                    )
                })
                .unwrap();
            paste(handle, &payload, cx);
            cx.update_window(handle, |_, w, cx| {
                let s = input.read(cx);
                let p = panel.read(cx);
                assert_eq!(
                    (
                        s.value(),
                        s.selected_range(),
                        s.cursor_position(),
                        s.scroll_offset(),
                        w.focused(cx),
                        p.cursor.clone(),
                        p.operation,
                        p.details_reveal.scroll_handle().offset()
                    ),
                    snapshot
                );
                assert_eq!(p.form.as_ref().unwrap().name, name);
                assert_eq!(p.form.as_ref().unwrap().path, path);
                assert!(p.cancel.is_none() && !p.writing && p.delete.is_none());
                assert!(!p.status.contains("private-before"));
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
        fs::read(config.path().join("projects.toml")).unwrap(),
        before
    );
    assert!(store.registry().unwrap().find("keep").unwrap() == &original);
    assert!(!config.path().join("drift.log").exists());
}

#[gpui_kit::test]
async fn project_composition_escape_and_enter_do_not_cancel_submit_or_steal_focus(
    cx: &mut TestAppContext,
) {
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let original = store.register("Keep", root.path().into()).unwrap();
    let (handle, panel) = fixture(&store, cx).await;
    let query = panel.read_with(cx, |p, _| p.query.clone());
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
    let (name, path, operation) = panel.read_with(cx, |p, _| {
        let form = p.form.as_ref().unwrap();
        (form.name.clone(), form.path.clone(), p.operation)
    });
    cx.update_window(handle, |_, w, cx| mark(&name, w, cx))
        .unwrap();
    frame(handle, cx);
    let before = cx
        .update_window(handle, |_, w, cx| {
            let state = name.read(cx);
            (
                state.selected_range(),
                state.scroll_offset(),
                state.cursor_position(),
                w.focused(cx),
            )
        })
        .unwrap();
    paste(handle, "private-preedit\r\nnot-a-second-line", cx);
    cx.update_window(handle, |_, w, cx| {
        let state = name.read(cx);
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
            name.update(cx, |s, cx| s.marked_text_range(w, cx)),
            Some(1..4)
        );
    })
    .unwrap();
    press(handle, "escape", cx);
    cx.update_window(handle, |_, w, cx| {
        let p = panel.read(cx);
        assert_eq!(p.form.as_ref().unwrap().name, name);
        assert_eq!(p.form.as_ref().unwrap().path, path);
        assert_eq!(p.operation, operation);
        assert_eq!(name.read(cx).value(), "Aに🦀Z");
        assert!(name.focus_handle(cx).is_focused(w));
        assert!(
            name.update(cx, |s, cx| s.marked_text_range(w, cx))
                .is_none()
        );
        mark(&name, w, cx);
    })
    .unwrap();
    frame(handle, cx);
    press(handle, "enter", cx);
    cx.update_window(handle, |_, w, cx| {
        assert!(name.focus_handle(cx).is_focused(w));
        assert_eq!(name.read(cx).value(), "Aに🦀Z");
        assert!(
            name.update(cx, |s, cx| s.marked_text_range(w, cx))
                .is_none()
        );
        assert_eq!(panel.read(cx).operation, operation);
    })
    .unwrap();
    tab_to(handle, "project-close", cx);
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.find("project-close").label(), Some("Cancel"))
    })
    .unwrap();
    press(handle, "enter", cx);
    assert!(panel.read_with(cx, |p, _| p.form.is_none() && p.visible()));
    press(handle, "e", cx);
    cx.update_window(handle, |_, w, cx| {
        let name = panel.read(cx).form.as_ref().unwrap().name.clone();
        mark(&name, w, cx);
    })
    .unwrap();
    frame(handle, cx);
    press(handle, "escape", cx);
    assert!(panel.read_with(cx, |p, _| p.form.is_some()));
    press(handle, "escape", cx);
    assert!(panel.read_with(cx, |p, _| p.form.is_none() && p.visible()));
    assert!(store.registry().unwrap().find("keep").unwrap() == &original);
}

#[gpui_kit::test]
async fn project_save_blocks_focused_and_native_tab_blurred_path_composition(
    cx: &mut TestAppContext,
) {
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let original = store.register("Keep", root.path().into()).unwrap();
    let before = fs::read(config.path().join("projects.toml")).unwrap();
    let (handle, panel) = fixture(&store, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    tab_to(handle, "project-edit-path", cx);
    let (name, path, operation) = panel.read_with(cx, |p, _| {
        let form = p.form.as_ref().unwrap();
        (form.name.clone(), form.path.clone(), p.operation)
    });
    cx.update_window(handle, |_, w, cx| mark(&path, w, cx))
        .unwrap();
    frame(handle, cx);
    for blurred in [false, true] {
        if blurred {
            press(handle, "tab", cx);
            cx.update_window(handle, |_, w, _| {
                assert_eq!(w.find("project-close").focused(), Some(true))
            })
            .unwrap();
        }
        for key in ["ctrl-s", "cmd-s"] {
            press(handle, key, cx);
            cx.update_window(handle, |_, w, cx| {
                let p = panel.read(cx);
                assert_eq!(p.status, COMPOSING);
                assert_eq!(p.operation, operation);
                assert!(p.cancel.is_none() && !p.writing);
                assert_eq!(p.form.as_ref().unwrap().name, name);
                assert_eq!(p.form.as_ref().unwrap().path, path);
                assert_eq!(path.read(cx).value(), "Aに🦀Z");
                assert_eq!(path.focus_handle(cx).is_focused(w), !blurred);
                assert_eq!(
                    path.update(cx, |s, cx| s.marked_text_range(w, cx)),
                    Some(1..4)
                );
            })
            .unwrap();
        }
    }
    tab_to(handle, "project-save", cx);
    cx.update_window(handle, |_, w, cx| w.click("project-save", cx))
        .unwrap();
    frame(handle, cx);
    panel.read_with(cx, |p, _| {
        assert_eq!(p.status, COMPOSING);
        assert_eq!(p.operation, operation);
        assert!(p.cancel.is_none() && p.form.is_some());
    });
    cx.update_window(handle, |_, w, cx| {
        path.update(cx, |s, cx| {
            s.unmark_text(w, cx);
            // Native set_value strips CR/LF; C1 exercises a poisoned stored value.
            s.set_value(" private-path\u{85}second-line ", w, cx);
        })
    })
    .unwrap();
    press(handle, "cmd-s", cx);
    panel.read_with(cx, |p, cx| {
        assert_eq!(p.status, FORBIDDEN);
        assert!(!p.status.contains("private-path"));
        assert_eq!(p.operation, operation);
        assert!(p.cancel.is_none() && p.form.is_some());
        assert_eq!(path.read(cx).value(), " private-path\u{85}second-line ");
    });
    assert_eq!(
        fs::read(config.path().join("projects.toml")).unwrap(),
        before
    );
    assert!(store.registry().unwrap().find("keep").unwrap() == &original);
}

#[gpui_kit::test]
async fn project_registration_validates_blurred_composition_and_raw_value_before_emitting(
    cx: &mut TestAppContext,
) {
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let original = store.register("Keep", root.path().into()).unwrap();
    let before = fs::read(config.path().join("projects.toml")).unwrap();
    let (handle, panel) = fixture(&store, cx).await;
    cx.simulate_window_resize(handle, size(px(900.), px(900.)));
    panel.update(cx, |p, cx| {
        p.set_context(store.registry().unwrap(), true, cx)
    });
    frame(handle, cx);
    let registrations = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|cx| {
        let registrations = registrations.clone();
        cx.subscribe(&panel, move |_, event: &ProjectEvent, _| {
            if let ProjectEvent::Register(name) = event {
                registrations.borrow_mut().push(name.clone());
            }
        })
    });
    let name = panel.read_with(cx, |p, _| p.name.clone());
    let operation = panel.read_with(cx, |p, _| p.operation);
    tab_to(handle, "project-name", cx);
    paste(handle, "bad\tname", cx);
    assert!(name.read_with(cx, |s, _| s.value().is_empty()));
    cx.update_window(handle, |_, w, cx| mark(&name, w, cx))
        .unwrap();
    frame(handle, cx);
    press(handle, "tab", cx);
    cx.update_window(handle, |_, w, cx| {
        assert!(!name.focus_handle(cx).is_focused(w));
        assert_eq!(
            name.update(cx, |s, cx| s.marked_text_range(w, cx)),
            Some(1..4)
        );
        w.click("register", cx);
    })
    .unwrap();
    frame(handle, cx);
    panel.read_with(cx, |p, _| {
        assert_eq!(p.status, COMPOSING);
        assert_eq!(p.operation, operation);
        assert!(p.cancel.is_none() && p.visible());
    });
    assert!(registrations.borrow().is_empty());
    cx.update_window(handle, |_, w, cx| {
        name.update(cx, |s, cx| {
            s.unmark_text(w, cx);
            s.set_value(" private-name\u{2029}tail ", w, cx);
        })
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| w.click("register", cx))
        .unwrap();
    frame(handle, cx);
    panel.read_with(cx, |p, cx| {
        assert_eq!(p.status, FORBIDDEN);
        assert!(!p.status.contains("private-name"));
        assert_eq!(p.operation, operation);
        assert_eq!(name.read(cx).value(), " private-name\u{2029}tail ");
    });
    assert!(registrations.borrow().is_empty());
    tab_to(handle, "project-name", cx);
    press(
        handle,
        if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        },
        cx,
    );
    let unicode = " 日本🦀e\u{301} ";
    paste(handle, unicode, cx);
    cx.update_window(handle, |_, w, cx| w.click("register", cx))
        .unwrap();
    frame(handle, cx);
    assert_eq!(*registrations.borrow(), vec![unicode.to_owned()]);
    assert_eq!(
        fs::read(config.path().join("projects.toml")).unwrap(),
        before
    );
    assert!(store.registry().unwrap().find("keep").unwrap() == &original);
}
