use drift_app::FileList;
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    input::{Input, InputEvent, InputState},
};
#[cfg(test)]
use gpui_kit::test::TestSupportExt;
use gpui_kit::{
    App, AppContext, ClipboardItem, Context, Entity, Focusable, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
    div, px, uniform_list,
};

gpui_kit::actions!(drift, [FocusFilter, CopySelection]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-f", FocusFilter, Some("Drift")),
        KeyBinding::new("cmd-f", FocusFilter, Some("Drift")),
        KeyBinding::new("ctrl-shift-c", CopySelection, Some("Drift")),
        KeyBinding::new("cmd-shift-c", CopySelection, Some("Drift")),
    ]);
}

/// Milestone-one input/virtualization harness. No transport or local I/O lives
/// in this view; the entries are explicitly labelled demonstration data.
pub struct Shell {
    filter: Entity<InputState>,
    files: FileList,
    status: String,
    _subscription: Subscription,
}
impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter test files…"));
        let subscription = cx.subscribe_in(&filter, window, |this, state, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.files.filter(&state.read(cx).value());
                this.status = format!("{} test files", this.files.len());
                cx.notify();
            }
        });
        filter.focus_handle(cx).focus(window, cx);
        Self {
            filter,
            files: FileList::new((0..10_000).map(|n| format!("file-{n:05}.txt")).collect()),
            status: "10,000 test files".into(),
            _subscription: subscription,
        }
    }
    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.focus_handle(cx).focus(window, cx);
    }
    fn copy_selection(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(name) = self.files.selected() {
            cx.write_to_clipboard(ClipboardItem::new_string(name.to_owned()));
            self.status = format!("Copied {name}");
            cx.notify();
        }
    }
}
impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let background = theme.background;
        let foreground = theme.foreground;
        let border = theme.border;
        let selected = theme.accent;
        let entity = cx.entity();
        let count = self.files.len();
        div()
            .key_context("Drift")
            .flex()
            .flex_col()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::copy_selection))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .p_3()
                    .border_b_1()
                    .border_color(border)
                    .child(div().font_weight(gpui_kit::FontWeight::BOLD).child("drift"))
                    .child(Input::new(&self.filter).id("filter").w(px(360.)))
                    .child(
                        Button::new("copy-selection")
                            .label("Copy filename")
                            .disabled(self.files.selected().is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.copy_selection(&CopySelection, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .border_r_1()
                            .border_color(border)
                            .child(
                                div()
                                    .p_3()
                                    .border_b_1()
                                    .border_color(border)
                                    .child("Local • test data"),
                            )
                            .child(
                                uniform_list("local-files", count, move |range, _, cx| {
                                    entity.update(cx, |this, cx| {
                                        range
                                            .filter_map(|index| {
                                                let name = this.files.row(index)?.to_owned();
                                                let is_selected =
                                                    this.files.selected() == Some(name.as_str());
                                                let row = div()
                                                    .id(index)
                                                    .h(px(28.))
                                                    .px_3()
                                                    .flex()
                                                    .items_center()
                                                    .bg(if is_selected {
                                                        selected
                                                    } else {
                                                        background
                                                    })
                                                    .child(name)
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            this.files.select(index);
                                                            cx.notify();
                                                        },
                                                    ));
                                                #[cfg(test)]
                                                let row = row.test_support();
                                                Some(row)
                                            })
                                            .collect()
                                    })
                                })
                                .flex_1(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .p_3()
                                    .border_b_1()
                                    .border_color(border)
                                    .child("Remote"),
                            )
                            .child(div().p_4().child("No host connected"))
                            .child(
                                div().p_4().child(
                                    self.files
                                        .selected()
                                        .unwrap_or("Select a test file")
                                        .to_owned(),
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .p_3()
                    .border_t_1()
                    .border_color(border)
                    .child(self.status.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Shell, bind_keys};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
    };

    #[gpui_kit::test]
    fn typing_filters_without_losing_focus_and_copy_uses_clipboard(cx: &mut TestAppContext) {
        let (handle, shell) = cx.update(|cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1100.), px(720.)),
                    })),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Shell::new(window, cx)),
            )
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(shell.read(cx).files.len(), 10_000);
            assert!(
                window.try_find(9999usize).is_none(),
                "offscreen rows must not be mounted"
            );
            window.click("filter", cx);
            window.input("00042", cx);
            assert_eq!(shell.read(cx).filter.read(cx).value(), "00042");
        })
        .unwrap();
        // Entity events are delivered after the borrowed window update ends,
        // matching the UI event loop rather than reading before dispatch.
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(shell.read(cx).files.len(), 1);
            assert_eq!(window.find("filter").focused(), Some(true));
            window.click(0usize, cx);
            assert_eq!(shell.read(cx).files.selected(), Some("file-00042.txt"));
            window.click("copy-selection", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("file-00042.txt")
            );
            window.press("ctrl-f", cx);
            assert_eq!(window.find("filter").focused(), Some(true));
            window.input("x", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(shell.read(cx).files.is_empty());
            assert_eq!(shell.read(cx).files.selected(), None);
        })
        .unwrap();
    }
}
