//! Fixed-side view controls; the shell owns selection and request validity.
use crate::design::{Palette, Text, control, metric, size, space};
use gpui_kit::base::{Disableable, Selectable};
use gpui_kit::component::button::Button;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ClickEvent, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window, div,
};
use std::rc::Rc;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PaneSide {
    Local,
    Remote,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PaneView {
    Files,
    Preview,
}

type Listener = Rc<dyn Fn(PaneView, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PaneTabs {
    side: PaneSide,
    view: PaneView,
    preview_enabled: bool,
    title: SharedString,
    listener: Listener,
    trailing: Option<AnyElement>,
}
impl PaneTabs {
    pub fn new(
        side: PaneSide,
        view: PaneView,
        preview_enabled: bool,
        title: impl Into<SharedString>,
        listener: impl Fn(PaneView, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            side,
            view,
            preview_enabled,
            title: title.into(),
            listener: Rc::new(listener),
            trailing: None,
        }
    }
    /// Pane-owned controls right of the tabs, such as the remote host picker.
    pub fn with_trailing(mut self, trailing: AnyElement) -> Self {
        self.trailing = Some(trailing);
        self
    }
}
impl RenderOnce for PaneTabs {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Palette::current(cx);
        let (id, group_id, files_id, preview_id, files_name, preview_name) = match self.side {
            PaneSide::Local => (
                "local-pane-tabs",
                "local-tab-group",
                "local-files-tab",
                "local-preview-tab",
                "Local files",
                "Local preview",
            ),
            PaneSide::Remote => (
                "remote-pane-tabs",
                "remote-tab-group",
                "remote-files-tab",
                "remote-preview-tab",
                "Remote files",
                "Remote preview",
            ),
        };
        let listener = self.listener.clone();
        // With a trailing picker the tab group scrolls on its own line, like the
        // whole row does without one; it never wraps tab by tab.
        let wraps = self.trailing.is_some();
        let tabs = div()
            .id(group_id)
            .test_support()
            .flex()
            .items_center()
            .min_w_0()
            .when(wraps, |tabs| tabs.max_w_full().overflow_x_scroll())
            .when(!wraps, |tabs| tabs.flex_shrink_0())
            .gap(metric(space::CONTROL, cx))
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(metric(size::FILTER / 2., cx))
                    .truncate()
                    .text_size(Text::Metadata.rems())
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child(self.title),
            )
            .children(
                [
                    (PaneView::Files, files_id, "Dateien", files_name),
                    (PaneView::Preview, preview_id, "Vorschau", preview_name),
                ]
                .into_iter()
                .map(|(view, id, label, name)| {
                    let listener = listener.clone();
                    control(Button::new(id), cx)
                        .label(label)
                        .accessibility_label(name)
                        .tooltip(name)
                        .selected(self.view == view)
                        .disabled(view == PaneView::Preview && !self.preview_enabled)
                        .flex_shrink_0()
                        .on_click(move |_: &ClickEvent, window, cx| listener(view, window, cx))
                }),
            );
        let row = div()
            .id(id)
            .test_support()
            .flex()
            .flex_shrink_0()
            .min_w_0()
            .min_h(metric(size::HEADER, cx))
            .items_center()
            .whitespace_nowrap()
            .gap(metric(space::CONTROL, cx))
            .px(metric(space::PANEL, cx))
            .py(metric(space::TIGHT, cx))
            .text_size(Text::Body.rems())
            .text_color(palette.text)
            .bg(palette.chrome)
            .child(tabs);
        match self.trailing {
            // Wrap instead of scrolling: the picker drops below the tabs rather
            // than leaving the pane. The shell aligns the row height across panes.
            // The basis decides when to wrap; afterwards it may shrink to the pane.
            Some(trailing) => row.flex_wrap().child(
                div()
                    .flex()
                    .flex_1()
                    .flex_basis(metric(size::FILTER / 2., cx))
                    .justify_end()
                    .min_w_0()
                    .child(trailing),
            ),
            None => row
                .h(metric(size::HEADER, cx))
                .max_h(metric(size::HEADER, cx))
                .overflow_x_scroll(),
        }
    }
}
