use super::*;

#[gpui_kit::test]
async fn multi_selection_pointer_highlights_mark_copy_menu_and_keyboard(cx: &mut TestAppContext) {
    let f = Fixture::new(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let click = |w: &mut Window, index, modifiers, cx: &mut App| {
            w.render_frame(cx);
            let row = w.within("left").within("local-files").find(index).bounds();
            pointer_click(
                w,
                point(row.right() - px(4.), row.center().y),
                2,
                modifiers,
                cx,
            );
        };
        let shift = gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        };
        let control = gpui_kit::Modifiers {
            control: true,
            ..Default::default()
        };
        w.within("left").within("local-files").click(3usize, cx);
        click(w, 1usize, shift, cx);
        assert_eq!(
            f.left.read(cx).selection(),
            ["alpha.txt", "bravo.txt", "charlie.txt"]
        );
        click(w, 2usize, shift, cx);
        assert_eq!(f.left.read(cx).selection(), ["bravo.txt", "charlie.txt"]);
        click(w, 0usize, control, cx);
        assert_eq!(
            f.left.read(cx).selection(),
            ["src", "bravo.txt", "charlie.txt"]
        );
        click(
            w,
            1usize,
            gpui_kit::Modifiers {
                shift: true,
                platform: true,
                ..Default::default()
            },
            cx,
        );
        assert_eq!(
            f.left.read(cx).selection(),
            ["src", "alpha.txt", "bravo.txt", "charlie.txt"]
        );
        assert!(f.left.read(cx).marked().is_empty());
        assert!(!f.left.read(cx).tree.node("src").unwrap().expanded);
        assert!(!f.left.read(cx).is_loading());
        click(w, 3usize, control, cx);
        assert_eq!(f.left.read(cx).selected(), Some("charlie.txt"));
        assert_eq!(
            f.left.read(cx).selection(),
            ["src", "alpha.txt", "bravo.txt"]
        );
        for index in 0..4usize {
            let row = w.within("left").within("local-files").find(index).bounds();
            let highlighted = w.painted_quads().iter().any(|q| {
                q.bounds == row.scale(w.scale_factor())
                    && q.background == Palette::current(cx).selected.into()
            });
            assert_eq!(highlighted, index < 3, "row {index}");
        }
        click(w, 0usize, shift, cx);
        assert_eq!(
            f.left.read(cx).selection(),
            ["src", "alpha.txt", "bravo.txt", "charlie.txt"]
        );
        w.within("left").within("local-files").click(0usize, cx);
        click(w, 2usize, shift, cx);
        assert_eq!(
            f.left.read(cx).selection(),
            ["src", "alpha.txt", "bravo.txt"]
        );
        w.press("space", cx);
        assert_eq!(f.left.read(cx).marked(), ["alpha.txt", "bravo.txt", "src"]);
        assert_eq!(
            w.within("left").find("local-selection-status").label(),
            Some("3 marked · 3 selected")
        );
        w.press("space", cx);
        assert!(f.left.read(cx).marked().is_empty());
        w.press("ctrl-shift-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("src\nalpha.txt\nbravo.txt")
        );
        assert_eq!(
            w.within("left").find("local-selection-status").label(),
            Some("0 marked · 3 selected")
        );
        w.press("v", cx);
        assert_eq!(
            w.within("left").find("local-selection-status").label(),
            Some("0 marked · range active · 3 selected")
        );
        w.press("escape", cx);
        w.within("left")
            .within("local-files")
            .right_click(1usize, cx);
        assert_eq!(f.left.read(cx).selected(), Some("alpha.txt"));
        assert_eq!(
            f.left.read(cx).selection(),
            ["src", "alpha.txt", "bravo.txt"]
        );
        click_item(w, "Mark", cx);
        assert_eq!(f.left.read(cx).marked(), ["alpha.txt", "bravo.txt", "src"]);
        w.within("left")
            .within("local-files")
            .right_click(2usize, cx);
        click_item(w, "Unmark", cx);
        assert!(f.left.read(cx).marked().is_empty());
        w.within("left").within("local-files").click(1usize, cx);
        w.press("space", cx);
        w.within("left").within("local-files").click(0usize, cx);
        click(w, 2usize, shift, cx);
        w.within("left")
            .within("local-files")
            .right_click(1usize, cx);
        click_item(w, "Mark", cx);
        assert_eq!(f.left.read(cx).marked(), ["alpha.txt", "bravo.txt", "src"]);
        w.within("left")
            .within("local-files")
            .right_click(2usize, cx);
        click_item(w, "Unmark", cx);
        assert!(f.left.read(cx).marked().is_empty());
        w.within("left")
            .within("local-files")
            .right_click(3usize, cx);
        assert_eq!(f.left.read(cx).selection(), ["charlie.txt"]);
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        assert!(f.left.read(cx).menu.is_none());
        w.within("left").within("local-files").click(0usize, cx);
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
        for (key, path) in [
            ("up", "bravo.txt"),
            ("down", "charlie.txt"),
            ("home", "src"),
            ("end", "charlie.txt"),
        ] {
            w.press(key, cx);
            assert_eq!(f.left.read(cx).selection(), [path], "{key}");
            assert_eq!(f.left.read(cx).selected(), Some(path));
        }
        w.press("home", cx);
        w.press("shift-down", cx);
        assert_eq!(f.left.read(cx).marked(), ["alpha.txt", "src"]);
        assert_eq!(f.left.read(cx).selection(), ["alpha.txt"]);
        assert!(f.right.read(cx).selection().is_empty());
        w.within("left").within("local-files").click(1usize, cx);
        w.render_frame(cx);
        let position = w
            .within("left")
            .within("local-files")
            .find(2usize)
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
        assert_eq!(f.left.read(cx).selection(), ["alpha.txt", "bravo.txt"]);
        assert_eq!(f.left.read(cx).selected(), Some("bravo.txt"));
        w.press("f5", cx);
    })
    .unwrap();
    f.wait_idle(cx).await;
    assert_eq!(
        f.left.read_with(cx, |p, _| p.selection()),
        ["alpha.txt", "bravo.txt"]
    );
    fs::remove_file(f.left_root.path().join("bravo.txt")).unwrap();
    cx.update_window(f.handle, |_, w, cx| w.press("f5", cx))
        .unwrap();
    f.wait_idle(cx).await;
    assert_eq!(f.left.read_with(cx, |p, _| p.selection()), ["alpha.txt"]);
    assert_eq!(
        f.left.read_with(cx, |p, _| p.selected().map(str::to_owned)),
        None
    );
}

#[gpui_kit::test]
async fn disclosure_toggle_keeps_multi_selection_so_children_can_be_added(cx: &mut TestAppContext) {
    let f = Fixture::new(cx).await;
    let control = gpui_kit::Modifiers {
        control: true,
        ..Default::default()
    };
    cx.update_window(f.handle, |_, w, cx| {
        w.within("left").within("local-files").click(1usize, cx);
        w.render_frame(cx);
        let bravo = w.within("left").within("local-files").find(2usize).bounds();
        pointer_click(w, bravo.center(), 1, control, cx);
        assert_eq!(f.left.read(cx).selection(), ["alpha.txt", "bravo.txt"]);
        w.within("left")
            .within("local-files")
            .click(("local-tree-toggle", 0usize), cx);
    })
    .unwrap();
    f.wait_idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert!(f.left.read(cx).tree.node("src").unwrap().expanded);
        assert_eq!(f.left.read(cx).selected(), Some("src"));
        assert_eq!(f.left.read(cx).selection(), ["alpha.txt", "bravo.txt"]);
        w.render_frame(cx);
        let child = (0..8usize)
            .find(|index| f.left.read(cx).files.row(*index) == Some("src/a.txt"))
            .expect("expanded child row");
        let bounds = w.within("left").within("local-files").find(child).bounds();
        pointer_click(w, bounds.center(), 1, control, cx);
        assert_eq!(
            f.left.read(cx).selection(),
            ["src/a.txt", "alpha.txt", "bravo.txt"]
        );
        w.within("left")
            .within("local-files")
            .click(("local-tree-toggle", 0usize), cx);
        w.render_frame(cx);
        assert!(!f.left.read(cx).tree.node("src").unwrap().expanded);
        assert_eq!(f.left.read(cx).selection(), ["alpha.txt", "bravo.txt"]);
        assert!(f.left.read(cx).marked().is_empty());
    })
    .unwrap();
}
