//! Raw, single-line input boundary; the native input remains the editing owner.
mod accessibility;
mod handler;
mod ime_rejection;
#[cfg(test)]
mod tests;

use gpui_kit::{
    AnyElement, App, Context, ElementInputHandler, Entity, EntityInputHandler, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Subscription,
    Window, canvas,
    component::{
        ActiveTheme,
        input::{Enter, Escape, InputEvent, InputState, Paste},
    },
    div,
};
use std::ops::Range;

use handler::GuardedHandler;

#[derive(Clone, Copy)]
enum Rejection {
    Forbidden,
    ClipboardRead,
}
impl Rejection {
    fn message(self) -> &'static str {
        match self {
            Self::Forbidden => {
                "Input rejected: control characters and line separators are not allowed."
            }
            Self::ClipboardRead => "Unable to read the clipboard.",
        }
    }
}

fn forbidden(character: char) -> bool {
    character.is_control() || matches!(character, '\u{2028}' | '\u{2029}')
}

fn editable_and_focused(input: &Entity<InputState>, window: &Window, cx: &App) -> bool {
    let state = input.read(cx);
    state.is_editable() && state.focus_handle(cx).is_focused(window)
}

/// Reject stored inputs before the native single-line state can normalize them.
pub(crate) fn validate_values<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), &'static str> {
    if values.into_iter().any(|value| value.chars().any(forbidden)) {
        Err(
            "Cannot open form: stored values contain control characters or line separators. Repair the configuration outside this form.",
        )
    } else {
        Ok(())
    }
}

/// Check every field, including hidden or blurred mapping fields, before saving.
/// No value is rewritten and error messages never include field contents.
pub(crate) fn validate_inputs<'a>(
    inputs: impl IntoIterator<Item = &'a Entity<InputState>>,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), &'static str> {
    for input in inputs {
        if input
            .update(cx, |state, cx| state.marked_text_range(window, cx))
            .is_some()
        {
            return Err("Finish text composition before saving.");
        }
        if input.read(cx).text().chars().any(forbidden) {
            return Err(Rejection::Forbidden.message());
        }
    }
    Ok(())
}

/// Configure the native control without exposing its unguarded accessibility
/// SetValue action. Keep control-only focus reveal inside `build`, so warnings
/// do not enlarge its reveal bounds.
pub(crate) fn guard<E: IntoElement>(
    input: &Entity<InputState>,
    build: impl FnOnce(gpui_kit::component::input::Input) -> E,
) -> impl IntoElement {
    Guard {
        input: input.clone(),
        child: build(
            gpui_kit::component::input::Input::new(input)
                .role(gpui_kit::base::RoleOverride::Presentational),
        )
        .into_any_element(),
    }
}

struct Boundary {
    input: Entity<InputState>,
    ime_rejection: ime_rejection::ImeRejection,
    warning: Option<Rejection>,
    revision: u64,
    request_serial: u64,
    _subscriptions: Vec<Subscription>,
}

#[derive(PartialEq, Eq)]
struct PasteTarget {
    document: gpui_kit::SharedString,
    selection: Option<(Range<usize>, bool)>,
    marked: Option<Range<usize>>,
    focus: Option<FocusHandle>,
    editable: bool,
}
impl PasteTarget {
    fn read(input: &Entity<InputState>, window: &mut Window, cx: &mut App) -> Self {
        let focus = window.focused(cx);
        input.update(cx, |state, cx| Self {
            document: state.value(),
            selection: state
                .selected_text_range(false, window, cx)
                .map(|selection| (selection.range, selection.reversed)),
            marked: state.marked_text_range(window, cx),
            focus,
            editable: state.is_editable(),
        })
    }
}

impl Boundary {
    fn new(input: Entity<InputState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let changes = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                // Counting events, not comparing values, also detects edit + undo.
                this.revision += 1;
                this.ime_rejection.clear();
                if !input.read(cx).text().chars().any(forbidden) {
                    this.warning = None;
                }
                cx.notify();
            }
        });
        let focus = input.focus_handle(cx);
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            // Leaving and returning during a permission prompt is still stale.
            this.request_serial += 1;
            this.ime_rejection.clear();
            cx.notify();
        });
        Self {
            input,
            ime_rejection: ime_rejection::ImeRejection::default(),
            warning: None,
            revision: 0,
            request_serial: 0,
            _subscriptions: vec![changes, blur],
        }
    }

    fn reject(&mut self, rejection: Rejection, cx: &mut Context<Self>) {
        self.warning = Some(rejection);
        cx.notify();
    }

    fn insert_clipboard(
        &mut self,
        item: gpui_kit::ClipboardItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !editable_and_focused(&self.input, window, cx) {
            return;
        }
        let Some(text) = item.text().filter(|text| !text.is_empty()) else {
            return;
        };
        if text.chars().any(forbidden) {
            self.reject(Rejection::Forbidden, cx);
            return;
        }
        self.ime_rejection.clear();
        self.input.update(cx, |state, cx| {
            // replace is native atomic history, but lifts edit restrictions:
            // the focus/editability check above must precede it.
            state.replace(text, window, cx);
            state.set_cursor_position(state.cursor_position(), window, cx);
            cx.notify();
        });
        self.warning = None;
        cx.notify();
    }

    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_serial += 1;
        cx.notify();
        if !editable_and_focused(&self.input, window, cx) {
            return;
        }
        // Use this actual item, never re-read or rewrite the clipboard.
        if let Some(item) = cx.read_from_clipboard() {
            self.insert_clipboard(item, window, cx);
            return;
        }
        // Start the permission-gated read inside the action's user activation.
        let read = cx.read_from_clipboard_async();
        let target = PasteTarget::read(&self.input, window, cx);
        let revision = self.revision;
        let serial = self.request_serial;
        cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            // A missing boundary/window is normal when a form was dismissed.
            let _ = this.update_in(cx, |this, window, cx| {
                if this.request_serial != serial
                    || this.revision != revision
                    || !editable_and_focused(&this.input, window, cx)
                    || PasteTarget::read(&this.input, window, cx) != target
                {
                    return;
                }
                match result {
                    Ok(Some(item)) => this.insert_clipboard(item, window, cx),
                    Ok(None) => {}
                    Err(_) => this.reject(Rejection::ClipboardRead, cx),
                }
            });
        })
        .detach();
    }

    fn finish_composition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !editable_and_focused(&self.input, window, cx) {
            cx.propagate();
            return;
        }
        let consumed = self.input.update(cx, |state, cx| {
            if state.marked_text_range(window, cx).is_none() {
                return false;
            }
            state.unmark_text(window, cx);
            cx.notify();
            true
        });
        if consumed {
            self.ime_rejection.clear();
            self.request_serial += 1;
            cx.notify();
            cx.stop_propagation();
        } else {
            cx.propagate();
        }
    }
}

#[derive(IntoElement)]
struct Guard {
    input: Entity<InputState>,
    child: AnyElement,
}
impl RenderOnce for Guard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = ("form-input", self.input.entity_id());
        let boundary = window.with_element_namespace("form-input-boundary", |window| {
            window.use_keyed_state(id, cx, |window, cx| {
                Boundary::new(self.input.clone(), window, cx)
            })
        });
        let warning = boundary.read(cx).warning;
        let escape = boundary.clone();
        let enter = boundary.clone();
        let paste = boundary.clone();
        let content_id = ("form-input-content", self.input.entity_id());
        let input = self.input;
        div()
            .id(id)
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .max_w_full()
            .capture_action(move |_: &Escape, window, cx| {
                escape.update(cx, |state, cx| state.finish_composition(window, cx));
            })
            .capture_action(move |_: &Enter, window, cx| {
                enter.update(cx, |state, cx| state.finish_composition(window, cx));
            })
            .capture_action(move |_: &Paste, window, cx| {
                // Also catches native context-menu Paste, which dispatches this
                // action through the input's focus handle.
                if input.focus_handle(cx).is_focused(window) {
                    cx.stop_propagation();
                    paste.update(cx, |state, cx| state.paste(window, cx));
                } else {
                    cx.propagate();
                }
            })
            .child(accessibility::control(
                boundary.clone(),
                div()
                    .id(content_id)
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .min_w_0()
                    .child(self.child)
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |_, _, window, cx| {
                                let input = boundary.read(cx).input.clone();
                                let state = input.read(cx);
                                // Paint after the native child and use its prepaint
                                // bounds, not the wrapper or warning's geometry.
                                if let Some(bounds) = state.text_bounds() {
                                    window.handle_input(
                                        &input.focus_handle(cx),
                                        GuardedHandler::new(
                                            ElementInputHandler::new(bounds, input.clone()),
                                            input.clone(),
                                            boundary.clone(),
                                        ),
                                        cx,
                                    );
                                }
                            },
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    ),
            ))
            .children(warning.map(|warning| {
                div()
                    .mt_1()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(warning.message())
            }))
    }
}
