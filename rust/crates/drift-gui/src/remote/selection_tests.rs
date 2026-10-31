use super::*;

async fn exercise_multi_selection(
    cx: &mut TestAppContext,
    host: Host,
    options: ConnectOptions,
    root: &std::path::Path,
    location: Option<Location>,
) {
    fs::create_dir(root.join("child")).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(name), name).unwrap();
    }
    let (handle, _, pane) = open(cx, host, options, location);
    connected(cx, handle, &pane).await;
    let session = pane.read_with(cx, |p, _| p.session.clone().unwrap());
    let paths: Vec<_> = ["child", "a.txt", "b.txt", "c.txt"]
        .iter()
        .map(|name| format!("{}/{name}", session.root.trim_end_matches('/')))
        .collect();
    cx.update_window(handle, |_, w, cx| {
        let click = |w: &mut Window, index, modifiers, cx: &mut App| {
            w.render_frame(cx);
            let row = w.within("remote-files").find(index).bounds();
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
        w.within("remote-files").click(3usize, cx);
        click(w, 1usize, shift, cx);
        assert_eq!(pane.read(cx).selection(), paths[1..]);
        click(w, 2usize, shift, cx);
        assert_eq!(pane.read(cx).selection(), paths[2..]);
        click(w, 0usize, control, cx);
        assert_eq!(
            pane.read(cx).selection(),
            [paths[0].clone(), paths[2].clone(), paths[3].clone()]
        );
        click(
            w,
            1usize,
            gpui_kit::Modifiers {
                shift: true,
                control: true,
                ..Default::default()
            },
            cx,
        );
        assert_eq!(pane.read(cx).selection(), paths);
        assert!(pane.read(cx).marked().is_empty());
        assert!(!pane.read(cx).tree.node(&paths[0]).unwrap().expanded);
        assert!(!pane.read(cx).is_loading());
        click(w, 3usize, control, cx);
        assert_eq!(pane.read(cx).selection(), paths[..3]);
        assert_eq!(pane.read(cx).selected(), Some(paths[3].as_str()));
        for index in 0..4usize {
            let row = w.within("remote-files").find(index).bounds();
            let highlighted = w.painted_quads().iter().any(|q| {
                q.bounds == row.scale(w.scale_factor())
                    && q.background == Palette::current(cx).selected.into()
            });
            assert_eq!(highlighted, index < 3, "row {index}");
        }
        click(w, 0usize, shift, cx);
        assert_eq!(pane.read(cx).selection(), paths);
        w.within("remote-files").click(0usize, cx);
        click(w, 2usize, shift, cx);
        assert_eq!(pane.read(cx).selection(), paths[..3]);
        w.press("space", cx);
        let mut expected = paths[..3].to_vec();
        expected.sort();
        assert_eq!(pane.read(cx).marked(), expected);
        assert_eq!(
            w.find("remote-selection-status").label(),
            Some("3 marked · 3 selected")
        );
        w.press("space", cx);
        assert!(pane.read(cx).marked().is_empty());
        w.press("ctrl-shift-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text(),
            Some(paths[..3].join("\n"))
        );
        assert_eq!(
            w.find("remote-selection-status").label(),
            Some("0 marked · 3 selected")
        );
        w.press("v", cx);
        assert_eq!(
            w.find("remote-selection-status").label(),
            Some("0 marked · range active · 3 selected")
        );
        w.press("escape", cx);
        pane.update(cx, |p, _| {
            p.files
                .restrict_marks([paths[1].clone(), paths[2].clone()].into())
        });
        w.within("remote-files").right_click(0usize, cx);
        assert_eq!(pane.read(cx).selection(), paths[..3]);
        assert_eq!(pane.read(cx).selected(), Some(paths[0].as_str()));
        click_menu(w, "Mark", cx);
        assert_eq!(pane.read(cx).marked(), paths[1..3]);
        w.within("remote-files").right_click(0usize, cx);
        click_menu(w, "Unmark", cx);
        assert!(pane.read(cx).marked().is_empty());
        w.press("space", cx);
        assert_eq!(pane.read(cx).marked(), paths[1..3]);
        w.press("space", cx);
        assert!(pane.read(cx).marked().is_empty());
        w.within("remote-files").right_click(3usize, cx);
        assert_eq!(pane.read(cx).selection(), [paths[3].clone()]);
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(pane.read(cx).menu.is_none());
        w.within("remote-files").click(0usize, cx);
        let position = w.within("remote-files").find(3usize).bounds().center();
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
        for (key, index) in [("up", 2), ("down", 3), ("home", 0), ("end", 3)] {
            w.press(key, cx);
            assert_eq!(pane.read(cx).selection(), [paths[index].clone()], "{key}");
            assert_eq!(pane.read(cx).selected(), Some(paths[index].as_str()));
        }
        w.press("home", cx);
        w.press("shift-down", cx);
        assert_eq!(pane.read(cx).marked(), [paths[1].clone()]);
        assert_eq!(pane.read(cx).selection(), [paths[1].clone()]);
        w.within("remote-files").click(1usize, cx);
        w.render_frame(cx);
        let position = w.within("remote-files").find(2usize).bounds().center();
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
        assert_eq!(pane.read(cx).selection(), paths[1..3]);
        assert_eq!(pane.read(cx).selected(), Some(paths[2].as_str()));
        w.press("f5", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    assert_eq!(pane.read_with(cx, |p, _| p.selection()), paths[1..3]);
    fs::remove_file(root.join("b.txt")).unwrap();
    cx.update_window(handle, |_, w, cx| w.press("f5", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    assert_eq!(pane.read_with(cx, |p, _| p.selection()), [paths[1].clone()]);
    assert_eq!(
        pane.read_with(cx, |p, _| p.selected().map(str::to_owned)),
        None
    );
    session.shutdown().await;
}

#[gpui_kit::test]
async fn multi_selection_pointer_highlights_mark_copy_menu_and_keyboard_real_sftp(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    exercise_multi_selection(
        cx,
        server.host(),
        server.options(),
        &server.dir.path().join("files"),
        None,
    )
    .await;
}

#[gpui_kit::test]
async fn multi_selection_pointer_highlights_mark_copy_menu_and_keyboard_real_ftp(
    cx: &mut TestAppContext,
) {
    let server = support::ftp::Server::new(4);
    exercise_multi_selection(
        cx,
        server.host(),
        server.options(),
        &server.dir.path().join("files"),
        None,
    )
    .await;
    for command in ["RETR", "STOR", "APPE", "DELE", "RMD", "MKD", "RNFR", "RNTO"] {
        assert_eq!(server.commands(command), 0, "unexpected {command}");
    }
}

#[gpui_kit::test]
async fn multi_selection_pointer_with_real_ftp_visibility_classification(cx: &mut TestAppContext) {
    let server = support::ftp::Server::new(4);
    let root = server.dir.path().join("files");
    let location = Location {
        root: Arc::new(ProjectRoot::open(&root).unwrap()),
        directory: root.clone(),
        slug: Some("test".into()),
        config: RuntimeConfig {
            hosts: vec![],
            mappings: vec![],
            ui: Default::default(),
        },
    };
    exercise_multi_selection(cx, server.host(), server.options(), &root, Some(location)).await;
    for command in ["RETR", "STOR", "APPE", "DELE", "RMD", "MKD", "RNFR", "RNTO"] {
        assert_eq!(server.commands(command), 0, "unexpected {command}");
    }
}
