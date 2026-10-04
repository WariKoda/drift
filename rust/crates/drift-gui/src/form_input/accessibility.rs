use super::{Boundary, Rejection, forbidden};
use gpui_kit::{
    AccessibleAction, App, Bounds, Context, Div, Element, ElementId, Entity, Focusable,
    GlobalElementId, InspectorElementId, InteractiveElement, Interactivity, IntoElement, LayoutId,
    Pixels, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, accesskit,
    prelude::FluentBuilder as _,
};
use gpui_kit::{TestSupportExt as _, base::ObservedElement};

/// A rendered node's context, not an authorization to edit a later document.
#[derive(Clone)]
pub(super) struct Target {
    serial: u64,
    revision: u64,
    document: SharedString,
}
impl Target {
    pub(super) fn read(boundary: &Boundary, cx: &App) -> Self {
        Self {
            serial: boundary.request_serial,
            revision: boundary.revision,
            document: boundary.input.read(cx).value(),
        }
    }
}

impl Boundary {
    pub(super) fn accessibility_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.input.read(cx).presentation().is_disabled() {
            self.input.focus_handle(cx).focus(window, cx);
        }
    }

    pub(super) fn accessibility_set_value(
        &mut self,
        target: &Target,
        data: Option<&accesskit::ActionData>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(accesskit::ActionData::Value(value)) = data else {
            return;
        };
        let state = self.input.read(cx);
        if !state.is_editable()
            || self.request_serial != target.serial
            || self.revision != target.revision
            || state.value() != target.document
        {
            return;
        }
        // Unlike platform text callbacks, accessible SetValue is a whole-field
        // operation and need not hold keyboard focus (matching native Input).
        self.request_serial += 1;
        if value.chars().any(forbidden) {
            self.reject(Rejection::Forbidden, cx);
            return;
        }
        self.ime_rejection.clear();
        self.input.update(cx, |state, cx| {
            // Native replace_all supplies UTF-16 range conversion, validation,
            // a single atomic undo entry, and the ordinary Change event.
            state.replace_all(value.to_string(), window, cx);
        });
        self.warning = None;
        cx.notify();
    }
}

pub(super) fn control(boundary: Entity<Boundary>, inner: Stateful<Div>) -> impl IntoElement {
    Control {
        inner: Some(inner.test_support()),
        boundary,
        disabled: false,
        readonly: false,
        paint_interactivity: None,
    }
}

/// Only adds accessibility metadata to the existing inner div. Native Input
/// renders during layout and updates presentation there, so decorating before
/// its layout would use last frame's disabled/readonly/masked state.
struct Control {
    inner: Option<ObservedElement<Stateful<Div>>>,
    boundary: Entity<Boundary>,
    disabled: bool,
    readonly: bool,
    paint_interactivity: Option<Interactivity>,
}
impl IntoElement for Control {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Control {
    type RequestLayoutState = <ObservedElement<Stateful<Div>> as Element>::RequestLayoutState;
    type PrepaintState = <ObservedElement<Stateful<Div>> as Element>::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        Element::id(self.inner.as_ref().unwrap())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.as_ref().unwrap().source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout = self
            .inner
            .as_mut()
            .unwrap()
            .request_layout(id, inspector, window, cx);
        let boundary = self.boundary.read(cx);
        let input = boundary.input.read(cx);
        let presentation = input.presentation();
        self.disabled = presentation.is_disabled();
        self.readonly = presentation.is_readonly();
        let placeholder = Some(presentation.placeholder().clone()).filter(|p| !p.is_empty());
        let label = placeholder
            .clone()
            .filter(|p| presentation.mask_placeholder() != Some(p.as_ref()));
        let value = (!presentation.is_masked()).then(|| input.value());
        let target = Target::read(boundary, cx);
        let warning = boundary.warning.map(Rejection::message);
        let mut mirror = presentation.focus_handle().clone();
        // Never use the setter: it changes the shared native handle. The SDK's
        // Clone rehydrates flags from that shared handle, including inside
        // track_focus. Thus even this local false flag cannot prevent a second
        // Tab entry; the mirror's tracking must be confined to prepaint.
        mirror.tab_stop = false;
        let decorate = |this: ObservedElement<Stateful<Div>>| {
            let focus = self.boundary.clone();
            let set_value = self.boundary.clone();
            let target = target.clone();
            this.role(gpui_kit::Role::TextInput)
                .when_some(label.clone(), |this, label| this.aria_label(label))
                .when_some(placeholder.clone(), |this, text| {
                    this.aria_placeholder(text)
                })
                .when_some(value.clone(), |this, value| this.aria_value(value))
                .when_some(warning, |this, warning| this.aria_description(warning))
                .when(!presentation.is_disabled(), |this| {
                    this.on_a11y_action(AccessibleAction::Focus, move |_, window, cx| {
                        focus.update(cx, |state, cx| state.accessibility_focus(window, cx));
                    })
                })
                .when(presentation.is_editable(), |this| {
                    this.on_a11y_action(AccessibleAction::SetValue, move |data, window, cx| {
                        set_value.update(cx, |state, cx| {
                            state.accessibility_set_value(&target, data, window, cx)
                        });
                    })
                })
        };
        let mut inner = self.inner.take().unwrap();
        let style = inner.style().clone();
        // This div is never laid out or painted: only its equivalent, untracked
        // interactivity is used for paint. Keep layout, children and observation
        // on the original div, and preserve all its style refinements.
        let mut painter = gpui_kit::div()
            .id(Element::id(&inner).unwrap())
            .test_support();
        *painter.style() = style;
        let mut painter = decorate(painter);
        self.paint_interactivity = Some(std::mem::take(painter.interactivity()));
        self.inner = Some(decorate(inner).when(!self.disabled, |this| this.track_focus(&mirror)));
        layout
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
        let paint = self
            .inner
            .as_mut()
            .unwrap()
            .prepaint(id, inspector, bounds, layout, window, cx);
        // Native child dispatch IDs win during prepaint. Paint the wrapper
        // without focus tracking, so only the native child enters TabStopMap.
        std::mem::swap(
            self.inner.as_mut().unwrap().interactivity(),
            self.paint_interactivity.as_mut().unwrap(),
        );
        paint
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
            .as_mut()
            .unwrap()
            .paint(id, inspector, bounds, layout, paint, window, cx);
    }
    fn a11y_role(&self) -> Option<accesskit::Role> {
        Some(gpui_kit::Role::TextInput)
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.as_ref().unwrap().write_a11y_info(node);
        if self.disabled {
            node.set_disabled();
        }
        if self.readonly {
            node.set_read_only();
        }
    }
}
