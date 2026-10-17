//! Two-pane sizing. The owners keep their child entities and focus.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::resizable::{
    ResizableState, h_resizable, resizable_panel, resize_handle_appearance,
};
use gpui_kit::{
    AnyElement, App, AppContext as _, AvailableSpace, Bounds, Entity, HitboxBehavior,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, ParentElement as _, Pixels,
    Styled as _, Window, canvas, div, point, px, size,
};

const MIN_PANE: Pixels = px(120.);
const STEP: Pixels = px(24.);

pub(super) enum SplitCommand {
    Left,
    Right,
    Reset,
}

pub(super) struct PaneSplit {
    session: Rc<RefCell<Session>>,
}

struct Session {
    state: Entity<ResizableState>,
    epoch: usize,
    visible: bool,
    width: Pixels,
    height: Pixels,
    initial_first: Option<Pixels>,
    preferred: Option<f32>,
    user_preferred: Option<f32>,
    changed: Option<Rc<dyn Fn(Option<f32>)>>,
    active: bool,
    dragging: bool,
    pending_drag: bool,
    may_own_drag: bool,
    drag_start_first: Option<Pixels>,
    gesture: u64,
}

impl Session {
    fn sizes(&self) -> [Pixels; 2] {
        let minimum = MIN_PANE.min(self.width / 2.);
        let first = (self.width * self.preferred.unwrap_or_else(|| self.default_fraction()))
            .clamp(minimum, self.width - minimum);
        [first, self.width - first]
    }

    // Kit's sizes are reliable here only within one unchanged, fully laid-out
    // epoch. Never turn a viewport clamp into a new user preference.
    fn remember_drag(&mut self, cx: &App) {
        if !self.dragging && !self.pending_drag {
            return;
        }
        self.remember_sizes(self.state.read(cx).sizes().as_slice().try_into().ok());
    }

    fn remember_sizes(&mut self, sizes: Option<[Pixels; 2]>) {
        let Some([first, second]) = sizes else { return };
        // A press or a drag pinned against the minimum is not a new choice.
        if self
            .drag_start_first
            .is_none_or(|start| (first - start).abs() < px(0.01))
        {
            return;
        }
        let total = first + second;
        if self.width > px(0.)
            && first.as_f32().is_finite()
            && second.as_f32().is_finite()
            && first >= px(0.)
            && second >= px(0.)
            && (total - self.width).abs() < px(0.1)
        {
            let fraction = first / total;
            if fraction > 0. && fraction < 1. {
                self.preferred = Some(fraction);
                if self.user_preferred != Some(fraction) {
                    self.user_preferred = Some(fraction);
                    if let Some(changed) = &self.changed {
                        changed(Some(fraction));
                    }
                }
            }
        }
    }

    fn default_fraction(&self) -> f32 {
        if self.width <= px(0.) {
            return 0.5;
        }
        self.initial_first
            .filter(|size| size.as_f32().is_finite())
            .map_or(0.5, |first| (first / self.width).clamp(0., 1.))
    }

    fn replace(&mut self, window: &mut Window, cx: &mut App) {
        // Ownership was checked at the press. Native drag initiation may have
        // happened since the last frame, before appearance reports Dragging.
        if self.active {
            cx.stop_active_drag(window);
        }
        self.active = false;
        self.dragging = false;
        self.pending_drag = false;
        self.may_own_drag = false;
        self.drag_start_first = None;
        self.epoch += 1;
        self.state = cx.new(|_| ResizableState::default());
        window.refresh();
    }

    fn cancel_owned(&mut self, epoch: usize, window: &mut Window, cx: &mut App) {
        if self.visible && self.epoch == epoch && self.active {
            self.remember_drag(cx);
            self.replace(window, cx);
            cx.stop_propagation();
        } else {
            cx.propagate();
        }
    }
}

impl PaneSplit {
    pub fn new(initial_first: Option<Pixels>, cx: &mut App) -> Self {
        Self {
            session: Rc::new(RefCell::new(Session {
                state: cx.new(|_| ResizableState::default()),
                epoch: 0,
                visible: false,
                width: px(0.),
                height: px(0.),
                initial_first,
                preferred: None,
                user_preferred: None,
                changed: None,
                active: false,
                dragging: false,
                pending_drag: false,
                may_own_drag: false,
                drag_start_first: None,
                gesture: 0,
            })),
        }
    }

    pub fn configure_persistence(
        &mut self,
        preferred: Option<f32>,
        changed: impl Fn(Option<f32>) + 'static,
    ) {
        let mut session = self.session.borrow_mut();
        session.preferred = preferred;
        session.user_preferred = preferred;
        session.changed = Some(Rc::new(changed));
        session.visible = false;
    }

    pub fn view(
        &mut self,
        id: &'static str,
        left: AnyElement,
        right: AnyElement,
        _window: &mut Window,
        _cx: &mut App,
    ) -> AnyElement {
        let session = self.session.clone();
        canvas(
            move |bounds, window, cx| {
                // Linux client decorations inset the content from the viewport.
                // Build against the actual body, not an estimated window width.
                let mut split = PaneSplit { session };
                let mut content = split.render_sized(id, left, right, bounds, window, cx);
                content.layout_as_root(
                    size(
                        AvailableSpace::Definite(bounds.size.width),
                        AvailableSpace::Definite(bounds.size.height),
                    ),
                    window,
                    cx,
                );
                content.prepaint_at(bounds.origin, window, cx);
                content
            },
            |_, mut content, window, cx| content.paint(window, cx),
        )
        .size_full()
        .into_any_element()
    }

    fn render_sized(
        &mut self,
        id: &'static str,
        left: AnyElement,
        right: AnyElement,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        // Rebuild before layout: Kit's proportional adjustment ignores minima.
        let width = bounds.size.width;
        let width = if width.as_f32().is_finite() {
            width.max(px(0.))
        } else {
            px(0.)
        };
        let height = bounds.size.height;
        let (state, epoch, sizes, minimum) = {
            let mut session = self.session.borrow_mut();
            if !session.visible || session.width != width || session.height != height {
                session.remember_drag(cx);
                session.replace(window, cx);
                session.visible = true;
                session.width = width;
                session.height = height;
            }
            if session.preferred.is_none()
                && width > px(0.)
                && session
                    .initial_first
                    .is_none_or(|first| first <= width - MIN_PANE.min(width / 2.))
            {
                session.preferred = Some(session.default_fraction());
            }
            (
                session.state.clone(),
                session.epoch,
                session.sizes(),
                MIN_PANE.min(width / 2.),
            )
        };
        let live_sizes = state
            .read(cx)
            .sizes()
            .as_slice()
            .try_into()
            .ok()
            .filter(|sizes: &[Pixels; 2]| (sizes[0] + sizes[1] - width).abs() < px(0.1))
            .unwrap_or(sizes);
        let appearance = resize_handle_appearance();
        let handle_session = self.session.clone();
        let pointer_session = self.session.clone();
        let resize_session = self.session.clone();

        div()
            .id(id)
            .test_support()
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            // Keep content outside the native gesture epoch: Kit buttons store
            // focus in element-local keyed state, unlike entity-owned inputs.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .child(
                        div()
                            .id("split-first")
                            .test_support()
                            .flex()
                            .flex_none()
                            .w(live_sizes[0])
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .child(left),
                    )
                    .child(
                        div()
                            .id("split-second")
                            .test_support()
                            .flex()
                            .flex_none()
                            .w(live_sizes[1])
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .child(right),
                    ),
            )
            .child(
                h_resizable(("pane-split", epoch))
                    .with_state(&state)
                    .with_handle_appearance(Rc::new(move |handle, window, cx| {
                        {
                            let mut session = handle_session.borrow_mut();
                            if session.visible && session.epoch == epoch {
                                let active = handle.is_active() && session.may_own_drag;
                                if active && !session.active {
                                    session.drag_start_first = Some(session.sizes()[0]);
                                }
                                session.active = active;
                                session.dragging = session.active
                                    && handle.state()
                                        == gpui_kit::base::ResizeHandleState::Dragging;
                            }
                        }
                        Some(
                            div()
                                .id("pane-divider")
                                .test_support()
                                .flex_none()
                                .w(px(1.))
                                .h_full()
                                .children(appearance(handle, window, cx))
                                .into_any_element(),
                        )
                    }))
                    .on_resize(move |state, window, cx| {
                        let gesture = {
                            let mut session = resize_session.borrow_mut();
                            if !session.visible || session.epoch != epoch {
                                return;
                            }
                            session.active = false;
                            session.dragging = false;
                            session.pending_drag = true;
                            session.gesture
                        };
                        // Do not read an Entity from a resize callback. Deferral
                        // also lets the final native drag layout settle first.
                        let session = resize_session.clone();
                        let state = state.clone();
                        window.defer(cx, move |window, cx| {
                            let mut session = session.borrow_mut();
                            if session.visible
                                && session.epoch == epoch
                                && session.gesture == gesture
                                && session.state.entity_id() == state.entity_id()
                            {
                                session.remember_sizes(
                                    state.read(cx).sizes().as_slice().try_into().ok(),
                                );
                                session.pending_drag = false;
                                session.drag_start_first = None;
                                window.refresh();
                            }
                        });
                    })
                    .child(
                        resizable_panel()
                            .size(sizes[0])
                            .size_range(minimum..width - minimum)
                            .flex_none(),
                    )
                    .child(
                        resizable_panel()
                            .size(sizes[1])
                            .size_range(minimum..width - minimum)
                            .flex_none(),
                    ),
            )
            // Observe the press before native drag initiation. A handle can
            // report Dragging even when an existing foreign drag prevented its
            // on_drag callback from running; appearance alone is not ownership.
            .child(
                canvas(
                    move |bounds, window, _| {
                        window.insert_hitbox(
                            Bounds {
                                origin: point(
                                    bounds.origin.x + live_sizes[0] - px(4.),
                                    bounds.origin.y,
                                ),
                                size: size(px(9.), bounds.size.height),
                            },
                            HitboxBehavior::Normal,
                        )
                    },
                    move |_, hitbox, window, _| {
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase.capture() && event.button == MouseButton::Left {
                                let mut session = pointer_session.borrow_mut();
                                if session.visible && session.epoch == epoch {
                                    session.remember_drag(cx);
                                    session.pending_drag = false;
                                    session.dragging = false;
                                    session.gesture += 1;
                                    let owned = hitbox.is_hovered_at(event.position, window)
                                        && !cx.has_active_drag();
                                    session.may_own_drag = owned;
                                    session.active = owned;
                                    session.drag_start_first = owned.then_some(live_sizes[0]);
                                }
                            }
                        });
                    },
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
            .into_any_element()
    }

    pub fn command(&mut self, command: SplitCommand, window: &mut Window, cx: &mut App) {
        let mut session = self.session.borrow_mut();
        if !session.visible || session.width <= px(0.) {
            return;
        }
        session.remember_drag(cx);
        let [first, _] = session.sizes();
        let minimum = MIN_PANE.min(session.width / 2.);
        let previous = session.preferred;
        let reset = matches!(command, SplitCommand::Reset);
        session.preferred = match command {
            SplitCommand::Reset => None,
            SplitCommand::Left | SplitCommand::Right => {
                let delta = match command {
                    SplitCommand::Left => -STEP,
                    _ => STEP,
                };
                let next = (first + delta).clamp(minimum, session.width - minimum);
                if next == first {
                    session.preferred
                } else {
                    Some(next / session.width)
                }
            }
        };
        let preference = session.preferred;
        if (reset || preference != previous) && session.user_preferred != preference {
            session.user_preferred = preference;
            if let Some(changed) = &session.changed {
                changed(preference);
            }
        }
        session.replace(window, cx);
    }

    pub fn is_resizing(&self) -> bool {
        let session = self.session.borrow();
        session.visible && session.active
    }

    pub fn cancel_resize(&mut self, window: &mut Window, cx: &mut App) {
        let mut session = self.session.borrow_mut();
        let epoch = session.epoch;
        session.cancel_owned(epoch, window, cx);
    }

    pub fn deactivate(&mut self, window: &mut Window, cx: &mut App) {
        let mut session = self.session.borrow_mut();
        if session.visible {
            session.remember_drag(cx);
            session.visible = false;
            session.replace(window, cx);
        }
    }
}

#[cfg(test)]
#[path = "pane_split/tests.rs"]
mod tests;
