use super::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions, point, size,
};

struct DiffHarness {
    pane: Entity<DiffPane>,
}
impl Render for DiffHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().size_full().child(self.pane.clone())
    }
}

#[gpui_kit::test]
fn expanding_and_reversing_the_diff_preserve_the_visible_source_anchor(cx: &mut TestAppContext) {
    let mut result = DiffResult::default();
    let local = "before\n".repeat(50) + "L1\nL2\n" + &"after\n".repeat(50);
    let remote = "before\n".repeat(50) + "R\n" + &"after\n".repeat(50);
    result.text(local.as_bytes(), remote.as_bytes());
    let (handle, harness) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(900.), px(500.)),
                })),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let pane = cx.new(|cx| {
                    let mut pane = DiffPane::new(cx);
                    pane.show(
                        Some(Arc::new(result)),
                        DiffState {
                            direction: Decision::Download,
                            ..Default::default()
                        },
                        String::new(),
                        cx,
                    );
                    pane
                });
                cx.new(|_| DiffHarness { pane })
            },
        )
        .unwrap()
    });
    let pane = harness.read_with(cx, |harness, _| harness.pane.clone());
    cx.update_window(handle, |_, w, cx| {
        w.click(("diff-row", 0usize), cx);
        let first = pane.read(cx).state.top_row();
        assert_eq!(pane.read(cx).rows[first].source(), 0);
        pane.update(cx, |pane, cx| pane.direction(Decision::Upload, cx));
        w.render_frame(cx);
        let first = pane.read(cx).state.top_row();
        assert_eq!(pane.read(cx).rows[first].source(), 0);
        assert!(pane.read(cx).state.expanded.contains(&0));
    })
    .unwrap();
}
