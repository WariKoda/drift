use super::*;
#[path = "tests/persistence.rs"]
mod persistence;
use gpui_kit::component::{
    button::Button,
    input::{Input, InputState},
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, EntityInputHandler as _, FocusHandle, InputEvent, KeyBinding,
    KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Point,
    Render, StatefulInteractiveElement as _, TestAppContext, WindowBounds, WindowOptions, point,
    size,
};
use std::cell::Cell;

gpui_kit::actions!(pane_split_test, [Left, Right, Reset]);

struct Child {
    focus: FocusHandle,
    cancelled: Rc<Cell<usize>>,
    clicks: Rc<Cell<usize>>,
    input: Option<Entity<InputState>>,
}
impl Render for Child {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let cancelled = self.cancelled.clone();
        let clicks = self.clicks.clone();
        div()
            .id("focus-child")
            .size_full()
            .track_focus(&self.focus)
            .key_context("SplitTest")
            .on_action(move |_: &crate::actions::Cancel, _, _| cancelled.set(cancelled.get() + 1))
            .on_action({
                let cancelled = self.cancelled.clone();
                move |_: &crate::actions::ClearMarks, _, _| cancelled.set(cancelled.get() + 1)
            })
            .on_action({
                let cancelled = self.cancelled.clone();
                move |_: &gpui_kit::base::actions::Cancel, _, _| cancelled.set(cancelled.get() + 1)
            })
            .on_drag((), |_, _, _, cx| {
                cx.new(|cx| Child {
                    focus: cx.focus_handle(),
                    cancelled: Rc::new(Cell::new(0)),
                    clicks: Rc::new(Cell::new(0)),
                    input: None,
                })
            })
            .child(div().w(px(2000.)).h(px(20.)))
            .child(
                Button::new("child-button")
                    .label("Activate")
                    .on_click(move |_, _, _| {
                        clicks.set(clicks.get() + 1);
                    }),
            )
            .children(
                self.input
                    .as_ref()
                    .map(|input| Input::new(input).id("composition")),
            )
    }
}

struct Harness {
    split: PaneSplit,
    children: [Entity<Child>; 2],
    visible: bool,
    padding: Pixels,
}
impl Render for Harness {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.visible {
            self.split.view(
                "test-split",
                self.children[0].clone().into_any_element(),
                self.children[1].clone().into_any_element(),
                window,
                cx,
            )
        } else {
            self.split.deactivate(window, cx);
            div().into_any_element()
        };
        div()
            .size_full()
            .p(self.padding)
            .capture_action(cx.listener(|this, _: &crate::actions::Cancel, window, cx| {
                this.split.cancel_resize(window, cx);
            }))
            .capture_action(
                cx.listener(|this, _: &crate::actions::ClearMarks, window, cx| {
                    this.split.cancel_resize(window, cx);
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                    this.split.cancel_resize(window, cx);
                }),
            )
            .capture_action(cx.listener(
                |this, _: &gpui_kit::component::input::Escape, window, cx| {
                    this.split.cancel_resize(window, cx);
                },
            ))
            .on_action(cx.listener(|this, _: &Left, window, cx| {
                this.split.command(SplitCommand::Left, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Right, window, cx| {
                this.split.command(SplitCommand::Right, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Reset, window, cx| {
                this.split.command(SplitCommand::Reset, window, cx)
            }))
            .child(body)
    }
}

fn open(cx: &mut TestAppContext, initial: Option<Pixels>) -> (AnyWindowHandle, Entity<Harness>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-alt-left", Left, Some("SplitTest")),
            KeyBinding::new("ctrl-alt-right", Right, Some("SplitTest")),
            KeyBinding::new("ctrl-alt-0", Reset, Some("SplitTest")),
            KeyBinding::new("escape", crate::actions::ClearMarks, Some("SplitTest")),
            KeyBinding::new("alt-escape", crate::actions::Cancel, Some("SplitTest")),
            KeyBinding::new(
                "shift-escape",
                gpui_kit::base::actions::Cancel,
                Some("SplitTest"),
            ),
        ]);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1000.), px(500.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let children = std::array::from_fn(|_| {
                        cx.new(|cx| Child {
                            focus: cx.focus_handle(),
                            cancelled: Rc::new(Cell::new(0)),
                            clicks: Rc::new(Cell::new(0)),
                            input: None,
                        })
                    });
                    children[0].read(cx).focus.clone().focus(window, cx);
                    Harness {
                        split: PaneSplit::new(initial, cx),
                        children,
                        visible: true,
                        padding: px(0.),
                    }
                })
            },
        )
        .unwrap()
    })
}

fn assert_sizes(window: &mut Window, cx: &mut App, expected_first: Pixels) {
    window.render_frame(cx);
    window.render_frame(cx);
    let first = window.find("split-first").bounds().size.width;
    let second = window.find("split-second").bounds().size.width;
    let width = window.find("test-split").bounds().size.width;
    assert!(first.as_f32().is_finite() && second.as_f32().is_finite());
    let minimum = MIN_PANE.min(width / 2.);
    assert!(
        first >= minimum && second >= minimum,
        "{first:?}, {second:?}, min {minimum:?}"
    );
    assert_eq!(first + second, width);
    assert!(
        (first - expected_first).abs() < px(0.01),
        "{first:?} != {expected_first:?}"
    );
}

fn start_drag(window: &mut Window, cx: &mut App, target: Pixels) -> Point<Pixels> {
    window.render_frame(cx);
    let from = window.find("pane-divider").bounds().center();
    window.dispatch_event(
        MouseDownEvent {
            button: MouseButton::Left,
            position: from,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
    let to = point(target, from.y);
    for position in [point(from.x + px(10.), from.y), to] {
        window.dispatch_event(
            MouseMoveEvent {
                position,
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    }
    assert!(cx.has_active_drag());
    to
}

#[gpui_kit::test]
fn keyboard_defaults_and_drag_preserve_child_entities_and_focus(cx: &mut TestAppContext) {
    for (initial, default_first) in [(None, px(500.)), (Some(px(350.)), px(350.))] {
        let (handle, harness) = open(cx, initial);
        cx.update_window(handle, |_, window, cx| {
            assert_sizes(window, cx, default_first);
            let children = harness.read(cx).children.clone();
            for child in &children {
                child.read(cx).focus.clone().focus(window, cx);
                let focus = window.focused(cx).unwrap();
                window.press("ctrl-alt-left", cx);
                assert_sizes(window, cx, default_first - STEP);
                window.press("ctrl-alt-right", cx);
                assert_sizes(window, cx, default_first);
                let from = window.find("pane-divider").bounds().center();
                window.drag(from, point(px(700.), from.y), cx);
                assert_sizes(window, cx, px(700.));
                window.press("ctrl-alt-0", cx);
                assert_sizes(window, cx, default_first);
                assert_eq!(window.focused(cx), Some(focus));
            }
            for _ in 0..40 {
                window.press("ctrl-alt-left", cx);
            }
            assert_sizes(window, cx, MIN_PANE);
            for _ in 0..40 {
                window.press("ctrl-alt-right", cx);
            }
            assert_sizes(window, cx, px(1000.) - MIN_PANE);
            window.press("ctrl-alt-0", cx);
            assert_sizes(window, cx, default_first);
            assert_eq!(harness.read(cx).children, children);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn narrow_grow_and_remount_retain_unclamped_relative_preference(cx: &mut TestAppContext) {
    let (handle, harness) = open(cx, None);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find("pane-divider").bounds().center();
        window.drag(from, point(px(800.), from.y), cx);
        assert_sizes(window, cx, px(800.));
    })
    .unwrap();
    for width in [300., 180., 1., 1000., 997.5, 300., 1000.] {
        cx.simulate_window_resize(handle, size(px(width), px(500.)));
        cx.update_window(handle, |_, window, cx| {
            let minimum = 120_f32.min(width / 2.);
            assert_sizes(
                window,
                cx,
                px((width * 0.8).clamp(minimum, width - minimum)),
            );
            if width <= 240. {
                window.press("ctrl-alt-left", cx);
                window.press("ctrl-alt-right", cx);
                assert_sizes(window, cx, px(width / 2.));
            }
            if width == 300. {
                // Pushing an already-clamped divider farther against its
                // limit must not replace the latent 80% preference with 60%.
                let from = window.find("pane-divider").bounds().center();
                window.drag(from, point(px(280.), from.y), cx);
                assert_sizes(window, cx, px(180.));
            }
        })
        .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        let focus = window.focused(cx);
        harness.update(cx, |harness, cx| {
            harness.visible = false;
            cx.notify();
        });
        window.render_frame(cx);
        let epoch = harness.read(cx).split.session.borrow().epoch;
        harness.update(cx, |harness, cx| harness.split.deactivate(window, cx));
        assert_eq!(harness.read(cx).split.session.borrow().epoch, epoch);
        harness.update(cx, |harness, cx| {
            harness.visible = true;
            cx.notify();
        });
        assert_sizes(window, cx, px(800.));
        assert_eq!(window.focused(cx), focus);
    })
    .unwrap();
}

#[gpui_kit::test]
fn escape_cancels_only_owned_resize_and_blocks_child_actions(cx: &mut TestAppContext) {
    let (handle, harness) = open(cx, None);
    cx.update_window(handle, |_, window, cx| {
        for key in ["escape", "alt-escape", "shift-escape"] {
            let before = harness.read(cx).children[0].read(cx).cancelled.get();
            let focus = window.focused(cx);
            start_drag(window, cx, px(680.));
            window.press(key, cx);
            assert!(!cx.has_active_drag());
            assert_eq!(window.focused(cx), focus);
            assert_eq!(
                harness.read(cx).children[0].read(cx).cancelled.get(),
                before
            );
            window.dispatch_event(
                MouseMoveEvent {
                    position: point(px(900.), px(100.)),
                    pressed_button: None,
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            assert_sizes(window, cx, px(680.));
            window.press(key, cx);
            assert_eq!(
                harness.read(cx).children[0].read(cx).cancelled.get(),
                before + 1
            );
        }
        // A child-owned native drag must survive hiding the split.
        window.render_frame(cx);
        let from = window.find("split-first").bounds().center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseMoveEvent {
                position: from + point(px(20.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert!(cx.has_active_drag());
        // Native handles still display a press/drag when drag initiation was
        // blocked by somebody else's active drag. That is not our gesture.
        let divider = window.find("pane-divider").bounds().center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: divider,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        window.dispatch_event(
            MouseMoveEvent {
                position: divider + point(px(20.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        harness.update(cx, |harness, cx| harness.split.deactivate(window, cx));
        assert!(cx.has_active_drag());
        cx.stop_active_drag(window);
    })
    .unwrap();
}

#[gpui_kit::test]
fn hide_resize_and_late_completion_cannot_revive_old_gestures(cx: &mut TestAppContext) {
    let (handle, harness) = open(cx, None);
    cx.update_window(handle, |_, window, cx| {
        let to = start_drag(window, cx, px(720.));
        // Queue a real completion, then replace its epoch before the deferred
        // preference callback runs. It must not overwrite Reset.
        window.dispatch_event(
            MouseUpEvent {
                position: to,
                button: MouseButton::Left,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        harness.update(cx, |harness, cx| {
            harness.split.command(SplitCommand::Reset, window, cx)
        });
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert_sizes(window, cx, px(500.));
        start_drag(window, cx, px(650.));
        harness.update(cx, |harness, cx| {
            harness.visible = false;
            harness.split.deactivate(window, cx);
        });
        assert!(!cx.has_active_drag());
        window.render_frame(cx);
        harness.update(cx, |harness, _| harness.visible = true);
        assert_sizes(window, cx, px(650.));
        start_drag(window, cx, px(700.));
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(600.), px(500.)));
    cx.update_window(handle, |_, window, cx| {
        assert_sizes(window, cx, px(420.));
        assert!(!cx.has_active_drag());
        window.dispatch_event(
            MouseMoveEvent {
                position: point(px(500.), px(100.)),
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        assert_sizes(window, cx, px(420.));
    })
    .unwrap();
}

// These deliberately dispatch without drawing: ownership must not depend on
// the next handle-appearance callback, nor may drawing flush a deferred mouseup.
fn pointer_down(window: &mut Window, cx: &mut App, position: Point<Pixels>) {
    window.dispatch_event(
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

fn pointer_move(window: &mut Window, cx: &mut App, position: Point<Pixels>) {
    window.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            modifiers: Default::default(),
        }
        .to_platform_input(),
        cx,
    );
}

fn pointer_up(window: &mut Window, cx: &mut App, position: Point<Pixels>) {
    window.dispatch_event(
        MouseUpEvent {
            position,
            button: MouseButton::Left,
            modifiers: Default::default(),
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
}

fn key_down(window: &mut Window, cx: &mut App, key: &str) {
    window.dispatch_event(
        KeyDownEvent {
            keystroke: Keystroke::parse(key).unwrap(),
            is_held: false,
            prefer_character_input: false,
        }
        .to_platform_input(),
        cx,
    );
}

#[gpui_kit::test]
fn keyed_button_focus_and_activation_survive_commands_and_viewport_epochs(cx: &mut TestAppContext) {
    let (handle, harness) = open(cx, None);
    let (focus, children) = cx
        .update_window(handle, |_, window, cx| {
            assert_sizes(window, cx, px(500.));
            window.focus_next(cx);
            window.render_frame(cx);
            assert_eq!(
                window.within("split-first").find("child-button").focused(),
                Some(true)
            );
            let focus = window.focused(cx).unwrap();
            assert_ne!(focus, harness.read(cx).children[0].read(cx).focus);
            let children = harness.read(cx).children.clone();
            for (key, first) in [
                ("ctrl-alt-left", px(500.) - STEP),
                ("ctrl-alt-right", px(500.)),
                ("ctrl-alt-right", px(500.) + STEP),
                ("ctrl-alt-0", px(500.)),
            ] {
                window.press(key, cx);
                assert_sizes(window, cx, first);
                assert_eq!(window.focused(cx), Some(focus.clone()));
                assert_eq!(
                    window.within("split-first").find("child-button").focused(),
                    Some(true)
                );
                let before = children[0].read(cx).clicks.get();
                window.within("split-first").press("enter", cx);
                assert_eq!(children[0].read(cx).clicks.get(), before + 1);
                window.within("split-first").click("child-button", cx);
                assert_eq!(children[0].read(cx).clicks.get(), before + 2);
                assert_eq!(window.focused(cx), Some(focus.clone()));
                assert_eq!(children[1].read(cx).clicks.get(), 0);
            }
            (focus, children)
        })
        .unwrap();
    for (width, height) in [(600., 550.), (600., 600.), (1200., 550.), (1000., 500.)] {
        cx.simulate_window_resize(handle, size(px(width), px(height)));
        cx.update_window(handle, |_, window, cx| {
            assert_sizes(window, cx, px(width / 2.));
            assert_eq!(harness.read(cx).children, children);
            assert_eq!(window.focused(cx), Some(focus.clone()));
            // An orphaned old handle is insufficient: the current button must
            // observe that exact focus and still receive keyboard activation.
            assert_eq!(
                window.within("split-first").find("child-button").focused(),
                Some(true)
            );
            let before = children[0].read(cx).clicks.get();
            window.within("split-first").press("enter", cx);
            assert_eq!(children[0].read(cx).clicks.get(), before + 1);
            window.within("split-first").click("child-button", cx);
            assert_eq!(children[0].read(cx).clicks.get(), before + 2);
            assert_eq!(window.focused(cx), Some(focus.clone()));
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn cancel_or_hide_before_any_drag_frame_clears_native_drag_and_cannot_rearm(
    cx: &mut TestAppContext,
) {
    for cancel in [
        Some("escape"),
        Some("alt-escape"),
        Some("shift-escape"),
        None,
    ] {
        let (handle, harness) = open(cx, None);
        cx.update_window(handle, |_, window, cx| {
            assert_sizes(window, cx, px(500.));
            let from = window.find("pane-divider").bounds().center();
            let focus = window.focused(cx);
            pointer_down(window, cx, from);
            pointer_move(window, cx, from + point(px(20.), px(0.)));
            assert!(cx.has_active_drag());
            if let Some(key) = cancel {
                key_down(window, cx, key);
            } else {
                harness.update(cx, |harness, cx| {
                    harness.visible = false;
                    harness.split.deactivate(window, cx);
                });
            }
            assert!(!cx.has_active_drag());
            assert_eq!(window.focused(cx), focus);
            assert_eq!(harness.read(cx).children[0].read(cx).cancelled.get(), 0);
            pointer_move(window, cx, point(px(900.), from.y));
            assert!(!cx.has_active_drag());
            pointer_up(window, cx, point(px(900.), from.y));
            window.render_frame(cx);
            harness.update(cx, |harness, _| harness.visible = true);
            assert_sizes(window, cx, px(500.));
            pointer_move(window, cx, point(px(800.), from.y));
            window.render_frame(cx);
            assert!(!cx.has_active_drag());
            assert_eq!(harness.read(cx).children[0].read(cx).cancelled.get(), 0);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn input_escape_capture_preserves_real_marked_text_until_resize_is_cancelled(
    cx: &mut TestAppContext,
) {
    let (handle, harness) = open(cx, None);
    cx.update_window(handle, |_, window, cx| {
        let input = cx.new(|cx| InputState::new(window, cx));
        harness.read(cx).children[0]
            .clone()
            .update(cx, |child, cx| {
                child.input = Some(input.clone());
                cx.notify();
            });
        window.render_frame(cx);
        window.within("split-first").click("composition", cx);
        window.input("A🦀Z", cx);
        // Public GPUI IME protocol on the real rendered InputState, not a
        // synthetic composition flag or an OS candidate-window simulation.
        input.update(cx, |state, cx| {
            state.replace_and_mark_text_in_range(Some(1..3), "に🦀", Some(1..3), window, cx);
            assert_eq!(state.marked_text_range(window, cx), Some(1..4));
        });
        window.render_frame(cx);
        let focus = window.focused(cx);
        let selection = input.read(cx).selected_range();
        let from = window.find("pane-divider").bounds().center();
        pointer_down(window, cx, from);
        pointer_move(window, cx, from + point(px(20.), px(0.)));
        assert!(cx.has_active_drag());
        // The input key context selects input::Escape, not SplitTest ClearMarks.
        key_down(window, cx, "escape");
        assert!(!cx.has_active_drag());
        assert_eq!(window.focused(cx), focus);
        input.update(cx, |state, cx| {
            assert_eq!(state.value(), "Aに🦀Z");
            assert_eq!(state.marked_text_range(window, cx), Some(1..4));
            assert_eq!(state.selected_range(), selection);
        });
        assert_eq!(harness.read(cx).children[0].read(cx).cancelled.get(), 0);
        pointer_up(window, cx, from);
        assert_sizes(window, cx, px(500.));
        assert_eq!(
            window.within("split-first").find("composition").value(),
            Some("Aに🦀Z")
        );
        // With no owned resize the same Escape must reach the input and unmark.
        key_down(window, cx, "escape");
        input.update(cx, |state, cx| {
            assert_eq!(state.marked_text_range(window, cx), None);
            assert_eq!(state.value(), "Aに🦀Z");
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn same_epoch_deferred_completion_cannot_clear_the_next_press_or_drag(cx: &mut TestAppContext) {
    for move_before_completion in [false, true] {
        let (handle, harness) = open(cx, None);
        let epoch = cx
            .update_window(handle, |_, window, cx| {
                let to = start_drag(window, cx, px(700.));
                let epoch = harness.read(cx).split.session.borrow().epoch;
                pointer_up(window, cx, to);
                assert!(!cx.has_active_drag());
                assert!(harness.read(cx).split.session.borrow().pending_drag);
                // Mouseup deferred its callback, but the next press is already real.
                let from = window.find("pane-divider").bounds().center();
                pointer_down(window, cx, from);
                if move_before_completion {
                    pointer_move(window, cx, from + point(px(20.), px(0.)));
                    assert!(cx.has_active_drag());
                }
                assert_eq!(
                    harness.read(cx).split.session.borrow().drag_start_first,
                    Some(px(700.))
                );
                epoch
            })
            .unwrap();
        // Leaving update_window drains the old deferred callback without a frame.
        cx.update_window(handle, |_, window, cx| {
            let session = harness.read(cx).split.session.borrow();
            assert_eq!(session.epoch, epoch);
            assert_eq!(session.drag_start_first, Some(px(700.)));
            assert!(!session.pending_drag);
            drop(session);
            let from = window.find("pane-divider").bounds().center();
            pointer_move(window, cx, from + point(px(30.), px(0.)));
            window.render_frame(cx);
            pointer_move(window, cx, point(px(620.), from.y));
            window.render_frame(cx);
            assert!(cx.has_active_drag());
            pointer_up(window, cx, point(px(620.), from.y));
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            assert_sizes(window, cx, px(620.));
            assert!(
                (harness.read(cx).split.session.borrow().preferred.unwrap() - 0.62).abs() < 0.00001
            );
            let from = window.find("pane-divider").bounds().center();
            window.drag(from, point(px(780.5), from.y), cx);
            assert_sizes(window, cx, px(780.5));
            assert_eq!(harness.read(cx).split.session.borrow().epoch, epoch);
        })
        .unwrap();
        cx.simulate_window_resize(handle, size(px(2000.), px(500.)));
        cx.update_window(handle, |_, window, cx| {
            assert_sizes(window, cx, px(1561.));
            assert!(!cx.has_active_drag());
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn inset_bodies_and_clamped_defaults_use_real_layout_bounds(cx: &mut TestAppContext) {
    let (handle, harness) = open(cx, Some(px(350.)));
    cx.update_window(handle, |_, window, cx| {
        harness.update(cx, |harness, cx| {
            harness.padding = px(40.);
            harness.split.command(SplitCommand::Reset, window, cx);
            cx.notify();
        });
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(200.), px(500.)));
    cx.update_window(handle, |_, window, cx| {
        assert_sizes(window, cx, px(60.));
        assert_eq!(window.find("test-split").bounds().origin.x, px(40.));
        assert_eq!(window.find("test-split").bounds().size.width, px(120.));
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(1000.), px(500.)));
    cx.update_window(handle, |_, window, cx| {
        assert_sizes(window, cx, px(350.));
        let from = window.find("pane-divider").bounds().center();
        window.drag(from, point(from.x + px(80.), from.y), cx);
        assert_sizes(window, cx, px(430.5));
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(200.), px(500.)));
    cx.update_window(handle, |_, window, cx| assert_sizes(window, cx, px(60.)))
        .unwrap();
    cx.simulate_window_resize(handle, size(px(1000.), px(500.)));
    cx.update_window(handle, |_, window, cx| assert_sizes(window, cx, px(430.5)))
        .unwrap();
}
