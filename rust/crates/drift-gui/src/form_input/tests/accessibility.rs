//! The pinned SDK's headless TestWindow has no accessibility activation or
//! simulate_a11y_action API. These tests invoke the production node callbacks
//! with real retained entities, and inspect rendered accessibility properties.
//! They do not claim acceptance through an OS accessibility adapter.
use super::super::{Boundary, PasteTarget, accessibility::Target, guard};
use crate::focus_reveal::FocusReveal;
use gpui_kit::{
    AccessibleAction, AnyElement, AnyWindowHandle, App, AppContext, Bounds, ClipboardItem, Context,
    Element, ElementId, Entity, EntityInputHandler, FocusHandle, Focusable, Global,
    GlobalElementId, InspectorElementId, InteractiveElement, IntoElement, LayoutId, ParentElement,
    Pixels, Render, RenderOnce, SharedString, Styled, Subscription, TestAppContext, Window,
    WindowOptions, accesskit,
    component::input::{InputEvent, InputState},
    div,
    prelude::FluentBuilder as _,
    px,
    test::{TestSupportExt as _, TestWindowExt as _},
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

#[derive(Default)]
struct Boundaries(HashMap<gpui_kit::EntityId, Entity<Boundary>>);
impl Global for Boundaries {}

#[derive(IntoElement)]
struct ObservedGuard {
    input: Entity<InputState>,
    child: AnyElement,
}
impl RenderOnce for ObservedGuard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Resolve the guard's keyed state in the identical rendering namespace.
        let boundary =
            window.with_element_namespace(std::any::type_name::<super::super::Guard>(), |window| {
                window.with_element_namespace("form-input-boundary", |window| {
                    window.use_keyed_state(
                        ("form-input", self.input.entity_id()),
                        cx,
                        |window, cx| Boundary::new(self.input.clone(), window, cx),
                    )
                })
            });
        cx.default_global::<Boundaries>()
            .0
            .insert(self.input.entity_id(), boundary);
        self.child
    }
}

const VALID: &str = " 日本語🦀e\u{301} spaces ";
const UNDO: &str = if cfg!(target_os = "macos") {
    "cmd-z"
} else {
    "ctrl-z"
};
const PASTE: &str = if cfg!(target_os = "macos") {
    "cmd-v"
} else {
    "ctrl-v"
};

struct Fields {
    inputs: [Entity<InputState>; 3],
    readonly: bool,
    disabled: bool,
    reveal: FocusReveal,
    changes: [usize; 3],
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
                    .placeholder(format!("Field {index}"))
                    .masked(index == 1)
                    .validate(move |_, _| {
                        validations.set(validations.get() + 1);
                        true
                    })
            })
        });
        let subscriptions = inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                cx.subscribe(input, move |this, _, event, _| {
                    if matches!(event, InputEvent::Change) {
                        this.changes[index] += 1;
                    }
                })
            })
            .collect();
        Self {
            inputs,
            readonly: false,
            disabled: false,
            reveal: FocusReveal::default(),
            changes: [0; 3],
            validations,
            _subscriptions: subscriptions,
        }
    }
}
impl Render for Fields {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p_4().flex().flex_col().gap_4().children(
            self.inputs.iter().enumerate().map(|(index, input)| {
                div()
                    .id(("slot", index))
                    .w(px(240.))
                    .test_support()
                    .child(ObservedGuard {
                        input: input.clone(),
                        child: guard(input, |native| {
                            div().id(("reveal-bounds", index)).test_support().child(
                                self.reveal.wrap(
                                    ("reveal", index),
                                    native
                                        .id(("native", index))
                                        .w(px(240.))
                                        .when(index == 1, |native| native.mask_toggle())
                                        .readonly(self.readonly && index == 0)
                                        .disabled(self.disabled && index == 0),
                                ),
                            )
                        })
                        .into_any_element(),
                    })
            }),
        )
    }
}
fn fixture(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Fields>) {
    let result = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Fields::new(window, cx))
        })
        .unwrap()
    });
    cx.update_window(result.0, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click(("native", 0usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    result
}
fn input(fields: &Entity<Fields>, index: usize, cx: &TestAppContext) -> Entity<InputState> {
    fields.read_with(cx, |fields, _| fields.inputs[index].clone())
}
fn boundary(input: &Entity<InputState>, _: &mut Window, cx: &mut App) -> Entity<Boundary> {
    cx.global::<Boundaries>().0[&input.entity_id()].clone()
}
fn set_value(input: &Entity<InputState>, raw: &str, window: &mut Window, cx: &mut App) {
    let boundary = boundary(input, window, cx);
    let target = Target::read(boundary.read(cx), cx);
    dispatch(&boundary, &target, raw, window, cx);
}
fn dispatch(
    boundary: &Entity<Boundary>,
    target: &Target,
    raw: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let data = accesskit::ActionData::Value(raw.into());
    boundary.update(cx, |state, cx| {
        state.accessibility_set_value(target, Some(&data), window, cx)
    });
}
#[derive(Debug, PartialEq)]
struct Snapshot {
    value: SharedString,
    selection: Option<(std::ops::Range<usize>, bool)>,
    marked: Option<std::ops::Range<usize>>,
    cursor: usize,
    scroll: gpui_kit::Point<Pixels>,
    focus: Option<FocusHandle>,
}
fn snapshot(input: &Entity<InputState>, window: &mut Window, cx: &mut App) -> Snapshot {
    let target = PasteTarget::read(input, window, cx);
    Snapshot {
        value: target.document,
        selection: target.selection,
        marked: target.marked,
        cursor: input.read(cx).cursor(),
        scroll: input.read(cx).scroll_offset(),
        focus: target.focus,
    }
}

#[gpui_kit::test]
fn raw_accessible_rejection_preserves_preedit_selection_history_and_native_validation(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    for index in 0..2 {
        let input = input(&fields, index, cx);
        cx.update_window(handle, |_, window, cx| {
            window.click(("native", index), cx);
            window.input("A🦀Z", cx);
            input.update(cx, |state, cx| {
                state.replace_and_mark_text_in_range(Some(1..3), "🦀", Some(0..2), window, cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
        let (changes, validations) = fields.read_with(cx, |s, _| (s.changes, s.validations.get()));
        cx.update_window(handle, |_, window, cx| {
            let before = snapshot(&input, window, cx);
            let boundary = boundary(&input, window, cx);
            for code in (0..=31).chain(127..=159).chain([0x2028, 0x2029]) {
                let control = char::from_u32(code).unwrap();
                let raw = format!("secret-before{control}secret-after");
                set_value(&input, &raw, window, cx);
                assert_eq!(snapshot(&input, window, cx), before);
                assert_eq!(
                    boundary.read(cx).warning.unwrap().message(),
                    super::super::Rejection::Forbidden.message()
                );
            }
            set_value(&input, "secret-before\r\nsecret-after", window, cx);
            assert_eq!(snapshot(&input, window, cx), before);
            let warning = boundary.read(cx).warning.unwrap().message();
            assert!(!warning.contains("secret"));
            window.render_frame(cx);
            assert!(
                window.find(("slot", index)).bounds().size.height
                    > window.find(("native", index)).bounds().size.height
            );
        })
        .unwrap();
        fields.read_with(cx, |s, _| {
            assert_eq!(s.changes, changes);
            assert_eq!(s.validations.get(), validations);
        });
        cx.update_window(handle, |_, window, cx| {
            // A rejected SetValue must not interpose an undo entry.
            input.update(cx, |state, cx| state.unmark_text(window, cx));
            window.press(UNDO, cx);
            assert_eq!(input.read(cx).value(), "");
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn unicode_whole_field_accessible_edit_and_paste_have_separate_atomic_undo_entries(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let input = input(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        window.input("A🦀Z", cx);
        input.update(cx, |state, cx| state.set_selected_range(1..5, cx));
        let boundary = boundary(&input, window, cx);
        set_value(&input, "private\r\nvalue", window, cx);
        assert!(boundary.read(cx).warning.is_some());
        let serial = boundary.read(cx).request_serial;
        set_value(&input, VALID, window, cx);
        assert_eq!(input.read(cx).value(), VALID);
        assert_eq!(input.read(cx).selected_range(), VALID.len()..VALID.len());
        assert_eq!(input.read(cx).cursor(), VALID.len());
        assert!(boundary.read(cx).warning.is_none());
        assert!(boundary.read(cx).request_serial > serial);
        cx.write_to_clipboard(ClipboardItem::new_string("後🦀".into()));
        window.press(PASTE, cx);
        assert_eq!(input.read(cx).value(), format!("{VALID}後🦀"));
        window.press(UNDO, cx);
        assert_eq!(input.read(cx).value(), VALID);
        window.press(UNDO, cx);
        assert_eq!(input.read(cx).value(), "A🦀Z");
        window.press(UNDO, cx);
        assert_eq!(input.read(cx).value(), "");
    })
    .unwrap();
}

#[gpui_kit::test]
fn accessible_set_value_preserves_native_unfocused_readonly_and_disabled_semantics(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let input = input(&fields, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click(("native", 1usize), cx);
        let focused = window.focused(cx);
        set_value(&input, VALID, window, cx);
        assert_eq!(input.read(cx).value(), VALID);
        assert_eq!(window.focused(cx), focused, "SetValue must not steal focus");
        let boundary = boundary(&input, window, cx);
        boundary.update(cx, |state, cx| state.accessibility_focus(window, cx));
        assert!(input.focus_handle(cx).is_focused(window));
    })
    .unwrap();
    for disabled in [false, true] {
        fields.update(cx, |s, cx| {
            s.readonly = !disabled;
            s.disabled = disabled;
            cx.notify();
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(("native", 1usize), cx);
            let before = snapshot(&input, window, cx);
            set_value(&input, "allowed", window, cx);
            set_value(&input, "secret\nvalue", window, cx);
            assert_eq!(snapshot(&input, window, cx), before);
            let boundary = boundary(&input, window, cx);
            assert!(boundary.read(cx).warning.is_none());
            boundary.update(cx, |state, cx| state.accessibility_focus(window, cx));
            assert_eq!(input.focus_handle(cx).is_focused(window), !disabled);
            if disabled {
                assert_eq!(
                    window
                        .find(("form-input-content", input.entity_id()))
                        .focused(),
                    None,
                    "disabled mirror must not introduce a focus restoration target"
                );
            }
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn stale_node_context_cannot_replace_a_changed_document_or_bypass_edit_undo_serial(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let input = input(&fields, 0, cx);
    let (boundary, target) = cx
        .update_window(handle, |_, window, cx| {
            window.input("keep🦀", cx);
            let boundary = boundary(&input, window, cx);
            let target = Target::read(boundary.read(cx), cx);
            input.update(cx, |state, cx| state.set_value("loaded", window, cx));
            dispatch(&boundary, &target, "stale", window, cx);
            assert_eq!(input.read(cx).value(), "loaded");
            let target = Target::read(boundary.read(cx), cx);
            window.input("x", cx);
            window.press(UNDO, cx);
            (boundary, target)
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(input.read(cx).value(), "loaded");
        let before = snapshot(&input, window, cx);
        dispatch(&boundary, &target, "stale", window, cx);
        assert_eq!(snapshot(&input, window, cx), before);
        let target = Target::read(boundary.read(cx), cx);
        boundary.update(cx, |state, _| state.request_serial += 1);
        dispatch(&boundary, &target, "stale\nsecret", window, cx);
        assert_eq!(snapshot(&input, window, cx), before);
        assert!(boundary.read(cx).warning.is_none());
        let target = Target::read(boundary.read(cx), cx);
        boundary.update(cx, |state, cx| {
            state.accessibility_set_value(&target, None, window, cx)
        });
        assert_eq!(snapshot(&input, window, cx), before);
    })
    .unwrap();
}

#[gpui_kit::test]
fn mirror_preserves_native_tab_roundtrip_and_control_only_reveal_geometry(cx: &mut TestAppContext) {
    let (handle, fields) = fixture(cx);
    let inputs = fields.read_with(cx, |s, _| s.inputs.clone());
    cx.update_window(handle, |_, window, cx| {
        for _ in 0..3 {
            for input in inputs.iter().skip(1) {
                window.press("tab", cx);
                assert!(input.focus_handle(cx).is_focused(window));
            }
            for index in (0..2).rev() {
                window.press("shift-tab", cx);
                assert!(
                    inputs[index].focus_handle(cx).is_focused(window),
                    "reverse Tab expected {index}, focus {:?}, handles {:?}, snapshots {:?}",
                    window.focused(cx),
                    inputs
                        .iter()
                        .map(|input| input.focus_handle(cx))
                        .collect::<Vec<_>>(),
                    gpui_kit::base::test_support::snapshots(window)
                );
            }
        }
        for (index, input) in inputs.iter().enumerate() {
            let mirror = window.find(("form-input-content", input.entity_id()));
            assert_eq!(mirror.role(), Some(gpui_kit::Role::TextInput));
            assert_eq!(
                mirror.bounds(),
                window.find(("reveal-bounds", index)).bounds()
            );
            assert_eq!(mirror.bounds(), window.find(("native", index)).bounds());
            assert_eq!(window.find(("native", index)).role(), None);
        }
        let input = &inputs[0];
        let before = window
            .find(("form-input-content", input.entity_id()))
            .bounds();
        set_value(input, "secret\r\nvalue", window, cx);
        window.render_frame(cx);
        assert_eq!(
            window
                .find(("form-input-content", input.entity_id()))
                .bounds(),
            before
        );
        assert!(window.find(("slot", 0usize)).bounds().size.height > before.size.height);
        assert_eq!(
            window
                .find(("form-input-content", input.entity_id()))
                .focused(),
            Some(true)
        );
    })
    .unwrap();
    assert_eq!(fields.read_with(cx, |s, _| s.inputs.clone()), inputs);
}

#[gpui_kit::test]
fn masked_accessible_value_is_absent_and_mask_toggle_remains_an_accessible_button(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let masked = input(&fields, 1, cx);
    cx.update_window(handle, |_, window, cx| {
        set_value(&masked, "private-accessibility-secret🦀", window, cx);
        window.render_frame(cx);
        let node = window.find(("form-input-content", masked.entity_id()));
        assert_eq!(node.value(), None);
        assert_eq!(node.label(), Some("Field 1"));
        assert_eq!(window.find(("native", 1usize)).value(), None);
        let snapshots = gpui_kit::base::test_support::snapshots(window);
        assert!(
            snapshots
                .iter()
                .any(|node| node.role() == Some(gpui_kit::Role::Button))
        );
        assert!(!format!("{snapshots:?}").contains("private-accessibility-secret"));
        // No JSON tree is available without platform AccessKit activation.
        assert!(window.debug_a11y_tree_json().is_none());
        window.within(("native", 1usize)).click("toggle-mask", cx);
        assert!(!masked.read(cx).presentation().is_masked());
        assert_eq!(
            window
                .find(("form-input-content", masked.entity_id()))
                .value(),
            Some("private-accessibility-secret🦀")
        );
        window.within(("native", 1usize)).click("toggle-mask", cx);
        assert!(masked.read(cx).presentation().is_masked());
        assert_eq!(
            window
                .find(("form-input-content", masked.entity_id()))
                .value(),
            None
        );
        window.click(("native", 1usize), cx);
        window.press(UNDO, cx);
        assert_eq!(masked.read(cx).value(), "");
    })
    .unwrap();
}

// A transparent probe of the production Control's exact AccessKit node. Unlike
// the test-support snapshot it also observes disabled/read-only/placeholder
// flags. It does not activate an OS adapter or invent an accessibility tree.
struct NodeProbe<E: Element> {
    inner: E,
    snapshot: Rc<RefCell<Option<accesskit::Node>>>,
}
impl<E: Element> IntoElement for NodeProbe<E> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl<E: Element> Element for NodeProbe<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let mut node = accesskit::Node::new(self.inner.a11y_role().unwrap());
        self.inner.write_a11y_info(&mut node);
        *self.snapshot.borrow_mut() = Some(node);
        self.inner
            .prepaint(id, inspector, bounds, layout, window, cx)
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        paint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, paint, window, cx);
    }
    fn a11y_role(&self) -> Option<accesskit::Role> {
        self.inner.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
    }
}
struct Metadata {
    input: Entity<InputState>,
    boundary: Entity<Boundary>,
    disabled: bool,
    readonly: bool,
    snapshot: Rc<RefCell<Option<accesskit::Node>>>,
}
impl Render for Metadata {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        NodeProbe {
            inner: super::super::accessibility::control(
                self.boundary.clone(),
                div().id("metadata-control").child(
                    gpui_kit::component::input::Input::new(&self.input)
                        .role(gpui_kit::base::RoleOverride::Presentational)
                        .disabled(self.disabled)
                        .readonly(self.readonly),
                ),
            )
            .into_element(),
            snapshot: self.snapshot.clone(),
        }
    }
}

#[gpui_kit::test]
fn exact_rendered_node_metadata_updates_without_a_previous_presentation_frame(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let input =
                    cx.new(|cx| InputState::new(window, cx).placeholder("Accessible field"));
                input.update(cx, |state, cx| state.set_value("initial🦀", window, cx));
                let boundary = cx.new(|cx| Boundary::new(input.clone(), window, cx));
                Metadata {
                    input,
                    boundary,
                    disabled: false,
                    readonly: false,
                    snapshot: Rc::default(),
                }
            })
        })
        .unwrap()
    });
    for (disabled, readonly, masked) in [
        (false, false, false),
        (false, true, false),
        (true, false, true),
        (false, false, true),
    ] {
        cx.update_window(handle, |_, window, cx| {
            fields.update(cx, |s, cx| {
                s.disabled = disabled;
                s.readonly = readonly;
                s.input
                    .update(cx, |state, cx| state.set_masked(masked, window, cx));
                cx.notify();
            });
            window.render_frame(cx);
        })
        .unwrap();
        fields.read_with(cx, |s, _| {
            let snapshot = s.snapshot.borrow();
            let node = snapshot.as_ref().unwrap();
            assert_eq!(node.role(), gpui_kit::Role::TextInput);
            assert_eq!(node.is_disabled(), disabled);
            assert_eq!(node.is_read_only(), readonly);
            assert_eq!(node.supports_action(AccessibleAction::Focus), !disabled);
            assert_eq!(
                node.supports_action(AccessibleAction::SetValue),
                !disabled && !readonly
            );
            assert_eq!(node.label(), Some("Accessible field"));
            assert_eq!(node.placeholder(), Some("Accessible field"));
            assert_eq!(node.value(), (!masked).then_some("initial🦀"));
            if masked {
                assert!(!format!("{node:?}").contains("initial🦀"));
            }
        });
    }
    cx.update_window(handle, |_, window, cx| {
        let boundary = fields.read(cx).boundary.clone();
        let target = Target::read(boundary.read(cx), cx);
        dispatch(
            &boundary,
            &target,
            "secret-before\r\nsecret-after",
            window,
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    fields.read_with(cx, |s, _| {
        let snapshot = s.snapshot.borrow();
        let node = snapshot.as_ref().unwrap();
        assert_eq!(
            node.description(),
            Some(super::super::Rejection::Forbidden.message())
        );
        assert_eq!(node.value(), None);
        assert!(!format!("{node:?}").contains("secret-before"));
    });
}
