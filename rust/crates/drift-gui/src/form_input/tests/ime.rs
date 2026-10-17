//! Public callback protocol tests, not simulated Wayland events. Linux exposes
//! neither Done nor its serial to InputHandler; rebuilding the real adapter is
//! the repaint proxy, while all edits/history belong to retained native entities.
use super::*;
mod empty;

const REDO: &str = if cfg!(target_os = "macos") {
    "cmd-shift-z"
} else {
    "ctrl-y"
};

fn japanese_preedit(input: &Entity<InputState>, window: &mut Window, cx: &mut App) {
    input.update(cx, |state, cx| {
        state.set_value("A🦀Z", window, cx);
        state.replace_and_mark_text_in_range(Some(1..3), "に🦀", Some(1..3), window, cx);
        assert_eq!(state.value(), "Aに🦀Z");
        assert_eq!(state.marked_text_range(window, cx), Some(1..4));
        assert_eq!(
            state.selected_text_range(false, window, cx).unwrap().range,
            2..4
        );
    });
    window.render_frame(cx);
}

fn reject_commit(
    input: &Entity<InputState>,
    boundary: &Entity<Boundary>,
    window: &mut Window,
    cx: &mut App,
) {
    handler(input, boundary, cx).replace_text_in_range(None, "bad\nvalue", window, cx);
    assert_eq!(boundary.read(cx).warning.unwrap().message(), WARNING);
}

#[gpui_kit::test]
fn rejected_commit_and_marked_cleanup_are_atomic_even_across_adapter_repaint(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    for index in 0..2 {
        for repaint in [false, true] {
            let (input, boundary) = entities(&fields, index, cx);
            cx.update_window(handle, |_, window, cx| {
                window.click(if index == 0 { "first" } else { "second" }, cx);
                japanese_preedit(&input, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            let counters = fields.read_with(cx, |s, cx| {
                (s.changes, s.validations.get(), boundary.read(cx).revision)
            });
            cx.update_window(handle, |_, window, cx| {
                let before = Snapshot::read(&input, window, cx);
                let mut proxy = handler(&input, &boundary, cx);
                proxy.replace_text_in_range(None, "bad\nvalue", window, cx);
                assert_eq!(Snapshot::read(&input, window, cx), before);
                let serial = boundary.read(cx).request_serial;
                if repaint {
                    drop(proxy);
                    window.render_frame(cx);
                    assert_eq!(Snapshot::read(&input, window, cx), before);
                    proxy = handler(&input, &boundary, cx);
                }
                // Wayland Done calls marked_text_range first, then Some(marked), "".
                let marked = proxy.marked_text_range(window, cx).unwrap();
                assert_eq!(marked, 1..4, "native UTF-16, not UTF-8 offsets");
                proxy.replace_text_in_range(Some(marked), "", window, cx);
                assert_eq!(Snapshot::read(&input, window, cx), before);
                assert_eq!(boundary.read(cx).request_serial, serial);
                assert_eq!(boundary.read(cx).warning.unwrap().message(), WARNING);
                window.render_frame(cx);
                assert_eq!(Snapshot::read(&input, window, cx), before);
            })
            .unwrap();
            cx.run_until_parked();
            fields.read_with(cx, |s, cx| {
                assert_eq!(
                    (s.changes, s.validations.get(), boundary.read(cx).revision),
                    counters
                );
            });
            cx.update_window(handle, |_, window, cx| {
                // The consumed gate must not keep blocking a real explicit delete.
                handler(&input, &boundary, cx).replace_text_in_range(Some(1..4), "", window, cx);
                assert_eq!(input.read(cx).value(), "AZ");
                assert!(
                    input
                        .update(cx, |s, cx| s.marked_text_range(window, cx))
                        .is_none()
                );
                assert!(boundary.read(cx).warning.is_none());
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn rejected_commit_cleanup_preserves_native_composition_history_and_selection_metadata(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        input.update(cx, |s, cx| s.set_value("A🦀Z", window, cx));
        let original = Snapshot::read(&input, window, cx);
        input.update(cx, |s, cx| {
            s.replace_and_mark_text_in_range(Some(1..3), "に🦀", Some(1..3), window, cx);
        });
        window.render_frame(cx);
        let composing = Snapshot::read(&input, window, cx);
        reject_commit(&input, &boundary, window, cx);
        window.render_frame(cx);
        handler(&input, &boundary, cx).replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(Snapshot::read(&input, window, cx), composing);
        // Continuing the native preedit must stay in the original transaction.
        handler(&input, &boundary, cx).replace_and_mark_text_in_range(
            None,
            "日本🦀",
            Some(2..4),
            window,
            cx,
        );
        let final_preedit = Snapshot::read(&input, window, cx);
        handler(&input, &boundary, cx).unmark_text(window, cx);
        window.press(UNDO, cx);
        let undone = Snapshot::read(&input, window, cx);
        assert_eq!(undone.value, original.value);
        assert_eq!(undone.selection, original.selection);
        assert_eq!(undone.cursor, original.cursor);
        window.press(REDO, cx);
        let redone = Snapshot::read(&input, window, cx);
        assert_eq!(redone.value, final_preedit.value);
        assert_eq!(redone.selection, final_preedit.selection);
        assert_eq!(redone.selected_text, final_preedit.selected_text);
        assert_eq!(redone.cursor, final_preedit.cursor);
        assert_eq!(redone.marked, None);
        window.press(UNDO, cx);
        assert_eq!(input.read(cx).value(), "A🦀Z");
        window.press(UNDO, cx);
        assert_eq!(
            input.read(cx).value(),
            "A🦀Z",
            "rejection added no undo entry"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn only_exact_some_marked_commit_cleanup_is_suppressed(cx: &mut TestAppContext) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        for range in [Some(0..1), Some(1..2)] {
            japanese_preedit(&input, window, cx);
            reject_commit(&input, &boundary, window, cx);
            let mut proxy = handler(&input, &boundary, cx);
            proxy.replace_text_in_range(range.clone(), "", window, cx);
            assert_eq!(
                input.read(cx).value(),
                match range {
                    None => "AZ",
                    Some(Range { start: 0, end: 1 }) => "に🦀Z",
                    _ => "A🦀Z",
                }
            );
            assert_eq!(proxy.marked_text_range(window, cx), None);
        }
        japanese_preedit(&input, window, cx);
        reject_commit(&input, &boundary, window, cx);
        handler(&input, &boundary, cx).replace_text_in_range(None, "", window, cx);
        assert_eq!(
            input.read(cx).value(),
            "Aに🦀Z",
            "None implicitly targets the live mark"
        );
        handler(&input, &boundary, cx).replace_text_in_range(None, "", window, cx);
        assert_eq!(input.read(cx).value(), "AZ");
        japanese_preedit(&input, window, cx);
        reject_commit(&input, &boundary, window, cx);
        handler(&input, &boundary, cx).replace_and_mark_text_in_range(None, "", None, window, cx);
        assert_eq!(
            input.read(cx).value(),
            "Aに🦀Z",
            "empty Wayland preedit cleanup is rejected once"
        );
        handler(&input, &boundary, cx).replace_and_mark_text_in_range(None, "", None, window, cx);
        assert_eq!(
            input.read(cx).value(),
            "AZ",
            "the next explicit cancellation remains native"
        );
        japanese_preedit(&input, window, cx);
        handler(&input, &boundary, cx).replace_and_mark_text_in_range(
            None,
            "bad\npreedit",
            None,
            window,
            cx,
        );
        handler(&input, &boundary, cx).replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(
            input.read(cx).value(),
            "Aに🦀Z",
            "rejected preedit also preserves its prior composition through cleanup"
        );
        handler(&input, &boundary, cx).replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(input.read(cx).value(), "AZ");
    })
    .unwrap();
}

#[gpui_kit::test]
fn changed_native_document_selection_or_mark_invalidates_cleanup(cx: &mut TestAppContext) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        for change in ["document", "selection", "mark"] {
            japanese_preedit(&input, window, cx);
            reject_commit(&input, &boundary, window, cx);
            // Bypass the adapter with real EntityInputHandler/model operations.
            input.update(cx, |s, cx| match change {
                "document" => {
                    s.replace_and_mark_text_in_range(Some(1..4), "な🦀", Some(1..3), window, cx)
                }
                "selection" => s.set_selected_range(0..1, cx),
                _ => s.replace_and_mark_text_in_range(Some(0..1), "A", Some(0..1), window, cx),
            });
            let mut proxy = handler(&input, &boundary, cx);
            let marked = proxy.marked_text_range(window, cx).unwrap();
            proxy.replace_text_in_range(Some(marked), "", window, cx);
            assert_eq!(
                input.read(cx).value(),
                if change == "mark" { "に🦀Z" } else { "AZ" }
            );
            assert_eq!(proxy.marked_text_range(window, cx), None);
        }
        // Even a selection callback that sets the identical selection is a new
        // user action and invalidates the pending cleanup.
        japanese_preedit(&input, window, cx);
        reject_commit(&input, &boundary, window, cx);
        let mut proxy = handler(&input, &boundary, cx);
        proxy.set_selected_text_range(2..4, window, cx);
        proxy.replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(input.read(cx).value(), "AZ");
    })
    .unwrap();
}

#[gpui_kit::test]
fn focus_and_editability_changes_expire_cleanup_without_native_mutation(cx: &mut TestAppContext) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        japanese_preedit(&input, window, cx);
        reject_commit(&input, &boundary, window, cx);
        let mut stale = handler(&input, &boundary, cx);
        window.press("tab", cx);
        let blurred = Snapshot::read(&input, window, cx);
        stale.replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(Snapshot::read(&input, window, cx), blurred);
        window.press("shift-tab", cx);
        stale.replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(input.read(cx).value(), "AZ");

        japanese_preedit(&input, window, cx);
        reject_commit(&input, &boundary, window, cx);
        input.update(cx, |s, cx| s.set_readonly(true, cx));
        let readonly = Snapshot::read(&input, window, cx);
        stale.replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(Snapshot::read(&input, window, cx), readonly);
        input.update(cx, |s, cx| s.set_readonly(false, cx));
        stale.replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(input.read(cx).value(), "AZ");
    })
    .unwrap();
}

#[gpui_kit::test]
fn blur_return_invalidates_cleanup_even_when_the_target_is_identical(cx: &mut TestAppContext) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    let target = cx
        .update_window(handle, |_, window, cx| {
            japanese_preedit(&input, window, cx);
            reject_commit(&input, &boundary, window, cx);
            PasteTarget::read(&input, window, cx)
        })
        .unwrap();
    let serial = boundary.read_with(cx, |s, _| s.request_serial);
    cx.update_window(handle, |_, window, cx| window.press("tab", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.press("shift-tab", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert!(PasteTarget::read(&input, window, cx) == target);
        assert!(boundary.read(cx).request_serial > serial);
        handler(&input, &boundary, cx).replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(input.read(cx).value(), "AZ");
    })
    .unwrap();
}

#[gpui_kit::test]
fn valid_preedit_unmark_and_paste_clear_cleanup_even_without_target_changes(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        for action in ["preedit", "unmark", "paste"] {
            japanese_preedit(&input, window, cx);
            reject_commit(&input, &boundary, window, cx);
            let before = Snapshot::read(&input, window, cx);
            let mut proxy = handler(&input, &boundary, cx);
            match action {
                "preedit" => {
                    proxy.replace_and_mark_text_in_range(Some(1..4), "に🦀", Some(1..3), window, cx)
                }
                "unmark" => {
                    proxy.unmark_text(window, cx);
                    input.update(cx, |s, cx| {
                        s.replace_and_mark_text_in_range(Some(1..4), "に🦀", Some(1..3), window, cx)
                    });
                }
                _ => proxy.paste(ClipboardItem::new_string(String::new()), window, cx),
            }
            assert_eq!(Snapshot::read(&input, window, cx), before);
            proxy.replace_text_in_range(Some(1..4), "", window, cx);
            assert_eq!(
                input.read(cx).value(),
                "AZ",
                "{action} left a stale cleanup gate"
            );
        }
        japanese_preedit(&input, window, cx);
        reject_commit(&input, &boundary, window, cx);
        handler(&input, &boundary, cx).replace_text_in_range(None, "valid", window, cx);
        assert_eq!(input.read(cx).value(), "AvalidZ");
        assert!(boundary.read(cx).warning.is_none());
        input.update(cx, |s, cx| {
            s.replace_and_mark_text_in_range(Some(1..6), "に🦀", Some(1..3), window, cx)
        });
        handler(&input, &boundary, cx).replace_text_in_range(Some(1..4), "", window, cx);
        assert_eq!(input.read(cx).value(), "AZ");
    })
    .unwrap();
}

#[gpui_kit::test]
fn real_native_backspace_is_not_intercepted_by_pending_cleanup(cx: &mut TestAppContext) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        japanese_preedit(&input, window, cx);
        reject_commit(&input, &boundary, window, cx);
        window.render_frame(cx);
        window.press("backspace", cx);
        assert_ne!(input.read(cx).value(), "Aに🦀Z");
        assert!(
            input
                .update(cx, |s, cx| s.marked_text_range(window, cx))
                .is_none()
        );
    })
    .unwrap();
}
