use super::{Boundary, Rejection, editable_and_focused, forbidden};
use gpui_kit::{
    App, Bounds, ClipboardItem, ElementInputHandler, Entity, Focusable, InputHandler, Pixels,
    Point, TextInputConfiguration, UTF16Selection, Window, component::input::InputState,
};
use std::ops::Range;

/// Installed by the guard's trailing paint canvas, ahead of native normalization.
pub(super) struct GuardedHandler {
    native: ElementInputHandler<InputState>,
    input: Entity<InputState>,
    boundary: Entity<Boundary>,
}
impl GuardedHandler {
    pub(super) fn new(
        native: ElementInputHandler<InputState>,
        input: Entity<InputState>,
        boundary: Entity<Boundary>,
    ) -> Self {
        Self {
            native,
            input,
            boundary,
        }
    }

    fn allow(
        &mut self,
        text: &str,
        replacement_range: Option<&Range<usize>>,
        commit: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if !editable_and_focused(&self.input, window, cx) {
            self.boundary
                .update(cx, |state, _| state.ime_rejection.clear());
            return false;
        }
        self.boundary.update(cx, |state, cx| {
            if commit
                && state.ime_rejection.rejects_cleanup(
                    &self.input,
                    replacement_range,
                    text,
                    (state.revision, state.request_serial),
                    window,
                    cx,
                )
            {
                return false;
            }
            state.request_serial += 1;
            if text.chars().any(forbidden) {
                state.ime_rejection.record(
                    &self.input,
                    (state.revision, state.request_serial),
                    window,
                    cx,
                );
                state.reject(Rejection::Forbidden, cx);
                false
            } else {
                state.ime_rejection.clear();
                if state.warning.take().is_some() {
                    cx.notify();
                }
                true
            }
        })
    }
}

impl InputHandler for GuardedHandler {
    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.native
            .selected_text_range(ignore_disabled_input, window, cx)
    }

    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.native.marked_text_range(window, cx)
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.native
            .text_for_range(range_utf16, adjusted_range, window, cx)
    }

    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        let marked = if text.is_empty() && replacement_range.is_none() {
            self.native.marked_text_range(window, cx)
        } else {
            None
        };
        let cleanup_range = replacement_range.as_ref().or(marked.as_ref());
        if self.allow(text, cleanup_range, true, window, cx) {
            self.native
                .replace_text_in_range(replacement_range, text, window, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if new_text.is_empty() {
            let marked = self.native.marked_text_range(window, cx);
            let cleanup_range = range_utf16.as_ref().or(marked.as_ref());
            let rejected_cleanup = self.boundary.update(cx, |state, cx| {
                state.ime_rejection.rejects_cleanup(
                    &self.input,
                    cleanup_range,
                    new_text,
                    (state.revision, state.request_serial),
                    window,
                    cx,
                )
            });
            if rejected_cleanup {
                return;
            }
        }
        if self.allow(new_text, range_utf16.as_ref(), false, window, cx) {
            self.native.replace_and_mark_text_in_range(
                range_utf16,
                new_text,
                new_selected_range,
                window,
                cx,
            );
        }
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        let marked = self.native.marked_text_range(window, cx);
        let rejected_cleanup = self.boundary.update(cx, |state, cx| {
            state.ime_rejection.rejects_cleanup(
                &self.input,
                marked.as_ref(),
                "",
                (state.revision, state.request_serial),
                window,
                cx,
            )
        });
        if rejected_cleanup {
            return;
        }
        if editable_and_focused(&self.input, window, cx) {
            self.boundary
                .update(cx, |state, _| state.request_serial += 1);
            self.native.unmark_text(window, cx);
            self.input.update(cx, |_, cx| cx.notify());
        }
    }

    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut App) {
        self.boundary.update(cx, |state, cx| {
            state.ime_rejection.clear();
            state.request_serial += 1;
            state.insert_clipboard(item, window, cx);
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.native.bounds_for_range(range_utf16, window, cx)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.native.character_index_for_point(point, window, cx)
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.boundary
            .update(cx, |state, _| state.ime_rejection.clear());
        // Readonly inputs still allow native selection, but a stale platform
        // adapter must not move an input that has since lost focus.
        if self.input.focus_handle(cx).is_focused(window) {
            self.boundary
                .update(cx, |state, _| state.request_serial += 1);
            self.native.set_selected_text_range(range_utf16, window, cx);
        }
    }

    fn element_bounds(&mut self, window: &mut Window, cx: &mut App) -> Option<Bounds<Pixels>> {
        self.native.element_bounds(window, cx)
    }

    fn text_length_utf16(&mut self, window: &mut Window, cx: &mut App) -> Option<usize> {
        self.native.text_length_utf16(window, cx)
    }

    fn apple_press_and_hold_enabled(&mut self) -> bool {
        self.native.apple_press_and_hold_enabled()
    }

    fn accepts_text_input(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.native.accepts_text_input(window, cx)
    }

    fn text_input_editable_range(
        &mut self,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Range<usize>> {
        self.native.text_input_editable_range(window, cx)
    }

    fn prefers_ime_for_printable_keys(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.native.prefers_ime_for_printable_keys(window, cx)
    }

    fn text_input_configuration(
        &mut self,
        window: &mut Window,
        cx: &mut App,
    ) -> TextInputConfiguration {
        self.native.text_input_configuration(window, cx)
    }
}
