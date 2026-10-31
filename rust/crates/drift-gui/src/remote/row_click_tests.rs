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

async fn exercise_directory_clicks(
    cx: &mut TestAppContext,
    host: Host,
    options: ConnectOptions,
    root: &std::path::Path,
    location: Option<Location>,
) {
    fs::create_dir_all(root.join("child/nested")).unwrap();
    fs::create_dir(root.join("child/node_modules")).unwrap();
    fs::write(root.join("child/text.txt"), "remote").unwrap();
    fs::write(root.join("child/.hidden"), "hidden").unwrap();
    fs::write(root.join("top.txt"), "top").unwrap();
    let (handle, view, pane) = open(cx, host, options, location);
    connected(cx, handle, &pane).await;
    let session = pane.read_with(cx, |p, _| p.session.clone().unwrap());
    let parent = pane.read_with(cx, |p, _| p.path.clone());
    let child = format!("{}/child", parent.trim_end_matches('/'));
    let top = format!("{}/top.txt", parent.trim_end_matches('/'));
    cx.update_window(handle, |_, w, cx| {
        pane.update(cx, |p, cx| p.focus_filter(&FocusFilter, w, cx));
        w.within("remote-files").click(0usize, cx);
        assert!(pane.read(cx).focus.is_focused(w));
        assert_eq!(pane.read(cx).selected(), Some(child.as_str()));
        assert!(!pane.read(cx).is_loading());
        assert!(!pane.read(cx).tree.node(&child).unwrap().expanded);
        assert_eq!(pane.read(cx).path, parent);
        w.within("remote-files").double_click(0usize, cx);
        assert_eq!(pane.read(cx).tree_pending.as_deref(), Some(child.as_str()));
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).path, parent);
        assert!(pane.read(cx).history.previous().is_none());
        assert!(pane.read(cx).tree.node(&child).unwrap().expanded);
        assert_eq!(pane.read(cx).files.len(), 4);
        w.within("remote-files").click(0usize, cx);
        assert!(pane.read(cx).tree.node(&child).unwrap().expanded);
        w.within("remote-files").double_click(0usize, cx);
        assert!(!pane.read(cx).tree.node(&child).unwrap().expanded);
        assert_eq!(pane.read(cx).files.len(), 2);
        assert_eq!(pane.read(cx).path, parent);
        assert!(pane.read(cx).history.previous().is_none());
        assert!(pane.read(cx).history.next().is_none());
        w.within("remote-files").double_click(0usize, cx);
        let expansion = pane.read(cx).listing.as_ref().unwrap().clone();
        w.within("remote-files").click(0usize, cx);
        assert_eq!(pane.read(cx).tree_pending.as_deref(), Some(child.as_str()));
        assert!(
            !expansion.is_cancelled(),
            "single click cancelled an expansion"
        );
        // Collapse a real outstanding bounded list without closing the transport.
        w.within("remote-files").double_click(0usize, cx);
        assert!(!expansion.is_cancelled());
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.executor().run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        assert!(!pane.read(cx).tree.node(&child).unwrap().expanded);
        assert!(
            pane.read(cx)
                .tree
                .node(&format!("{child}/text.txt"))
                .is_none()
        );
        assert_eq!(pane.read(cx).path, parent);
        assert!(pane.read(cx).history.previous().is_none());
        assert_eq!(pane.read(cx).session_id(), Some(session.id));
        assert_eq!(session.state(), ConnectionState::Connected);
        w.within("remote-files").click(1usize, cx);
        assert_eq!(pane.read(cx).selected(), Some(top.as_str()));
        w.within("remote-files").click(0usize, cx);
        w.within("remote-files").double_click(1usize, cx);
        assert_eq!(pane.read(cx).selected(), Some(top.as_str()));
        assert!(!pane.read(cx).is_loading());
        w.render_frame(cx);
        let list = w.find("remote-files-pane").bounds();
        let row = w.within("remote-files").find(1usize).bounds();
        assert_eq!(row.origin.x, list.origin.x + metric(2., cx));
        assert_eq!(row.right(), list.right());
        w.within("remote-files").click(0usize, cx);
        pointer_click(
            w,
            point(list.right() - px(4.), row.center().y),
            1,
            Default::default(),
            cx,
        );
        assert_eq!(pane.read(cx).selected(), Some(top.as_str()));
        assert!(
            w.painted_quads()
                .iter()
                .any(|q| q.bounds == row.scale(w.scale_factor())
                    && q.background == Palette::current(cx).selected.into())
        );
        w.within("remote-files").hover(0usize, cx);
        w.render_frame(cx);
        let hovered = w.within("remote-files").find(0usize).bounds();
        assert!(
            w.painted_quads()
                .iter()
                .any(|q| q.bounds == hovered.scale(w.scale_factor())
                    && q.background == Palette::current(cx).hover.into())
        );
        let position = w.within("remote-files").find(1usize).bounds().center();
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
        assert!(pane.read(cx).selection().is_empty());
        assert!(pane.read(cx).marked().is_empty());
        w.press("space", cx);
        // The disclosure remains a separate single-click target and preserves marks.
        w.within("remote-files")
            .click(("remote-tree-toggle", 0usize), cx);
        assert_eq!(
            pane.read(cx).marked().as_slice(),
            std::slice::from_ref(&top)
        );
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let list = w.find("remote-files-pane").bounds();
        let row = w.within("remote-files").find(2usize).bounds();
        let text = format!("{child}/text.txt");
        assert_eq!(pane.read(cx).files.row(2), Some(text.as_str()));
        assert_eq!(row.origin.x, list.origin.x + metric(2., cx));
        assert_eq!(row.right(), list.right());
        for position in [
            point(list.right() - px(4.), row.center().y),
            point(row.origin.x + px(3.), row.center().y),
        ] {
            w.within("remote-files").click(0usize, cx);
            pointer_click(w, position, 1, Default::default(), cx);
            assert_eq!(pane.read(cx).selected(), Some(text.as_str()));
            assert!(
                w.painted_quads()
                    .iter()
                    .any(|q| q.bounds == row.scale(w.scale_factor())
                        && q.background == Palette::current(cx).selected.into())
            );
        }
        w.within("remote-files")
            .click(("remote-tree-toggle", 0usize), cx);
        assert!(!pane.read(cx).tree.node(&child).unwrap().expanded);
        assert_eq!(
            pane.read(cx).marked().as_slice(),
            std::slice::from_ref(&top)
        );
        w.within("remote-files").click(1usize, cx);
        let position = w.within("remote-files").find(1usize).bounds().center();
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
        assert_eq!(
            pane.read(cx).marked().as_slice(),
            std::slice::from_ref(&top)
        );
        w.press("space", cx);
        assert!(pane.read(cx).marked().is_empty());
        w.within("remote-files").click(1usize, cx);
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
                let position = w.within("remote-files").find(0usize).bounds().center();
                pointer_click(w, position, count, modifiers, cx);
                assert_eq!(pane.read(cx).files.is_selected(&child), count == 1);
                assert!(pane.read(cx).marked().is_empty());
                assert!(!pane.read(cx).is_loading());
                assert!(!pane.read(cx).tree.node(&child).unwrap().expanded);
            }
        }
        w.within("remote-files").click(0usize, cx);
        let position = w.within("remote-files").find(1usize).bounds().center();
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
        assert!(pane.read(cx).marked().is_empty());
        assert_eq!(pane.read(cx).selected(), Some(top.as_str()));
        assert_eq!(pane.read(cx).selection(), [child.clone(), top.clone()]);
        w.within("remote-files").click(1usize, cx);
        w.press("space", cx);
        w.press("v", cx);
        let marks = pane.read(cx).marked();
        assert_eq!(marks.as_slice(), std::slice::from_ref(&top));
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
            for (index, path) in [(0usize, child.as_str()), (1usize, top.as_str())] {
                for count in [1, 2] {
                    w.render_frame(cx);
                    let row = w.within("remote-files").find(index).bounds();
                    pointer_click(
                        w,
                        point(row.right() - px(4.), row.center().y),
                        count,
                        modifiers,
                        cx,
                    );
                    assert_eq!(pane.read(cx).selected(), Some(path));
                    assert_eq!(pane.read(cx).marked(), marks);
                    assert!(pane.read(cx).files.range_active());
                    assert!(!pane.read(cx).tree.node(&child).unwrap().expanded);
                    assert!(!pane.read(cx).is_loading());
                }
            }
        }
        w.press("v", cx);
        assert_eq!(
            pane.read(cx).marked().as_slice(),
            std::slice::from_ref(&top)
        );
    })
    .unwrap();
    assert!(view.read_with(cx, |v, _| v.events.iter().all(|e| e == "selection")));
    cx.update_window(handle, |_, w, cx| {
        w.within("remote-files").click(0usize, cx);
        w.press("alt-enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).path, child);
        assert!(pane.read(cx).history.previous().is_some());
        assert_eq!(pane.read(cx).session_id(), Some(session.id));
        w.press("alt-left", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        let selected = pane.read(cx).selected().map(str::to_owned);
        w.render_frame(cx);
        let position = w.within("remote-files").find(1usize).bounds().center();
        pointer_event(w, position, 1, Default::default(), true, cx);
        pane.update(cx, |p, cx| p.refresh(&Refresh, w, cx));
        pane.update(cx, |p, cx| p.focus_filter(&FocusFilter, w, cx));
        pointer_event(w, position, 1, Default::default(), false, cx);
        assert!(!pane.read(cx).focus.is_focused(w));
        assert_eq!(pane.read(cx).selected(), selected.as_deref());
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        pane.update(cx, |p, cx| p.set_workspace_active(false, false, cx));
        pane.update(cx, |p, cx| p.focus_filter(&FocusFilter, w, cx));
        let selected = pane.read(cx).selected().map(str::to_owned);
        for id in [
            gpui_kit::ElementId::from(0usize),
            gpui_kit::ElementId::from(1usize),
            gpui_kit::ElementId::from(("remote-tree-toggle", 0usize)),
        ] {
            for count in [1, 2] {
                w.render_frame(cx);
                let position = w.within("remote-files").find(id.clone()).bounds().center();
                pointer_event(w, position, count, Default::default(), true, cx);
                // Ignore native track_focus on mouse-down; the callback must not steal it back.
                pane.update(cx, |p, cx| p.focus_filter(&FocusFilter, w, cx));
                pointer_event(w, position, count, Default::default(), false, cx);
                assert!(pane.read(cx).filter.focus_handle(cx).is_focused(w));
                assert_eq!(pane.read(cx).selected(), selected.as_deref());
                assert!(!pane.read(cx).is_loading());
            }
        }
        assert_eq!(pane.read(cx).path, parent);
        assert_eq!(session.state(), ConnectionState::Connected);
        pane.update(cx, |p, cx| p.set_workspace_active(true, true, cx));
        w.render_frame(cx);
        let position = w.within("remote-files").find(1usize).bounds().center();
        pointer_event(w, position, 2, Default::default(), true, cx);
        pane.update(cx, |p, cx| p.set_context(8, vec![], cx));
        pane.update(cx, |p, cx| p.focus_filter(&FocusFilter, w, cx));
        pointer_event(w, position, 2, Default::default(), false, cx);
        assert!(
            pane.read(cx).filter.focus_handle(cx).is_focused(w),
            "old-session callback stole focus"
        );
        assert!(pane.read(cx).selected().is_none());
        assert!(!pane.read(cx).has_session());
    })
    .unwrap();
    assert!(view.read_with(cx, |v, _| v.events.iter().all(|e| e == "selection")));
    assert_eq!(
        fs::read_to_string(root.join("child/text.txt")).unwrap(),
        "remote"
    );
    assert_eq!(fs::read_to_string(root.join("top.txt")).unwrap(), "top");
    session.shutdown().await;
}

#[gpui_kit::test]
async fn directory_row_clicks_real_sftp(cx: &mut TestAppContext) {
    let server = support::Server::new(false);
    exercise_directory_clicks(
        cx,
        server.host(),
        server.options(),
        &server.dir.path().join("files"),
        None,
    )
    .await;
}

#[gpui_kit::test]
async fn directory_row_clicks_real_ftp_no_transfers(cx: &mut TestAppContext) {
    let server = support::ftp::Server::new(4);
    exercise_directory_clicks(
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
async fn directory_row_clicks_real_ftp_with_local_visibility_classification(
    cx: &mut TestAppContext,
) {
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
    exercise_directory_clicks(cx, server.host(), server.options(), &root, Some(location)).await;
    for command in ["RETR", "STOR", "APPE", "DELE", "RMD", "MKD", "RNFR", "RNTO"] {
        assert_eq!(server.commands(command), 0, "unexpected {command}");
    }
}
