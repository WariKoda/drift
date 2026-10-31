use super::*;
use crate::design::{metric, size as token};
use gpui_kit::component::{Theme, ThemeMode};

#[gpui_kit::test]
fn diff_chrome_wraps_without_changing_code_rows_selection_or_folds(cx: &mut TestAppContext) {
    let local = (0..40)
        .map(|i| format!("local {i} 👩🏽‍💻\n"))
        .collect::<String>();
    let remote = (0..40)
        .map(|i| format!("remote {i} Ω\n"))
        .collect::<String>();
    let mut result = DiffResult::default();
    result.text(local.as_bytes(), remote.as_bytes());
    let (handle, harness) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(900.), px(700.)),
                })),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let pane = cx.new(|cx| {
                    let mut pane = DiffPane::new(cx);
                    pane.show(
                        Some(Arc::new(result)),
                        DiffState::default(),
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
        w.click_at(("diff-row", 1usize), point(px(2.), px(10.)), cx);
        let selection = pane.read(cx).state.selection;
        assert!(selection.is_some());
        let expanded = pane.read(cx).state.expanded.clone();
        let rows = pane.read(cx).rows.len();
        let revision = pane.read(cx).revision;
        let scroll = pane.read(cx).state.scroll.clone();
        let focus = w.focused(cx);
        for (width, font, mode) in [
            (900., 16., ThemeMode::Dark),
            (320., 16., ThemeMode::Light),
            (420., 24., ThemeMode::Dark),
        ] {
            w.resize(size(px(width), px(700.)));
            w.bounds_changed(cx);
            Theme::change(mode, Some(w), cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            w.render_frame(cx);
            let mut tops = vec![];
            for id in [
                "previous-hunk",
                "next-hunk",
                "collapse-context",
                "copy-diff",
            ] {
                let bounds = w.find(id).bounds();
                assert!(
                    bounds.left() >= px(0.) && bounds.right() <= px(width),
                    "{id}: {bounds:?}"
                );
                assert_eq!(bounds.size.height, metric(token::CONTROL, cx));
                tops.push(bounds.top());
            }
            if width < 900. {
                assert!(tops.iter().any(|top| *top != tops[0]));
            }
            assert_eq!(
                w.find(("diff-row", 1usize)).bounds().size.height,
                px(ROW_HEIGHT)
            );
            assert_eq!(pane.read(cx).state.selection, selection);
            assert_eq!(pane.read(cx).state.expanded, expanded);
            assert_eq!(pane.read(cx).rows.len(), rows);
            assert_eq!(pane.read(cx).revision, revision);
            assert!(Rc::ptr_eq(&pane.read(cx).state.scroll.0, &scroll.0));
            assert_eq!(w.focused(cx), focus);
        }
        w.click("copy-diff", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("local 0 👩🏽‍💻")
        );
    })
    .unwrap();
}
