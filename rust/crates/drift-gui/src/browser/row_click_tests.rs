use super::*;

#[path = "selection_tests.rs"]
mod selection_tests;

fn pointer_event(
    window: &mut Window,
    position: Point<Pixels>,
    click_count: usize,
    modifiers: gpui_kit::Modifiers,
    down: bool,
    cx: &mut App,
) {
    if down {
        window.dispatch_event(
            gpui_kit::MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers,
                click_count,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
    } else {
        window.dispatch_event(
            gpui_kit::MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers,
                click_count,
            }
            .to_platform_input(),
            cx,
        );
    }
}

fn pointer_click(
    window: &mut Window,
    position: Point<Pixels>,
    click_count: usize,
    modifiers: gpui_kit::Modifiers,
    cx: &mut App,
) {
    pointer_event(window, position, click_count, modifiers, true, cx);
    window.render_frame(cx);
    pointer_event(window, position, click_count, modifiers, false, cx);
    window.render_frame(cx);
}

#[gpui_kit::test]
async fn directory_row_single_selects_double_toggles_without_entering_real_local_tree(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        f.right.update(cx, |p, cx| p.focus_filter_input(w, cx));
        w.within("left").within("local-files").click(0usize, cx);
        let p = f.left.read(cx);
        assert!(p.browser_focus.is_focused(w));
        assert_eq!(p.selected(), Some("src"));
        assert!(!p.is_loading());
        assert!(!p.tree.node("src").unwrap().expanded);
        assert_eq!(p.location().unwrap().directory, f.left_root.path());
        w.within("left")
            .within("local-files")
            .double_click(0usize, cx);
        assert_eq!(f.left.read(cx).tree_pending.as_deref(), Some("src"));
    })
    .unwrap();
    f.wait_idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert!(f.left.read(cx).tree.node("src").unwrap().expanded);
        assert!(f.left.read(cx).tree.node("src/a.txt").is_some());
        w.within("left").within("local-files").click(0usize, cx);
        assert!(f.left.read(cx).tree.node("src").unwrap().expanded);
        w.within("left")
            .within("local-files")
            .double_click(0usize, cx);
        assert!(!f.left.read(cx).tree.node("src").unwrap().expanded);
        assert!(f.left.read(cx).tree.node("src/a.txt").is_none());
        assert_eq!(
            f.left.read(cx).location().unwrap().directory,
            f.left_root.path()
        );
        assert!(!f.left.read(cx).can_back());
        assert!(!f.left.read(cx).can_forward());
        // A second native double arrives before the real expansion completes.
        w.within("left")
            .within("local-files")
            .double_click(0usize, cx);
        let expansion = f.left.read(cx).listing_cancel.as_ref().unwrap().clone();
        w.within("left").within("local-files").click(0usize, cx);
        assert_eq!(f.left.read(cx).tree_pending.as_deref(), Some("src"));
        assert!(
            !expansion.is_cancelled(),
            "single click cancelled an expansion"
        );
        w.within("left")
            .within("local-files")
            .double_click(0usize, cx);
        assert!(expansion.is_cancelled());
    })
    .unwrap();
    f.wait_idle(cx).await;
    cx.executor().run_until_parked();
    cx.update_window(f.handle, |_, w, cx| {
        let p = f.left.read(cx);
        assert!(!p.tree.node("src").unwrap().expanded);
        assert!(p.tree.node("src/a.txt").is_none());
        assert_eq!(p.selected(), Some("src"));
        assert_eq!(p.location().unwrap().directory, f.left_root.path());
        assert!(!p.can_back());
        assert_eq!(
            f.right.read(cx).location().unwrap().directory,
            f._right_root.path()
        );
        // Entering is explicit and preserves the existing keyboard contract.
        w.press("alt-enter", cx);
    })
    .unwrap();
    f.wait_idle(cx).await;
    assert_eq!(
        f.left
            .read_with(cx, |p, _| p.location().unwrap().directory.clone()),
        f.left_root.path().join("src")
    );
    assert!(f.left.read_with(cx, |p, _| p.can_back()));
    assert!(
        f.events
            .borrow()
            .iter()
            .all(|e| matches!(e, ObservedEvent::Selected(_, _, false)))
    );
}

#[gpui_kit::test]
async fn file_single_and_double_select_without_preview_but_explicit_enter_previews(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.within("left").within("local-files").click(1usize, cx);
        assert_eq!(f.left.read(cx).selected(), Some("alpha.txt"));
        w.within("left")
            .within("local-files")
            .double_click(2usize, cx);
        assert_eq!(f.left.read(cx).selected(), Some("bravo.txt"));
        assert!(!f.left.read(cx).is_loading());
        assert!(!f.left.read(cx).can_back());
    })
    .unwrap();
    assert!(
        f.events
            .borrow()
            .iter()
            .all(|e| matches!(e, ObservedEvent::Selected("left", Some(_), false)))
    );
    f.events.borrow_mut().clear();
    cx.update_window(f.handle, |_, w, cx| {
        w.press("enter", cx);
    })
    .unwrap();
    assert_eq!(f.events.borrow().iter().filter(|e| matches!(e, ObservedEvent::Selected("left", Some(path), true) if path == std::path::Path::new("bravo.txt"))).count(), 1);
    assert_eq!(
        fs::read_to_string(f.left_root.path().join("alpha.txt")).unwrap(),
        "alpha"
    );
    assert_eq!(
        fs::read_to_string(f.left_root.path().join("bravo.txt")).unwrap(),
        "bravo"
    );
}

#[gpui_kit::test]
async fn full_width_local_rows_include_trailing_blank_and_indentation_with_full_background(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        let list = w.within("left").find("local-files-pane").bounds();
        let row = w.within("left").within("local-files").find(1usize).bounds();
        assert_eq!(row.origin.x, list.origin.x);
        assert_eq!(row.size.width, list.size.width);
        let position = point(list.right() - px(4.), row.center().y);
        pointer_click(w, position, 1, Default::default(), cx);
        assert_eq!(f.left.read(cx).selected(), Some("alpha.txt"));
        let palette = Palette::current(cx);
        assert!(
            w.painted_quads()
                .iter()
                .any(|q| q.bounds == row.scale(w.scale_factor())
                    && q.background == palette.selected.into())
        );
        w.within("left").within("local-files").hover(2usize, cx);
        w.render_frame(cx);
        let hovered = w.within("left").within("local-files").find(2usize).bounds();
        assert!(
            w.painted_quads()
                .iter()
                .any(|q| q.bounds == hovered.scale(w.scale_factor())
                    && q.background == palette.hover.into())
        );
        let position = w
            .within("left")
            .within("local-files")
            .find(1usize)
            .bounds()
            .center();
        pointer_click(
            w,
            position,
            1,
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
            cx,
        );
        assert!(f.left.read(cx).files.selection().is_empty());
        assert!(f.left.read(cx).marked().is_empty());
        w.press("space", cx);
        w.within("left")
            .within("local-files")
            .click(("local-tree-toggle", 0usize), cx);
        assert_eq!(f.left.read(cx).marked(), ["alpha.txt"]);
    })
    .unwrap();
    f.wait_idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        let list = w.within("left").find("local-files-pane").bounds();
        let row = w.within("left").within("local-files").find(2usize).bounds();
        assert_eq!(f.left.read(cx).files.row(2), Some("src/a.txt"));
        assert_eq!(row.origin.x, list.origin.x);
        assert_eq!(row.size.width, list.size.width);
        for position in [
            point(list.right() - px(4.), row.center().y),
            point(list.origin.x + px(3.), row.center().y),
        ] {
            w.within("left").within("local-files").click(0usize, cx);
            pointer_click(w, position, 1, Default::default(), cx);
            assert_eq!(f.left.read(cx).selected(), Some("src/a.txt"));
            assert!(
                w.painted_quads()
                    .iter()
                    .any(|q| q.bounds == row.scale(w.scale_factor())
                        && q.background == Palette::current(cx).selected.into())
            );
        }
        w.within("left")
            .within("local-files")
            .click(("local-tree-toggle", 0usize), cx);
        assert!(!f.left.read(cx).tree.node("src").unwrap().expanded);
        assert_eq!(f.left.read(cx).marked(), ["alpha.txt"]);
        assert_eq!(
            f.left.read(cx).location().unwrap().directory,
            f.left_root.path()
        );
    })
    .unwrap();
    assert!(
        f.events
            .borrow()
            .iter()
            .all(|e| matches!(e, ObservedEvent::Selected(_, _, false)))
    );
}

#[gpui_kit::test]
async fn modifier_clicks_keep_shift_selection_separate_from_marking(cx: &mut TestAppContext) {
    let f = Fixture::new(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        for modifiers in [
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
            gpui_kit::Modifiers {
                platform: true,
                ..Default::default()
            },
        ] {
            for count in [1, 2] {
                w.render_frame(cx);
                let position = w
                    .within("left")
                    .within("local-files")
                    .find(0usize)
                    .bounds()
                    .center();
                pointer_click(w, position, count, modifiers, cx);
                assert_eq!(f.left.read(cx).files.is_selected("src"), count == 1);
                assert!(f.left.read(cx).marked().is_empty());
                assert!(!f.left.read(cx).is_loading());
                assert!(!f.left.read(cx).tree.node("src").unwrap().expanded);
            }
        }
        w.within("left").within("local-files").click(1usize, cx);
        let position = w
            .within("left")
            .within("local-files")
            .find(3usize)
            .bounds()
            .center();
        pointer_click(
            w,
            position,
            1,
            gpui_kit::Modifiers {
                shift: true,
                ..Default::default()
            },
            cx,
        );
        assert!(f.left.read(cx).marked().is_empty());
        assert_eq!(f.left.read(cx).selected(), Some("charlie.txt"));
        assert_eq!(
            f.left.read(cx).selection(),
            ["alpha.txt", "bravo.txt", "charlie.txt"]
        );
        w.within("left").within("local-files").click(1usize, cx);
        w.press("space", cx);
        w.press("v", cx);
        let marks = f.left.read(cx).marked();
        assert_eq!(marks, ["alpha.txt"]);
        for modifiers in [
            gpui_kit::Modifiers {
                shift: true,
                ..Default::default()
            },
            gpui_kit::Modifiers {
                shift: true,
                control: true,
                ..Default::default()
            },
            gpui_kit::Modifiers {
                shift: true,
                platform: true,
                ..Default::default()
            },
        ] {
            for (index, path) in [(0usize, "src"), (3usize, "charlie.txt")] {
                for count in [1, 2] {
                    w.render_frame(cx);
                    let row = w.within("left").within("local-files").find(index).bounds();
                    pointer_click(
                        w,
                        point(row.right() - px(4.), row.center().y),
                        count,
                        modifiers,
                        cx,
                    );
                    assert_eq!(f.left.read(cx).selected(), Some(path));
                    assert_eq!(f.left.read(cx).marked(), marks);
                    assert!(f.left.read(cx).files.range_active());
                    assert!(!f.left.read(cx).tree.node("src").unwrap().expanded);
                    assert!(!f.left.read(cx).is_loading());
                }
            }
        }
        w.press("v", cx);
        assert_eq!(
            f.left.read(cx).marked(),
            ["alpha.txt", "bravo.txt", "charlie.txt"]
        );
    })
    .unwrap();
    assert!(
        f.events
            .borrow()
            .iter()
            .all(|e| matches!(e, ObservedEvent::Selected(_, _, false)))
    );
}

#[gpui_kit::test]
async fn inactive_and_stale_local_row_callbacks_cannot_focus_or_select(cx: &mut TestAppContext) {
    let f = Fixture::new(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        f.left
            .update(cx, |p, cx| p.set_navigation_active(false, cx));
        f.right.update(cx, |p, cx| p.focus(w, cx));
        for id in [
            gpui_kit::ElementId::from(0usize),
            gpui_kit::ElementId::from(1usize),
            gpui_kit::ElementId::from(("local-tree-toggle", 0usize)),
        ] {
            for count in [1, 2] {
                w.render_frame(cx);
                let position = w
                    .within("left")
                    .within("local-files")
                    .find(id.clone())
                    .bounds()
                    .center();
                pointer_event(w, position, count, Default::default(), true, cx);
                // track_focus handles mouse-down natively; isolate the guarded click callback.
                f.right.update(cx, |p, cx| p.focus(w, cx));
                pointer_event(w, position, count, Default::default(), false, cx);
                assert!(
                    f.right.read(cx).browser_focus.is_focused(w),
                    "inactive callback stole focus"
                );
                assert_eq!(f.left.read(cx).selected(), None);
                assert!(!f.left.read(cx).is_loading());
            }
        }
        f.left.update(cx, |p, cx| p.set_navigation_active(true, cx));
        w.render_frame(cx);
        let position = w
            .within("left")
            .within("local-files")
            .find(1usize)
            .bounds()
            .center();
        pointer_event(w, position, 1, Default::default(), true, cx);
        f.left
            .update(cx, |p, cx| p.command(BrowserCommand::Refresh, w, cx));
        f.right.update(cx, |p, cx| p.focus(w, cx));
        // Dispatch against the captured frame, before a replacement row is rendered.
        pointer_event(w, position, 1, Default::default(), false, cx);
        assert!(
            !f.left.read(cx).browser_focus.is_focused(w),
            "stale row focused the local browser"
        );
        assert_eq!(f.left.read(cx).selected(), None);
    })
    .unwrap();
    f.wait_idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        let position = w
            .within("left")
            .within("local-files")
            .find(1usize)
            .bounds()
            .center();
        pointer_event(w, position, 2, Default::default(), true, cx);
        f.left
            .update(cx, |p, cx| p.open(f._right_root.path().into(), w, cx));
        f.right.update(cx, |p, cx| p.focus(w, cx));
        pointer_event(w, position, 2, Default::default(), false, cx);
        assert!(
            f.right.read(cx).browser_focus.is_focused(w),
            "old-root callback stole focus"
        );
        assert_eq!(f.left.read(cx).selected(), None);
    })
    .unwrap();
    f.wait_idle(cx).await;
    assert_eq!(
        f.left
            .read_with(cx, |p, _| p.location().unwrap().directory.clone()),
        f._right_root.path()
    );
    assert!(
        f.events
            .borrow()
            .iter()
            .all(|e| matches!(e, ObservedEvent::Selected(_, _, false)))
    );
}
