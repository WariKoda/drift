//! One-shot focus reveal for controls nested inside an external scroll container.
use gpui_kit::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Pixels, RenderOnce, ScrollHandle, Size, Styled, WeakFocusHandle, Window, canvas, div, px,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct FocusReveal {
    scroll: ScrollHandle,
}
impl FocusReveal {
    pub(crate) fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
    }

    /// Wrap one control without adding a Tab stop. Stable IDs keep native focus
    /// when siblings move; oversized controls can only have their top revealed.
    pub(crate) fn wrap(
        &self,
        id: impl Into<ElementId>,
        child: impl IntoElement,
    ) -> impl IntoElement {
        RevealControl {
            id: id.into(),
            child: child.into_any_element(),
            scroll: self.scroll.clone(),
        }
    }
}

struct RevealState {
    scope: FocusHandle,
    last_focus: Option<WeakFocusHandle>,
    last_viewport: Option<Size<Pixels>>,
    measurement: u64,
}

#[derive(IntoElement)]
struct RevealControl {
    id: ElementId,
    child: AnyElement,
    scroll: ScrollHandle,
}
impl RenderOnce for RevealControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window
            .with_element_namespace("focus-reveal-state", |window| {
                window.use_keyed_state(self.id.clone(), cx, |_, cx| {
                    Rc::new(RefCell::new(RevealState {
                        scope: cx.focus_handle().tab_stop(false),
                        last_focus: None,
                        last_viewport: None,
                        measurement: 0,
                    }))
                })
            })
            .read(cx)
            .clone();
        let scope = state.borrow().scope.clone();
        let scroll = self.scroll;
        let measuring_state = state.clone();
        let measuring_scroll = scroll.clone();
        div()
            .id(self.id)
            .track_focus(&scope)
            .relative()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .child(self.child)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let mut state = measuring_state.borrow_mut();
                        state.measurement += 1;
                        (
                            bounds,
                            measuring_scroll.offset(),
                            measuring_scroll.bounds(),
                            window.focused(cx).as_ref().map(FocusHandle::downgrade),
                            state.measurement,
                        )
                    },
                    move |_,
                          (
                        bounds,
                        measured_offset,
                        measured_viewport,
                        expected_focus,
                        generation,
                    ),
                          window,
                          cx| {
                        let state = state.clone();
                        let scroll = scroll.clone();
                        // Containment needs the completed dispatch tree, after the
                        // rendering entities have been returned to the application.
                        window.defer(cx, move |window, cx| {
                            let mut state = state.borrow_mut();
                            let current = window.focused(cx);
                            let expected =
                                expected_focus.as_ref().and_then(|focus| focus.upgrade());
                            let viewport = scroll.bounds();
                            if state.measurement != generation
                                || current != expected
                                || viewport != measured_viewport
                            {
                                return;
                            }
                            let focused =
                                current.filter(|focus| state.scope.contains(focus, window));
                            let previous =
                                state.last_focus.as_ref().and_then(|focus| focus.upgrade());
                            if focused == previous && state.last_viewport == Some(viewport.size) {
                                return;
                            }
                            if viewport.size.height <= px(0.) || bounds.size.height <= px(0.) {
                                return;
                            }
                            state.last_focus = focused.as_ref().map(FocusHandle::downgrade);
                            state.last_viewport = Some(viewport.size);
                            if focused.is_none() {
                                return;
                            }
                            let mut offset = scroll.offset();
                            // A later wheel/drag takes precedence over this frame's
                            // reveal request; ordinary redraws must not snap back.
                            if offset != measured_offset {
                                return;
                            }
                            let delta = if bounds.top() < viewport.top()
                                || bounds.size.height > viewport.size.height
                            {
                                viewport.top() - bounds.top()
                            } else if bounds.bottom() > viewport.bottom() {
                                viewport.bottom() - bounds.bottom()
                            } else {
                                px(0.)
                            };
                            let next_y = (offset.y + delta).clamp(-scroll.max_offset().y, px(0.));
                            if next_y != offset.y {
                                offset.y = next_y;
                                scroll.set_offset(offset);
                                window.refresh();
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }
}
