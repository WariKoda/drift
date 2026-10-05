//! Headless window-event coverage, not native OS or root-modal acceptance tests.
use super::*;
mod scroll;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Entity, InputEvent as _, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, TestAppContext, WindowBounds, WindowOptions,
    point, size,
};

struct SelectionWindow(Entity<DiffPane>);
impl Render for SelectionWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Drift")
            .size_full()
            .flex()
            .flex_col()
            .child(self.0.clone())
    }
}

struct Fixture {
    handle: AnyWindowHandle,
    pane: Entity<DiffPane>,
    result: Arc<DiffResult>,
}

fn fixture(cx: &mut TestAppContext, local: &str, remote: &str) -> Fixture {
    let mut result = DiffResult::default();
    result.text(local.as_bytes(), remote.as_bytes());
    let result = Arc::new(result);
    let (handle, owner) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        // Kit installs the real Base Root and its component/theme integration.
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(960.), px(720.)),
                ))),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let pane = cx.new(|cx| {
                    let mut pane = DiffPane::new(cx);
                    pane.show(
                        Some(result.clone()),
                        DiffState {
                            direction: Decision::Download,
                            ..Default::default()
                        },
                        String::new(),
                        cx,
                    );
                    pane
                });
                cx.new(|_| SelectionWindow(pane))
            },
        )
        .unwrap()
    });
    // Test platforms do not necessarily adopt WindowOptions' size as the viewport.
    cx.simulate_window_resize(handle, size(px(960.), px(720.)));
    let pane = owner.read_with(cx, |owner, _| owner.0.clone());
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    Fixture {
        handle,
        pane,
        result,
    }
}

fn source(result: &DiffResult, text: &str) -> usize {
    let matches: Vec<_> = result
        .lines
        .iter()
        .enumerate()
        .filter_map(|(source, line)| (line.text == text).then_some(source))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "fixture must identify a unique line: {text:?}"
    );
    matches[0]
}

fn endpoint(source: usize, byte: usize) -> TextPoint {
    TextPoint { source, byte }
}

fn assert_selection(pane: &Entity<DiffPane>, anchor: TextPoint, head: TextPoint, cx: &App) {
    assert_eq!(
        pane.read(cx).state.selection,
        Some(Selection { anchor, head })
    );
}

fn body_position(pane: &Entity<DiffPane>, point: TextPoint, cx: &App) -> Point<Pixels> {
    let geometry = pane.read(cx).geometry.borrow();
    let line = geometry
        .lines
        .iter()
        .find(|line| line.source == point.source)
        .expect("source must have been laid out by a real frame");
    assert!(line.text.is_char_boundary(point.byte));
    let mut position = line
        .layout
        .position_for_index(point.byte)
        .expect("painted StyledText must expose its byte position");
    position.x += px(0.1);
    position.y = line.bounds.top() + line.layout.line_height() / 2.;
    assert!(geometry.viewport.unwrap().contains(&position));
    position
}

fn mouse_down(w: &mut Window, cx: &mut App, position: Point<Pixels>, shift: bool) {
    w.dispatch_event(
        MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: gpui_kit::Modifiers {
                shift,
                ..Default::default()
            },
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
}

fn mouse_move(w: &mut Window, cx: &mut App, position: Point<Pixels>, pressed: bool) {
    w.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: pressed.then_some(MouseButton::Left),
            modifiers: Default::default(),
        }
        .to_platform_input(),
        cx,
    );
}

fn mouse_up(w: &mut Window, cx: &mut App, position: Point<Pixels>) {
    w.dispatch_event(
        MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Default::default(),
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
}

fn click(w: &mut Window, position: Point<Pixels>, shift: bool, cx: &mut App) {
    mouse_down(w, cx, position, shift);
    mouse_up(w, cx, position);
    w.render_frame(cx);
}

fn drag(w: &mut Window, cx: &mut App, from: Point<Pixels>, to: Point<Pixels>) {
    mouse_down(w, cx, from, false);
    w.render_frame(cx);
    mouse_move(w, cx, to, true);
    w.render_frame(cx);
    mouse_up(w, cx, to);
    w.render_frame(cx);
}

fn copied(w: &mut Window, cx: &mut App) -> String {
    w.press(
        if cfg!(target_os = "macos") {
            "cmd-c"
        } else {
            "ctrl-c"
        },
        cx,
    );
    cx.read_from_clipboard().unwrap().text().unwrap()
}

#[gpui_kit::test]
fn body_clicks_place_carets_and_snap_native_hits_to_extended_graphemes(cx: &mut TestAppContext) {
    let text = "A日本🦀e\u{301}👩‍💻 Z";
    let f = fixture(cx, &(text.to_owned() + "\n"), &(text.to_owned() + "\n"));
    let source = source(&f.result, text);
    let boundaries = [
        "",
        "A",
        "A日",
        "A日本",
        "A日本🦀",
        "A日本🦀e\u{301}",
        "A日本🦀e\u{301}👩‍💻",
        "A日本🦀e\u{301}👩‍💻 ",
        text,
    ]
    .map(str::len);
    cx.update_window(f.handle, |_, w, cx| {
        for byte in boundaries {
            let caret = endpoint(source, byte);
            click(w, body_position(&f.pane, caret, cx), false, cx);
            assert_selection(&f.pane, caret, caret, cx);
            assert!(f.pane.read(cx).focus.is_focused(w));
            assert!(f.pane.read(cx).drag.is_none());
        }
        // Probe character positions inside the combining and ZWJ clusters too.
        // The shaper may map several character indices to the same glyph.
        for byte in [
            "A日本🦀e".len(),
            "A日本🦀e\u{301}👩".len(),
            "A日本🦀e\u{301}👩‍".len(),
        ] {
            let position = body_position(&f.pane, endpoint(source, byte), cx);
            let native_byte = {
                let geometry = f.pane.read(cx).geometry.borrow();
                let line = geometry
                    .lines
                    .iter()
                    .find(|line| line.source == source)
                    .unwrap();
                line.layout
                    .index_for_position(position)
                    .unwrap_or_else(|byte| byte)
            };
            let snapped = *boundaries
                .iter()
                .filter(|byte| **byte <= native_byte)
                .max()
                .unwrap();
            click(w, position, false, cx);
            assert_selection(
                &f.pane,
                endpoint(source, snapped),
                endpoint(source, snapped),
                cx,
            );
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn gutter_click_keeps_the_whole_line_after_release(cx: &mut TestAppContext) {
    let text = "  日本🦀  ";
    let f = fixture(cx, &(text.to_owned() + "\n"), "remote\n");
    let source = source(&f.result, text);
    cx.update_window(f.handle, |_, w, cx| {
        let body = body_position(&f.pane, endpoint(source, 0), cx);
        let gutter = {
            let geometry = f.pane.read(cx).geometry.borrow();
            let line = geometry
                .lines
                .iter()
                .find(|line| line.source == source)
                .unwrap();
            let left = geometry.viewport.unwrap().left();
            assert!(line.bounds.left() > left);
            point((left + line.bounds.left()) / 2., body.y)
        };
        mouse_down(w, cx, gutter, false);
        assert_selection(
            &f.pane,
            endpoint(source, 0),
            endpoint(source, text.len()),
            cx,
        );
        mouse_up(w, cx, gutter);
        w.render_frame(cx);
        assert_selection(
            &f.pane,
            endpoint(source, 0),
            endpoint(source, text.len()),
            cx,
        );
        assert!(f.pane.read(cx).drag.is_none());
        assert_eq!(copied(w, cx), text);
    })
    .unwrap();
}

#[gpui_kit::test]
fn shift_click_retains_the_original_anchor_across_lines_and_reversal(cx: &mut TestAppContext) {
    let f = fixture(
        cx,
        "first 日本\nsecond 🦀\nthird\n",
        "first 日本\nsecond 🦀\nthird\n",
    );
    let first = source(&f.result, "first 日本");
    let second = source(&f.result, "second 🦀");
    let anchor = endpoint(first, "first ".len());
    cx.update_window(f.handle, |_, w, cx| {
        click(w, body_position(&f.pane, anchor, cx), false, cx);
        let head = endpoint(second, "second ".len());
        click(w, body_position(&f.pane, head, cx), true, cx);
        assert_selection(&f.pane, anchor, head, cx);
        assert_eq!(copied(w, cx), "日本\nsecond ");
        let reversed = endpoint(first, 0);
        click(w, body_position(&f.pane, reversed, cx), true, cx);
        assert_selection(&f.pane, anchor, reversed, cx);
        assert_eq!(copied(w, cx), "first ");
    })
    .unwrap();
}

#[gpui_kit::test]
fn forward_and_reverse_drags_copy_exact_unicode_spaces_and_blank_lines(cx: &mut TestAppContext) {
    let first = "  日本🦀 e\u{301} 👩‍💻  ";
    let last = "  tail  ";
    let content = format!("{first}\n\n{last}\n");
    let f = fixture(cx, &content, &content);
    let start = endpoint(source(&f.result, first), 2);
    let end = endpoint(source(&f.result, last), last.len() - 2);
    cx.update_window(f.handle, |_, w, cx| {
        for (anchor, head) in [(start, end), (end, start)] {
            let from = body_position(&f.pane, anchor, cx);
            let to = body_position(&f.pane, head, cx);
            drag(w, cx, from, to);
            assert_selection(&f.pane, anchor, head, cx);
            assert!(f.pane.read(cx).drag.is_none());
            assert_eq!(f.pane.read(cx).auto_scroll.delta(), None);
            assert_eq!(copied(w, cx), "日本🦀 e\u{301} 👩‍💻  \n\n  tail");
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn releasing_outside_the_viewport_ends_the_gesture(cx: &mut TestAppContext) {
    let f = fixture(cx, "alpha\nbeta\ngamma\n", "alpha\nbeta\ngamma\n");
    let anchor = endpoint(source(&f.result, "alpha"), 2);
    let end = endpoint(source(&f.result, "gamma"), "gamma".len());
    cx.update_window(f.handle, |_, w, cx| {
        let from = body_position(&f.pane, anchor, cx);
        let to = body_position(&f.pane, end, cx);
        mouse_down(w, cx, from, false);
        w.render_frame(cx);
        mouse_move(w, cx, to, true);
        let viewport = f.pane.read(cx).geometry.borrow().viewport.unwrap();
        let outside = point(to.x, viewport.bottom() + px(20.));
        assert!(!viewport.contains(&outside));
        mouse_up(w, cx, outside);
        w.render_frame(cx);
        assert_selection(&f.pane, anchor, end, cx);
        assert!(f.pane.read(cx).drag.is_none());
        assert_eq!(f.pane.read(cx).auto_scroll.delta(), None);
        mouse_move(w, cx, from, true);
        mouse_move(w, cx, from, false);
        w.render_frame(cx);
        assert_selection(&f.pane, anchor, end, cx);
        assert_eq!(copied(w, cx), "pha\nbeta\ngamma");
    })
    .unwrap();
}

#[gpui_kit::test]
fn select_all_copies_visible_bodies_without_headers_gutters_or_hidden_context(
    cx: &mut TestAppContext,
) {
    let before = (0..18)
        .map(|i| format!("before-{i:02}\n"))
        .collect::<String>();
    let after = (0..18)
        .map(|i| format!("after-{i:02}\n"))
        .collect::<String>();
    let f = fixture(
        cx,
        &format!("{before}old 日本\n{after}"),
        &format!("{before}new 🦀\n{after}"),
    );
    let rows = hunks::flatten(&f.result, &BTreeSet::new(), Decision::Download);
    assert!(rows.iter().any(|row| matches!(row, Row::Fold { .. })));
    assert!(rows.iter().any(|row| matches!(row, Row::Header { .. })));
    let sources: Vec<_> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Line(source) => Some(*source),
            _ => None,
        })
        .collect();
    let expected =
        "before-15\nbefore-16\nbefore-17\nold 日本\nnew 🦀\nafter-00\nafter-01\nafter-02";
    cx.update_window(f.handle, |_, w, cx| {
        for (all, copy) in [("ctrl-a", "ctrl-c"), ("cmd-a", "cmd-c")] {
            let caret = endpoint(source(&f.result, "old 日本"), 0);
            click(w, body_position(&f.pane, caret, cx), false, cx);
            w.press(all, cx);
            assert_selection(
                &f.pane,
                endpoint(sources[0], 0),
                endpoint(
                    *sources.last().unwrap(),
                    f.result.lines[*sources.last().unwrap()].text.len(),
                ),
                cx,
            );
            w.press(copy, cx);
            assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), expected);
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn shift_arrows_home_and_end_use_graphemes_and_keep_the_anchor(cx: &mut TestAppContext) {
    let first = "A日本🦀e\u{301}👩‍💻 Z";
    let second = "ae\u{301}👩‍💻🦀z";
    let content = format!("{first}\n{second}\n");
    let f = fixture(cx, &content, &content);
    let first_source = source(&f.result, first);
    let second_source = source(&f.result, second);
    let anchor = endpoint(first_source, 1);
    cx.update_window(f.handle, |_, w, cx| {
        click(w, body_position(&f.pane, anchor, cx), false, cx);
        for prefix in [
            "A日",
            "A日本",
            "A日本🦀",
            "A日本🦀e\u{301}",
            "A日本🦀e\u{301}👩‍💻",
        ] {
            w.press("shift-right", cx);
            assert_selection(&f.pane, anchor, endpoint(first_source, prefix.len()), cx);
        }
        assert_eq!(copied(w, cx), "日本🦀e\u{301}👩‍💻");
        w.press("shift-left", cx);
        assert_selection(
            &f.pane,
            anchor,
            endpoint(first_source, "A日本🦀e\u{301}".len()),
            cx,
        );
        w.press("shift-home", cx);
        assert_selection(&f.pane, anchor, endpoint(first_source, 0), cx);
        w.press("shift-end", cx);
        assert_selection(&f.pane, anchor, endpoint(first_source, first.len()), cx);
        assert_eq!(copied(w, cx), &first[1..]);

        let column_three = endpoint(first_source, "A日本".len());
        click(w, body_position(&f.pane, column_three, cx), false, cx);
        w.press("shift-down", cx);
        assert_selection(
            &f.pane,
            column_three,
            endpoint(second_source, "ae\u{301}👩‍💻".len()),
            cx,
        );
        assert_eq!(copied(w, cx), "🦀e\u{301}👩‍💻 Z\nae\u{301}👩‍💻");
        w.press("shift-up", cx);
        assert_selection(&f.pane, column_three, column_three, cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn direction_changes_preserve_logical_endpoints_and_follow_real_hunk_order(
    cx: &mut TestAppContext,
) {
    let old = "old 日本🦀  ";
    let new = "new e\u{301}👩‍💻  ";
    let f = fixture(cx, &format!("{old}\n"), &format!("{new}\n"));
    let old_source = source(&f.result, old);
    let new_source = source(&f.result, new);
    let anchor = endpoint(old_source, "old ".len());
    let head = endpoint(new_source, new.len() - 2);
    cx.update_window(f.handle, |_, w, cx| {
        let from = body_position(&f.pane, anchor, cx);
        let to = body_position(&f.pane, head, cx);
        drag(w, cx, from, to);
        assert_eq!(copied(w, cx), "日本🦀  \nnew e\u{301}👩‍💻");
        for direction in [Decision::Upload, Decision::Download] {
            f.pane.update(cx, |pane, cx| pane.direction(direction, cx));
            w.render_frame(cx);
            assert_selection(&f.pane, anchor, head, cx);
            let rows = hunks::flatten(&f.result, &BTreeSet::new(), direction);
            assert_eq!(f.pane.read(cx).rows, rows);
            let old_rank = rows
                .iter()
                .position(|row| *row == Row::Line(old_source))
                .unwrap();
            let new_rank = rows
                .iter()
                .position(|row| *row == Row::Line(new_source))
                .unwrap();
            let expected = if old_rank < new_rank {
                format!("{}\n{}", &old[anchor.byte..], &new[..head.byte])
            } else {
                format!("{}\n{}", &new[head.byte..], &old[..anchor.byte])
            };
            assert_eq!(copied(w, cx), expected);
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn folding_retains_hidden_endpoints_but_copy_omits_hidden_content(cx: &mut TestAppContext) {
    let before = (0..18)
        .map(|i| format!("before-{i:02}\n"))
        .collect::<String>();
    let f = fixture(
        cx,
        &format!("{before}old 日本\n"),
        &format!("{before}new 🦀\n"),
    );
    let anchor = endpoint(source(&f.result, "before-03"), "before-".len());
    let head = endpoint(source(&f.result, "old 日本"), "old".len());
    cx.update_window(f.handle, |_, w, cx| {
        f.pane.focus_handle(cx).focus(w, cx);
        w.press("c", cx);
        let from = body_position(&f.pane, anchor, cx);
        let to = body_position(&f.pane, head, cx);
        drag(w, cx, from, to);
        let expanded = "03\n".to_owned()
            + &(4..18)
                .map(|i| format!("before-{i:02}\n"))
                .collect::<String>()
            + "old";
        assert_eq!(copied(w, cx), expanded);
        w.press("c", cx);
        assert_selection(&f.pane, anchor, head, cx);
        assert!(f.pane.read(cx).rows.iter().any(|row| matches!(
            row, Row::Fold { start, end, .. } if (*start..*end).contains(&anchor.source)
        )));
        assert_eq!(copied(w, cx), "before-15\nbefore-16\nbefore-17\nold");
        w.press("c", cx);
        assert_selection(&f.pane, anchor, head, cx);
        assert_eq!(copied(w, cx), expanded);
    })
    .unwrap();
}

#[gpui_kit::test]
fn resize_and_per_file_show_restore_endpoints_scroll_and_reject_old_paint_events(
    cx: &mut TestAppContext,
) {
    let content = (0..100)
        .map(|i| format!("line-{i:03} 日本🦀\n"))
        .collect::<String>();
    let f = fixture(cx, &content, &content);
    let first = source(&f.result, "line-050 日本🦀");
    let anchor = endpoint(first, 5);
    let head = endpoint(source(&f.result, "line-052 日本🦀"), 8);
    let expected = "050 日本🦀\nline-051 日本🦀\nline-052";
    let saved = cx
        .update_window(f.handle, |_, w, cx| {
            f.pane.update(cx, |pane, cx| {
                pane.state.scroll.scroll_to_item_strict(
                    hunks::source_index(&pane.rows, first),
                    ScrollStrategy::Top,
                );
                cx.notify();
            });
            w.render_frame(cx);
            let from = body_position(&f.pane, anchor, cx);
            let to = body_position(&f.pane, head, cx);
            drag(w, cx, from, to);
            assert_selection(&f.pane, anchor, head, cx);
            assert_eq!(
                f.pane.read(cx).rows[f.pane.read(cx).state.top_row()].source(),
                first
            );
            assert_eq!(copied(w, cx), expected);
            f.pane.read(cx).state.clone()
        })
        .unwrap();

    for (width, height) in [(640., 420.), (1100., 850.)] {
        cx.simulate_window_resize(f.handle, size(px(width), px(height)));
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            let viewport = f.pane.read(cx).geometry.borrow().viewport.unwrap();
            assert!(viewport.size.width <= px(width));
            assert!(viewport.size.width > px(width - 50.));
            assert!(viewport.size.height < px(height));
            assert!(viewport.size.height > px(height - 200.));
            assert_selection(&f.pane, anchor, head, cx);
            assert_eq!(
                f.pane.read(cx).rows[f.pane.read(cx).state.top_row()].source(),
                first
            );
            assert_eq!(copied(w, cx), expected);
        })
        .unwrap();
    }

    cx.update_window(f.handle, |_, w, cx| {
        let old_position = body_position(&f.pane, anchor, cx);
        let old_geometry = f.pane.read(cx).geometry.clone();
        mouse_down(w, cx, old_position, false);
        assert!(f.pane.read(cx).drag.is_some());
        let mut other = DiffResult::default();
        other.text(b"another file\n", b"another file\n");
        let other = Arc::new(other);
        f.pane.update(cx, |pane, cx| {
            pane.show(Some(other.clone()), DiffState::default(), String::new(), cx);
        });
        // Do not paint here: these events encounter callbacks from the old frame.
        mouse_down(w, cx, old_position, false);
        mouse_move(w, cx, old_position, true);
        mouse_up(w, cx, old_position);
        assert_eq!(f.pane.read(cx).state.selection, None);
        assert!(f.pane.read(cx).drag.is_none());
        assert_eq!(f.pane.read(cx).auto_scroll.delta(), None);
        assert!(f.pane.read(cx).geometry.borrow().lines.is_empty());
        assert!(!Rc::ptr_eq(&old_geometry, &f.pane.read(cx).geometry));

        w.render_frame(cx);
        let other_caret = endpoint(source(&other, "another file"), 4);
        click(w, body_position(&f.pane, other_caret, cx), false, cx);
        assert_selection(&f.pane, other_caret, other_caret, cx);
        let other_state = f.pane.read(cx).state.clone();
        f.pane.update(cx, |pane, cx| {
            pane.show(Some(f.result.clone()), saved.clone(), String::new(), cx);
        });
        w.render_frame(cx);
        assert_selection(&f.pane, anchor, head, cx);
        assert_eq!(
            f.pane.read(cx).rows[f.pane.read(cx).state.top_row()].source(),
            first
        );
        assert_eq!(copied(w, cx), expected);
        f.pane.update(cx, |pane, cx| {
            pane.show(Some(other.clone()), other_state.clone(), String::new(), cx);
        });
        w.render_frame(cx);
        assert_selection(&f.pane, other_caret, other_caret, cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn absent_or_collapsed_selection_copies_the_formatted_whole_diff(cx: &mut TestAppContext) {
    let f = fixture(cx, "old 日本\n", "new 🦀\n");
    let rows = hunks::flatten(&f.result, &BTreeSet::new(), Decision::Download);
    let expected = "@@ -1,1 +1,1 @@\n    1       - old 日本\n          1 + new 🦀";
    assert!(matches!(rows[0], Row::Header { .. }));
    cx.update_window(f.handle, |_, w, cx| {
        f.pane.focus_handle(cx).focus(w, cx);
        assert_eq!(f.pane.read(cx).state.selection, None);
        assert_eq!(copied(w, cx), expected);
        let caret = endpoint(source(&f.result, "old 日本"), "old ".len());
        click(w, body_position(&f.pane, caret, cx), false, cx);
        assert_selection(&f.pane, caret, caret, cx);
        assert_eq!(copied(w, cx), expected);
    })
    .unwrap();
}
