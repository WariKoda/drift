use super::*;
use std::time::Duration;

#[gpui_kit::test]
fn held_drag_autoscroll_selects_beyond_mounted_rows_and_stops_on_release(cx: &mut TestAppContext) {
    let contents = (0..160)
        .map(|i| format!("line-{i:03}\n"))
        .collect::<String>();
    let f = fixture(cx, &contents, &contents);
    cx.simulate_window_resize(f.handle, gpui_kit::size(px(900.), px(350.)));
    let (pointer, first_max) = cx
        .update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            let start = endpoint(0, 2);
            let from = body_position(&f.pane, start, cx);
            let geometry = f.pane.read(cx).geometry.borrow();
            let viewport = geometry.viewport.unwrap();
            let first_max = geometry.lines.iter().map(|line| line.source).max().unwrap();
            let pointer = point(from.x, viewport.bottom() - px(2.));
            drop(geometry);
            mouse_down(w, cx, from, false);
            mouse_move(w, cx, pointer, true);
            assert!(f.pane.read(cx).auto_scroll.is_active());
            (pointer, first_max)
        })
        .unwrap();
    for _ in 0..32 {
        cx.executor().advance_clock(Duration::from_millis(17));
        cx.run_until_parked();
        cx.update_window(f.handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    cx.update_window(f.handle, |_, w, cx| {
        let selected = f.pane.read(cx).state.selection.unwrap();
        assert_eq!(selected.anchor, endpoint(0, 2));
        assert!(
            selected.head.source > first_max,
            "head {:?}, initial mounted max {first_max}",
            selected.head
        );
        assert!(
            !f.pane
                .read(cx)
                .geometry
                .borrow()
                .lines
                .iter()
                .any(|line| line.source == 0)
        );
        assert!(
            f.pane.read(cx).geometry.borrow().lines.len() < 30,
            "mounted geometry must not accumulate after scrolling"
        );
        mouse_up(w, cx, pointer);
        assert!(f.pane.read(cx).drag.is_none());
        assert!(!f.pane.read(cx).auto_scroll.is_active());
        let expected = selected.copy_text(&f.result, &f.pane.read(cx).rows);
        assert_eq!(copied(w, cx), expected);
        assert!(expected.starts_with("ne-000\nline-001\n"));
    })
    .unwrap();
    let offset = f
        .pane
        .read_with(cx, |p, _| p.state.scroll.0.borrow().base_handle.offset());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(
        f.pane
            .read_with(cx, |p, _| p.state.scroll.0.borrow().base_handle.offset()),
        offset
    );
}

#[gpui_kit::test]
fn gesture_escape_and_deactivation_stop_timer_without_clearing_the_logical_range(
    cx: &mut TestAppContext,
) {
    let contents = (0..120)
        .map(|i| format!("line-{i:03}\n"))
        .collect::<String>();
    let f = fixture(cx, &contents, &contents);
    for hide in [false, true] {
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            let from = body_position(&f.pane, endpoint(0, 2), cx);
            let viewport = f.pane.read(cx).geometry.borrow().viewport.unwrap();
            mouse_down(w, cx, from, false);
            mouse_move(w, cx, point(from.x, viewport.bottom() - px(1.)), true);
            let selection = f.pane.read(cx).state.selection;
            if hide {
                f.pane.update(cx, |pane, _| pane.deactivate());
            } else {
                w.press("escape", cx);
            }
            assert!(f.pane.read(cx).drag.is_none());
            assert!(!f.pane.read(cx).auto_scroll.is_active());
            assert_eq!(f.pane.read(cx).state.selection, selection);
        })
        .unwrap();
        let offset = f
            .pane
            .read_with(cx, |p, _| p.state.scroll.0.borrow().base_handle.offset());
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        assert_eq!(
            f.pane
                .read_with(cx, |p, _| p.state.scroll.0.borrow().base_handle.offset()),
            offset
        );
    }
}

#[gpui_kit::test]
fn whitespace_only_and_empty_interior_lines_copy_without_native_root_trimming(
    cx: &mut TestAppContext,
) {
    let f = fixture(cx, "   \n\n \n", "   \n\n \n");
    cx.update_window(f.handle, |_, w, cx| {
        f.pane.focus_handle(cx).focus(w, cx);
        w.press("ctrl-a", cx);
        assert_eq!(copied(w, cx), "   \n\n ");
        w.click("copy-diff", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "   \n\n "
        );
    })
    .unwrap();
}
