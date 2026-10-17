use super::*;
use crate::design::{Palette, Text, metric, size as sizes, space};
use gpui_kit::base::{StyledExt as _, TestSupportExt as _};
use gpui_kit::component::{Sizable, button::ButtonVariants};
use gpui_kit::{AnyElement, AvailableSpace, canvas, point, size};

impl ProjectsPanel {
    pub(in crate::projects) fn render_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let form = self.form.as_ref().unwrap();
        let title = if form.expected.is_some() {
            "Edit project"
        } else {
            "Create project"
        };
        let busy = self.is_loading();
        let name = form_input::guard(&form.name, |input| {
            self.details_reveal.wrap(
                ("reveal-project-name", form.name.entity_id()),
                // Input::h configures multi-line height only; style the native frame.
                Styled::h(
                    input.id("project-edit-name").small().disabled(busy),
                    metric(sizes::CONTROL, cx),
                ),
            )
        })
        .into_any_element();
        let path = form_input::guard(&form.path, |input| {
            self.details_reveal.wrap(
                ("reveal-project-path", form.path.entity_id()),
                Styled::h(
                    input.id("project-edit-path").small().disabled(busy),
                    metric(sizes::CONTROL, cx),
                ),
            )
        })
        .into_any_element();
        // Visual and native traversal order agree: fields, Cancel, Save project.
        let cancel = Button::new("project-close")
            .label("Cancel")
            .small()
            .secondary()
            .tab_index(0)
            .h(metric(sizes::CONTROL, cx))
            .disabled(self.writing || self.busy)
            .on_click(cx.listener(|this, _, w, cx| this.cancel_view(&Cancel, w, cx)))
            .into_any_element();
        let save = Button::new("project-save")
            .label("Save project")
            .tooltip("Save project (Ctrl/Cmd+S)")
            .small()
            .primary()
            .tab_index(0)
            .h(metric(sizes::CONTROL, cx))
            .disabled(busy)
            .on_click(cx.listener(|this, _, w, cx| this.save_form(w, cx)))
            .into_any_element();
        let scroll = self.details_reveal.scroll_handle().clone();
        let status = self.status.clone();
        canvas(
            move |bounds, window, cx| {
                // This is the parent's actual content allocation, including all
                // ancestor insets and decorations, not the window viewport.
                let compact = bounds.size.height < metric(240., cx);
                let outer = if compact {
                    px(0.)
                } else if bounds.size.width < metric(400., cx) {
                    metric(space::CONTROL, cx)
                } else {
                    metric(space::INDENT, cx)
                };
                let width = (bounds.size.width - outer * 2.)
                    .max(px(0.))
                    .min(metric(sizes::PROJECT_FORM, cx));
                let height = (bounds.size.height - outer * 2.).max(px(0.));
                let palette = Palette::current(cx);
                let inset = metric(
                    if compact {
                        space::CONTROL
                    } else {
                        space::INDENT
                    },
                    cx,
                );
                let tight = metric(space::TIGHT, cx);
                let compact_y = if bounds.size.height < metric(120., cx) {
                    px(0.)
                } else {
                    tight
                };
                let mut content = div()
                    .id("project-dialog")
                    .test_support()
                    .w(width)
                    .max_h(height)
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(metric(sizes::RADIUS, cx))
                    .bg(palette.canvas)
                    .text_color(palette.text)
                    .text_size(Text::Body.rems())
                    .child(
                        div()
                            .id("project-form-header")
                            .test_support()
                            .flex_shrink_0()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .px(inset)
                            .pt(if compact {
                                compact_y
                            } else {
                                metric(space::INDENT, cx)
                            })
                            .pb(if compact {
                                compact_y
                            } else {
                                metric(space::CONTROL, cx)
                            })
                            .gap(tight)
                            .bg(palette.chrome)
                            .border_b_1()
                            .border_color(palette.border)
                            .child(
                                div()
                                    .text_size(Text::Title.rems())
                                    .font_semibold()
                                    .child(title),
                            )
                            .when(!compact, |header| {
                                header.child("Choose a name and local folder for this project.")
                            }),
                    )
                    .child(
                        div()
                            .id("project-details")
                            .test_support()
                            .min_h_0()
                            .min_w_0()
                            .flex_shrink_1()
                            .overflow_y_scroll()
                            .track_scroll(&scroll)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .px(inset)
                                    // Drop body padding before sacrificing the field viewport.
                                    // Oversized inputs retain FocusReveal's top-edge fallback.
                                    .py(if compact {
                                        px(0.)
                                    } else {
                                        metric(space::INDENT, cx)
                                    })
                                    .gap(metric(
                                        if compact {
                                            space::CONTROL
                                        } else {
                                            space::INDENT
                                        },
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .min_w_0()
                                            .gap(tight)
                                            .child(
                                                div()
                                                    .text_size(Text::Metadata.rems())
                                                    .text_color(palette.muted)
                                                    .child("Project name"),
                                            )
                                            .child(name),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .min_w_0()
                                            .gap(tight)
                                            .child(
                                                div()
                                                    .text_size(Text::Metadata.rems())
                                                    .text_color(palette.muted)
                                                    .child("Local folder"),
                                            )
                                            .child(path),
                                    )
                                    .when(!status.is_empty(), |body| {
                                        body.child(
                                            div()
                                                .text_size(Text::Metadata.rems())
                                                .text_color(palette.muted)
                                                .child(status),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("project-form-footer")
                            .test_support()
                            .flex_shrink_0()
                            .min_w_0()
                            .flex()
                            .flex_wrap()
                            .justify_end()
                            .px(if compact { tight } else { inset })
                            .py(if compact {
                                compact_y
                            } else {
                                metric(space::PANEL, cx)
                            })
                            .gap(metric(space::CONTROL, cx))
                            .bg(palette.chrome)
                            .border_t_1()
                            .border_color(palette.border)
                            .child(cancel)
                            .child(save),
                    )
                    .into_any_element();
                let measured = content.layout_as_root(
                    size(
                        AvailableSpace::Definite(width),
                        AvailableSpace::Definite(height),
                    ),
                    window,
                    cx,
                );
                content.prepaint_at(
                    bounds.origin
                        + point(
                            (bounds.size.width - measured.width) / 2.,
                            (bounds.size.height - measured.height) / 2.,
                        ),
                    window,
                    cx,
                );
                content
            },
            |_, mut content, window, cx| content.paint(window, cx),
        )
        .size_full()
        .into_any_element()
    }
}
