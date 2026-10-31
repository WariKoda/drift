use crate::design::{Palette, metric, size as dimensions};
use crate::pane_tabs::PaneSide;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AvailableSpace, InteractiveElement, IntoElement, ParentElement, Pixels,
    Styled, TestSupportExt, canvas, div, px, size,
};
use std::{cell::RefCell, rc::Rc};

pub(super) struct AlignedHeaders {
    heights: Rc<RefCell<[Pixels; 2]>>,
    name: &'static str,
    minimum_rows: f32,
    bordered: bool,
}

impl AlignedHeaders {
    pub fn new(name: &'static str, minimum_rows: f32) -> Self {
        Self {
            heights: Rc::new(RefCell::new([px(0.); 2])),
            name,
            minimum_rows,
            bordered: false,
        }
    }

    /// The slot draws the row's bottom border, so the shorter side's line sits
    /// at the shared bottom instead of in the middle of the padded slot.
    pub fn bordered(name: &'static str, minimum_rows: f32) -> Self {
        Self {
            bordered: true,
            ..Self::new(name, minimum_rows)
        }
    }

    pub fn view(&self, side: PaneSide, content: AnyElement, cx: &App) -> AnyElement {
        let mut content = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .child(content)
            .into_any_element();
        let height = self.heights.borrow().iter().copied().fold(
            metric(dimensions::HEADER * self.minimum_rows, cx),
            Pixels::max,
        );
        let heights = self.heights.clone();
        let (index, side_name) = match side {
            PaneSide::Local => (0, "local"),
            PaneSide::Remote => (1, "remote"),
        };
        div()
            .id(format!("{side_name}-aligned-{}", self.name))
            .test_support()
            .w_full()
            .h(if self.bordered {
                height + px(1.)
            } else {
                height
            })
            .when(self.bordered, |slot| {
                slot.border_b_1().border_color(Palette::current(cx).border)
            })
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .bg(Palette::current(cx).chrome)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let measured = content.layout_as_root(
                            size(
                                AvailableSpace::Definite(bounds.size.width),
                                AvailableSpace::MaxContent,
                            ),
                            window,
                            cx,
                        );
                        let measured = px(measured.height.as_f32().ceil());
                        {
                            let mut heights = heights.borrow_mut();
                            if (heights[index] - measured).abs() > px(0.5) {
                                heights[index] = measured;
                                // Refresh is ignored during prepaint; invalidate after drawing.
                                window.defer(cx, |window, _| window.refresh());
                            }
                        }
                        content.prepaint_at(bounds.origin, window, cx);
                        content
                    },
                    |_, mut content, window, cx| content.paint(window, cx),
                )
                .size_full(),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
