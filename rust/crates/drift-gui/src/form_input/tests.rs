//! Protocol tests use the production adapter and real retained InputState entities.
//! Constructing that adapter is not evidence of native OS handler installation or
//! OS IME translation. Keyboard tests below render the actual guard. PasteTarget
//! tests exercise real model updates, not a pending permission-gated OS read.
use super::*;
mod accessibility;
mod ime;
use gpui_kit::test::{TestSupportExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, ClipboardItem, InputHandler, Pixels, Point, Render,
    TestAppContext, WindowBounds, WindowOptions, point, px, size,
};
use std::{cell::Cell, rc::Rc};

const WARNING: &str = "Input rejected: control characters and line separators are not allowed.";
const VALID: &str = " 日本語🦀e\u{301} spaces ";
const PASTE: &str = if cfg!(target_os = "macos") {
    "cmd-v"
} else {
    "ctrl-v"
};
const UNDO: &str = if cfg!(target_os = "macos") {
    "cmd-z"
} else {
    "ctrl-z"
};
const SELECT_ALL: &str = if cfg!(target_os = "macos") {
    "cmd-a"
} else {
    "ctrl-a"
};

struct Fields {
    inputs: [Entity<InputState>; 2],
    boundaries: [Entity<Boundary>; 2],
    readonly: bool,
    changes: [usize; 2],
    submits: [usize; 2],
    owner_cancel: usize,
    owner_enter: usize,
    validations: Rc<Cell<usize>>,
    _subscriptions: Vec<Subscription>,
}

impl Fields {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let validations = Rc::new(Cell::new(0));
        let inputs = std::array::from_fn(|index| {
            let validations = validations.clone();
            cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(index == 1)
                    .validate(move |_, _| {
                        validations.set(validations.get() + 1);
                        true
                    })
            })
        });
        let boundaries = std::array::from_fn(|index| {
            cx.new(|cx| Boundary::new(inputs[index].clone(), window, cx))
        });
        let subscriptions = inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                cx.subscribe(input, move |this, _, event, _| match event {
                    InputEvent::Change => this.changes[index] += 1,
                    InputEvent::PressEnter { .. } => this.submits[index] += 1,
                    _ => {}
                })
            })
            .collect();
        Self {
            inputs,
            boundaries,
            readonly: false,
            changes: [0; 2],
            submits: [0; 2],
            owner_cancel: 0,
            owner_enter: 0,
            validations,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for Fields {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .on_action(cx.listener(|this, _: &Escape, _, _| this.owner_cancel += 1))
            .on_action(cx.listener(|this, _: &Enter, _, _| this.owner_enter += 1))
            .child(
                div()
                    .id("first-slot")
                    .test_support()
                    .child(guard(&self.inputs[0], |input| {
                        input.id("first").w(px(240.)).readonly(self.readonly)
                    })),
            )
            .child(
                div()
                    .id("second-slot")
                    .test_support()
                    .child(guard(&self.inputs[1], |input| {
                        input.id("second").w(px(240.)).readonly(self.readonly)
                    })),
            )
    }
}

fn fixture(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Fields>) {
    let (handle, fields) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(800.), px(420.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| Fields::new(window, cx)),
        )
        .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("first", cx);
    })
    .unwrap();
    cx.run_until_parked();
    (handle, fields)
}

fn entities(
    fields: &Entity<Fields>,
    index: usize,
    cx: &TestAppContext,
) -> (Entity<InputState>, Entity<Boundary>) {
    fields.read_with(cx, |fields, _| {
        (
            fields.inputs[index].clone(),
            fields.boundaries[index].clone(),
        )
    })
}

fn handler(input: &Entity<InputState>, boundary: &Entity<Boundary>, cx: &App) -> GuardedHandler {
    GuardedHandler::new(
        ElementInputHandler::new(input.read(cx).text_bounds().unwrap(), input.clone()),
        input.clone(),
        boundary.clone(),
    )
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    value: gpui_kit::SharedString,
    selection: Option<(Range<usize>, bool)>,
    selected_text: gpui_kit::SharedString,
    marked: Option<Range<usize>>,
    cursor: usize,
    scroll: Point<Pixels>,
    focus: Option<FocusHandle>,
    editable: bool,
    masked: bool,
}

impl Snapshot {
    fn read(input: &Entity<InputState>, window: &mut Window, cx: &mut App) -> Self {
        let focus = window.focused(cx);
        input.update(cx, |state, cx| Self {
            value: state.value(),
            selection: state
                .selected_text_range(false, window, cx)
                .map(|selection| (selection.range, selection.reversed)),
            selected_text: state.selected_value(),
            marked: state.marked_text_range(window, cx),
            cursor: state.cursor(),
            scroll: state.scroll_offset(),
            focus,
            editable: state.is_editable(),
            masked: state.presentation().is_masked(),
        })
    }
}

fn native_preedit(input: &Entity<InputState>, window: &mut Window, cx: &mut App) {
    input.update(cx, |state, cx| {
        state.set_value("A🦀Z", window, cx);
        // EntityInputHandler establishes a native preedit, independent of the guard.
        state.replace_and_mark_text_in_range(Some(1..3), "🦀", Some(0..2), window, cx);
        assert_eq!(state.marked_text_range(window, cx), Some(1..3));
        assert_eq!(
            state.selected_text_range(false, window, cx).unwrap().range,
            1..3
        );
    });
    window.render_frame(cx);
}

fn controls() -> impl Iterator<Item = char> {
    (0..=31)
        .chain(127..=159)
        .chain([0x2028, 0x2029])
        .map(|code| char::from_u32(code).unwrap())
}

#[gpui_kit::test]
fn raw_replacements_reject_every_control_before_native_normalization_or_validation(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    for index in 0..2 {
        let (input, boundary) = entities(&fields, index, cx);
        cx.update_window(handle, |_, window, cx| {
            window.click(if index == 0 { "first" } else { "second" }, cx);
            native_preedit(&input, window, cx);
        })
        .unwrap();
        let (changes, validations) = fields.read_with(cx, |s, _| (s.changes, s.validations.get()));
        cx.update_window(handle, |_, window, cx| {
            let before = Snapshot::read(&input, window, cx);
            let mut proxy = handler(&input, &boundary, cx);
            for control in controls() {
                let payload = format!("private-before{control}private-after");
                for ime in [false, true] {
                    if ime {
                        proxy.replace_and_mark_text_in_range(
                            Some(1..3),
                            &payload,
                            Some(0..1),
                            window,
                            cx,
                        );
                    } else {
                        proxy.replace_text_in_range(Some(1..3), &payload, window, cx);
                    }
                    assert_eq!(Snapshot::read(&input, window, cx), before);
                    assert_eq!(boundary.read(cx).warning.unwrap().message(), WARNING);
                    assert!(
                        !boundary
                            .read(cx)
                            .warning
                            .unwrap()
                            .message()
                            .contains("private-before")
                    );
                    assert!(validate_inputs([&input], window, cx).is_err());
                }
            }
            let bulk = format!("private-bulk\n{}\r\nprivate-end", "日本🦀".repeat(4096));
            proxy.replace_text_in_range(None, &bulk, window, cx);
            proxy.replace_and_mark_text_in_range(None, &bulk, None, window, cx);
            assert_eq!(Snapshot::read(&input, window, cx), before);
        })
        .unwrap();
        fields.read_with(cx, |s, cx| {
            assert_eq!(s.changes, changes, "rejection emitted Change");
            assert_eq!(
                s.validations.get(),
                validations,
                "raw rejection reached native validation"
            );
            assert_eq!(boundary.read(cx).revision, 0);
        });
    }
}

#[gpui_kit::test]
fn utf16_queries_geometry_and_configuration_are_native_and_valid_unicode_is_exact(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    for index in 0..2 {
        let (input, boundary) = entities(&fields, index, cx);
        cx.update_window(handle, |_, window, cx| {
            window.click(if index == 0 { "first" } else { "second" }, cx);
            native_preedit(&input, window, cx);
            let bounds = input.read(cx).text_bounds().unwrap();
            let mut native = ElementInputHandler::new(bounds, input.clone());
            let mut proxy = handler(&input, &boundary, cx);
            for ignore_disabled in [false, true] {
                let a = proxy
                    .selected_text_range(ignore_disabled, window, cx)
                    .unwrap();
                let b = native
                    .selected_text_range(ignore_disabled, window, cx)
                    .unwrap();
                assert_eq!((a.range, a.reversed), (b.range, b.reversed));
            }
            assert_eq!(proxy.marked_text_range(window, cx), Some(1..3));
            assert_eq!(
                proxy.text_length_utf16(window, cx),
                native.text_length_utf16(window, cx)
            );
            let (mut a, mut b) = (None, None);
            assert_eq!(
                proxy.text_for_range(1..3, &mut a, window, cx),
                Some("🦀".into())
            );
            assert_eq!(
                native.text_for_range(1..3, &mut b, window, cx),
                Some("🦀".into())
            );
            assert_eq!(a, b);
            assert_eq!(a, Some(1..3));
            assert_eq!(
                proxy.bounds_for_range(1..3, window, cx),
                native.bounds_for_range(1..3, window, cx)
            );
            assert_eq!(proxy.element_bounds(window, cx), Some(bounds));
            assert_eq!(
                proxy.character_index_for_point(bounds.center(), window, cx),
                native.character_index_for_point(bounds.center(), window, cx)
            );
            assert_eq!(
                proxy.text_input_configuration(window, cx),
                native.text_input_configuration(window, cx)
            );
            assert_eq!(
                proxy.text_input_editable_range(window, cx),
                native.text_input_editable_range(window, cx)
            );
            assert_eq!(
                proxy.accepts_text_input(window, cx),
                native.accepts_text_input(window, cx)
            );
            assert_eq!(
                proxy.prefers_ime_for_printable_keys(window, cx),
                native.prefers_ime_for_printable_keys(window, cx)
            );
            assert_eq!(
                proxy.apple_press_and_hold_enabled(),
                native.apple_press_and_hold_enabled()
            );
            proxy.replace_text_in_range(None, VALID, window, cx);
            assert_eq!(input.read(cx).value(), format!("A{VALID}Z"));
            assert_eq!(proxy.marked_text_range(window, cx), None);
            proxy.replace_and_mark_text_in_range(
                Some(0..input.read(cx).value().encode_utf16().count()),
                VALID,
                Some(0..0),
                window,
                cx,
            );
            assert_eq!(input.read(cx).value(), VALID);
            assert_eq!(
                proxy.marked_text_range(window, cx),
                Some(0..VALID.encode_utf16().count())
            );
            proxy.unmark_text(window, cx);
            assert_eq!(input.read(cx).value(), VALID);
            assert_eq!(proxy.marked_text_range(window, cx), None);
            assert!(boundary.read(cx).warning.is_none());
            assert!(validate_inputs([&input], window, cx).is_ok());
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn readonly_and_blurred_stale_callbacks_preserve_state_but_readonly_selection_delegates(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| native_preedit(&input, window, cx))
        .unwrap();
    fields.update(cx, |fields, cx| {
        fields.readonly = true;
        cx.notify();
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(!input.read(cx).is_editable());
        assert!(input.focus_handle(cx).is_focused(window));
        let mut proxy = handler(&input, &boundary, cx);
        let before = Snapshot::read(&input, window, cx);
        proxy.replace_text_in_range(None, "allowed", window, cx);
        proxy.replace_and_mark_text_in_range(None, "日本", None, window, cx);
        proxy.paste(ClipboardItem::new_string(VALID.into()), window, cx);
        proxy.unmark_text(window, cx);
        assert_eq!(Snapshot::read(&input, window, cx), before);
        let serial = boundary.read(cx).request_serial;
        let bounds = input.read(cx).text_bounds().unwrap();
        let mut native = ElementInputHandler::new(bounds, input.clone());
        native.set_selected_text_range(0..1, window, cx);
        let native_selection = native.selected_text_range(false, window, cx).unwrap();
        proxy.set_selected_text_range(0..1, window, cx);
        let guarded_selection = proxy.selected_text_range(false, window, cx).unwrap();
        assert_eq!(
            (guarded_selection.range, guarded_selection.reversed),
            (native_selection.range, native_selection.reversed)
        );
        assert!(
            boundary.read(cx).request_serial > serial,
            "focused readonly selection must reach the native delegate"
        );
        assert_eq!(proxy.marked_text_range(window, cx), Some(1..3));
        assert!(boundary.read(cx).warning.is_none());
    })
    .unwrap();
    fields.update(cx, |fields, cx| {
        fields.readonly = false;
        cx.notify();
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut stale = handler(&input, &boundary, cx);
        window.press("tab", cx);
        assert_eq!(window.find("second").focused(), Some(true));
        let before = Snapshot::read(&input, window, cx);
        stale.replace_text_in_range(None, "allowed", window, cx);
        stale.replace_and_mark_text_in_range(None, "日本", None, window, cx);
        stale.paste(ClipboardItem::new_string(VALID.into()), window, cx);
        stale.unmark_text(window, cx);
        stale.set_selected_text_range(0..4, window, cx);
        assert_eq!(Snapshot::read(&input, window, cx), before);
        assert!(boundary.read(cx).warning.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn rejected_raw_callbacks_add_neither_change_events_nor_native_undo_entries(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        window.input("A🦀Z", cx);
        let mut proxy = handler(&input, &boundary, cx);
        proxy.paste(ClipboardItem::new_string(VALID.into()), window, cx);
        window.render_frame(cx);
    })
    .unwrap();
    let changes = fields.read_with(cx, |s, _| s.changes);
    let revision = boundary.read_with(cx, |s, _| s.revision);
    cx.update_window(handle, |_, window, cx| {
        let mut proxy = handler(&input, &boundary, cx);
        for control in controls() {
            let payload = format!("private{control}payload");
            proxy.replace_text_in_range(None, &payload, window, cx);
            proxy.replace_and_mark_text_in_range(None, &payload, None, window, cx);
            proxy.paste(ClipboardItem::new_string(payload), window, cx);
        }
    })
    .unwrap();
    fields.read_with(cx, |s, cx| {
        assert_eq!(s.changes, changes);
        assert_eq!(boundary.read(cx).revision, revision);
    });
    cx.update_window(handle, |_, window, cx| {
        window.press(UNDO, cx);
        assert_eq!(input.read(cx).value(), "A🦀Z");
        window.press(UNDO, cx);
        assert_eq!(input.read(cx).value(), "");
    })
    .unwrap();
}

#[gpui_kit::test]
fn actual_guard_keyboard_paste_is_atomic_and_preserves_mask_style_focus_and_entities(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let original = fields.read_with(cx, |s, _| s.inputs.clone());
    for index in 0..2 {
        let (input, _) = entities(&fields, index, cx);
        let id = if index == 0 { "first" } else { "second" };
        cx.update_window(handle, |_, window, cx| {
            window.click(id, cx);
            window.input("A🦀Z", cx);
            window.press(SELECT_ALL, cx);
            assert_eq!(input.read(cx).selected_range(), 0..6);
            cx.write_to_clipboard(ClipboardItem::new_string(VALID.into()));
            window.press(PASTE, cx);
            assert_eq!(input.read(cx).value(), VALID);
            assert_eq!(input.read(cx).cursor(), VALID.len());
            assert_eq!(input.read(cx).selected_range(), VALID.len()..VALID.len());
            assert!(input.focus_handle(cx).is_focused(window));
            assert_eq!(input.read(cx).presentation().is_masked(), index == 1);
            assert_eq!(window.find(id).bounds().size.width, px(240.));
            if index == 1 {
                assert_eq!(window.find(id).value(), None);
            }
        })
        .unwrap();
        let changes = fields.read_with(cx, |s, _| s.changes);
        cx.update_window(handle, |_, window, cx| {
            window.press(SELECT_ALL, cx);
            let before = Snapshot::read(&input, window, cx);
            for control in controls() {
                let payload = format!("private-before{control}private-after");
                cx.write_to_clipboard(ClipboardItem::new_string(payload.clone()));
                window.press(PASTE, cx);
                assert_eq!(Snapshot::read(&input, window, cx), before);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some(payload.as_str())
                );
            }
        })
        .unwrap();
        assert_eq!(fields.read_with(cx, |s, _| s.changes), changes);
        cx.update_window(handle, |_, window, cx| {
            window.press(UNDO, cx);
            assert_eq!(input.read(cx).value(), "A🦀Z");
            window.press(UNDO, cx);
            assert_eq!(input.read(cx).value(), "");
            assert_eq!(fields.read(cx).inputs, original);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn actual_guard_escape_commits_preedit_consumes_owner_cancel_then_propagates(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let (input, _) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        window.input("A🦀Z", cx);
        input.update(cx, |state, cx| {
            state.replace_and_mark_text_in_range(Some(1..3), "日本", Some(2..2), window, cx)
        });
        window.render_frame(cx);
        let composing = Snapshot::read(&input, window, cx);
        cx.write_to_clipboard(ClipboardItem::new_string("private-preedit\r\nnext".into()));
        window.press(PASTE, cx);
        assert_eq!(Snapshot::read(&input, window, cx), composing);
        window.press("escape", cx);
        assert_eq!(input.read(cx).value(), "A日本Z");
        assert!(
            input
                .update(cx, |s, cx| s.marked_text_range(window, cx))
                .is_none()
        );
        assert!(input.focus_handle(cx).is_focused(window));
        assert_eq!(fields.read(cx).owner_cancel, 0);
        window.press("escape", cx);
        assert_eq!(fields.read(cx).owner_cancel, 1);
        window.input("x", cx);
        window.press(UNDO, cx);
        assert_eq!(input.read(cx).value(), "A日本Z");
        window.press(UNDO, cx);
        assert_eq!(
            input.read(cx).value(),
            "A🦀Z",
            "unmark must commit native composition history"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn actual_guard_enter_finishes_preedit_without_submit_or_focus_move_then_is_native(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let (input, _) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        native_preedit(&input, window, cx);
        window.press("enter", cx);
        assert_eq!(input.read(cx).value(), "A🦀Z");
        assert!(
            input
                .update(cx, |s, cx| s.marked_text_range(window, cx))
                .is_none()
        );
        assert!(input.focus_handle(cx).is_focused(window));
        assert_eq!(fields.read(cx).owner_enter, 0);
    })
    .unwrap();
    assert_eq!(fields.read_with(cx, |s, _| s.submits), [0; 2]);
    cx.update_window(handle, |_, window, cx| {
        window.press("enter", cx);
        assert_eq!(fields.read(cx).owner_enter, 1);
        assert!(input.focus_handle(cx).is_focused(window));
        assert_eq!(input.read(cx).value(), "A🦀Z");
    })
    .unwrap();
    assert_eq!(fields.read_with(cx, |s, _| s.submits), [1, 0]);
}

#[gpui_kit::test]
fn paste_target_detects_real_selection_mark_focus_and_editability_changes(cx: &mut TestAppContext) {
    let (handle, fields) = fixture(cx);
    let (input, _) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        window.input("A🦀Z", cx);
        let target = PasteTarget::read(&input, window, cx);
        assert!(PasteTarget::read(&input, window, cx) == target);
        input.update(cx, |state, cx| state.set_value("B🦀Z", window, cx));
        let edited = PasteTarget::read(&input, window, cx);
        assert!(edited.document != target.document);
        assert_eq!(edited.selection, target.selection);
        assert_eq!(edited.marked, target.marked);
        assert_eq!(edited.focus, target.focus);
        assert_eq!(edited.editable, target.editable);
        assert!(edited != target);
        input.update(cx, |state, cx| {
            state.set_value("A🦀Z", window, cx);
            state.set_selected_range(1..5, cx);
        });
        let selected = PasteTarget::read(&input, window, cx);
        assert!(selected != target);
        assert_eq!(selected.document, target.document);
        assert_eq!(selected.selection, Some((1..3, false)));
        window.press("right", cx);
        window.press("shift-left", cx);
        let reversed = PasteTarget::read(&input, window, cx);
        assert_eq!(reversed.selection, Some((1..3, true)));
        assert_eq!(reversed.document, selected.document);
        assert_eq!(reversed.marked, selected.marked);
        assert_eq!(reversed.focus, selected.focus);
        assert!(reversed != selected);
        input.update(cx, |s, cx| {
            s.set_selected_range(1..5, cx);
            s.replace_and_mark_text_in_range(Some(1..3), "🦀", Some(0..2), window, cx)
        });
        let marked = PasteTarget::read(&input, window, cx);
        assert_eq!(marked.document, selected.document);
        assert_eq!(marked.selection, selected.selection);
        assert_eq!(marked.marked, Some(1..3));
        assert!(marked != selected);
        window.press("tab", cx);
        let blurred = PasteTarget::read(&input, window, cx);
        assert_eq!(blurred.document, marked.document);
        assert_eq!(blurred.selection, marked.selection);
        assert_eq!(blurred.marked, marked.marked);
        assert!(blurred.focus != marked.focus);
        assert!(blurred != marked);
    })
    .unwrap();
    let editable = cx
        .update_window(handle, |_, window, cx| {
            PasteTarget::read(&input, window, cx)
        })
        .unwrap();
    fields.update(cx, |fields, cx| {
        fields.readonly = true;
        cx.notify();
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let readonly = PasteTarget::read(&input, window, cx);
        assert!(editable.editable);
        assert!(!readonly.editable);
        assert_eq!(readonly.document, editable.document);
        assert_eq!(readonly.selection, editable.selection);
        assert_eq!(readonly.marked, editable.marked);
        assert_eq!(readonly.focus, editable.focus);
        assert!(readonly != editable);
    })
    .unwrap();
}

#[gpui_kit::test]
fn boundary_revision_detects_edit_undo_and_serial_detects_blur_return(cx: &mut TestAppContext) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    let target = cx
        .update_window(handle, |_, window, cx| {
            window.input("A🦀Z", cx);
            PasteTarget::read(&input, window, cx)
        })
        .unwrap();
    let revision = boundary.read_with(cx, |s, _| s.revision);
    cx.update_window(handle, |_, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(VALID.into()));
        window.press(PASTE, cx);
        window.press(UNDO, cx);
        assert!(PasteTarget::read(&input, window, cx) == target);
    })
    .unwrap();
    assert!(boundary.read_with(cx, |s, _| s.revision) >= revision + 2);
    let serial = boundary.read_with(cx, |s, _| s.request_serial);
    cx.update_window(handle, |_, window, cx| window.press("tab", cx))
        .unwrap();
    cx.run_until_parked();
    assert!(boundary.read_with(cx, |s, _| s.request_serial) > serial);
    cx.update_window(handle, |_, window, cx| {
        window.press("shift-tab", cx);
        assert!(PasteTarget::read(&input, window, cx) == target);
    })
    .unwrap();
    assert!(boundary.read_with(cx, |s, _| s.request_serial) > serial);
}

#[gpui_kit::test]
fn actual_guard_rejection_redraws_warning_without_payload_or_native_geometry_changes(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        window.input("keep", cx);
        let bounds = window.find("first").bounds();
        let slot = window.find("first-slot").bounds();
        let next = window.find("second-slot").bounds();
        let before = Snapshot::read(&input, window, cx);
        // This routes through the rendered guard, not the separately constructed adapter.
        cx.write_to_clipboard(ClipboardItem::new_string(
            "private-warning-payload\nnext".into(),
        ));
        window.press(PASTE, cx);
        window.render_frame(cx);
        assert_eq!(Snapshot::read(&input, window, cx), before);
        assert_eq!(window.find("first").bounds(), bounds);
        assert!(window.find("first-slot").visible());
        assert!(window.find("first-slot").bounds().size.height > slot.size.height);
        assert!(window.find("second-slot").bounds().top() > next.top());
        assert_eq!(
            window.find("second-slot").bounds().size.height,
            next.size.height
        );
        assert_eq!(window.find("first").value(), Some("keep"));
        // Headless observation exposes bounds, not painted text/color. Verify
        // the same rejection's static contents independently of the layout proof.
        let mut proxy = handler(&input, &boundary, cx);
        proxy.replace_text_in_range(None, "private-warning-payload\nnext", window, cx);
        assert_eq!(boundary.read(cx).warning.unwrap().message(), WARNING);
        assert!(!WARNING.contains("private-warning-payload"));
        cx.write_to_clipboard(ClipboardItem::new_string(VALID.into()));
        window.press(PASTE, cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("first-slot").bounds().size.height,
            slot.size.height
        );
        assert_eq!(window.find("second-slot").bounds().top(), next.top());
    })
    .unwrap();
}
