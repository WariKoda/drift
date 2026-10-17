use super::*;
use gpui_kit::{
    AnyElement, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, LayoutId,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, WeakEntity,
};

pub(super) struct Viewport {
    child: AnyElement,
    geometry: Rc<RefCell<text::Geometry>>,
    pane: WeakEntity<DiffPane>,
    revision: u64,
}
impl Viewport {
    pub fn new(
        child: impl IntoElement,
        geometry: Rc<RefCell<text::Geometry>>,
        pane: WeakEntity<DiffPane>,
        revision: u64,
    ) -> Self {
        Self {
            child: child.into_any_element(),
            geometry,
            pane,
            revision,
        }
    }
}
impl IntoElement for Viewport {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Viewport {
    type RequestLayoutState = ();
    type PrepaintState = gpui_kit::Hitbox;
    fn id(&self) -> Option<ElementId> {
        Some("diff-geometry".into())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // Wheel/layout redraws may replay the view without calling Render again.
        self.geometry.borrow_mut().lines.clear();
        (self.child.request_layout(w, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.geometry.borrow_mut().viewport = Some(bounds);
        let hitbox = w.insert_hitbox(bounds, gpui_kit::HitboxBehavior::Normal);
        self.child.prepaint(w, cx);
        hitbox
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Self::PrepaintState,
        w: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(w, cx);
        let revision = self.revision;
        let wheel = self.pane.clone();
        let wheel_hitbox = hitbox.clone();
        let wheel_geometry = self.geometry.clone();
        w.on_mouse_event(move |event: &gpui_kit::ScrollWheelEvent, phase, w, cx| {
            if phase.capture() && wheel_hitbox.is_hovered_at(event.position, w) {
                let _ = wheel.update(cx, |this, cx| {
                    if !this.active
                        || this.revision != revision
                        || !Rc::ptr_eq(&this.geometry, &wheel_geometry)
                    {
                        w.prevent_default();
                        cx.stop_propagation();
                    }
                });
            }
        });
        let down = self.pane.clone();
        let hitbox = hitbox.clone();
        let geometry = self.geometry.clone();
        w.on_mouse_event(move |event: &MouseDownEvent, phase, w, cx| {
            if phase.bubble() && hitbox.is_hovered_at(event.position, w) {
                let _ = down.update(cx, |this, cx| {
                    if this.revision == revision && Rc::ptr_eq(&this.geometry, &geometry) {
                        this.mouse_down(event, w, cx);
                    }
                });
            }
        });
        let movement = self.pane.clone();
        let geometry = self.geometry.clone();
        w.on_mouse_event(move |event: &MouseMoveEvent, phase, w, cx| {
            if phase.bubble() {
                let _ = movement.update(cx, |this, cx| {
                    if this.revision == revision && Rc::ptr_eq(&this.geometry, &geometry) {
                        this.mouse_move(event, w, cx);
                    }
                });
            }
        });
        let up = self.pane.clone();
        let geometry = self.geometry.clone();
        w.on_mouse_event(move |event: &MouseUpEvent, phase, w, cx| {
            if phase.bubble() {
                let _ = up.update(cx, |this, cx| {
                    if this.revision == revision && Rc::ptr_eq(&this.geometry, &geometry) {
                        this.mouse_up(event, w, cx);
                    }
                });
            }
        });
        let completed = self.pane.clone();
        let geometry = self.geometry.clone();
        w.defer(cx, move |w, cx| {
            let _ = completed.update(cx, |this, cx| {
                if this.revision == revision
                    && Rc::ptr_eq(&this.geometry, &geometry)
                    && this.focus.is_focused(w)
                    && let Some(pointer) = this.drag.as_ref().map(|drag| drag.pointer)
                {
                    this.update_drag(pointer, cx);
                }
            });
        });
    }
}
