use super::*;
use gpui_kit::InputEvent as _;
use std::{cell::Cell, rc::Rc};

struct ClassificationObserver {
    _subscription: Subscription,
}

#[gpui_kit::test]
async fn double_click_during_real_expansion_visibility_collapses_without_stale_rows(
    cx: &mut TestAppContext,
) {
    let server = support::ftp::Server::new(4);
    let root = server.dir.path().join("files");
    fs::create_dir(root.join("child")).unwrap();
    fs::write(root.join("child/text.txt"), "remote").unwrap();
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
    let (handle, _, pane) = open(cx, server.host(), server.options(), Some(location));
    connected(cx, handle, &pane).await;
    let session = pane.read_with(cx, |pane, _| pane.session.clone().unwrap());
    let collapsed = Rc::new(Cell::new(false));
    let _observer = cx.update_window(handle, |_, window, cx| {
        let pane = pane.clone();
        let collapsed = collapsed.clone();
        cx.new(|cx| ClassificationObserver {
            _subscription: cx.subscribe_in(&pane, window, move |_, pane, event, window, cx| {
                if !matches!(event, RemoteEvent::Status { message, .. } if message == "Classifying remote paths…")
                    || collapsed.replace(true) {
                    return;
                }
                assert!(pane.read(cx).listing.is_none());
                assert!(pane.read(cx).tree_pending.is_none());
                assert!(pane.read(cx).tree.node("/child").unwrap().expanded);
                assert!(pane.read(cx).visibility_cancel.is_some());
                window.render_frame(cx);
                let position = window.find(0usize).bounds().center();
                window.dispatch_event(gpui_kit::MouseDownEvent {
                    button: MouseButton::Left, position, modifiers: Default::default(),
                    click_count: 2, first_mouse: false,
                }.to_platform_input(), cx);
                window.render_frame(cx);
                window.dispatch_event(gpui_kit::MouseUpEvent {
                    button: MouseButton::Left, position, modifiers: Default::default(), click_count: 2,
                }.to_platform_input(), cx);
                assert!(!pane.read(cx).tree.node("/child").unwrap().expanded);
                assert_eq!(pane.read(cx).path, "/");
                assert!(pane.read(cx).listing.is_none());
            }),
        })
    }).unwrap();
    cx.update_window(handle, |_, w, cx| w.double_click(0usize, cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        collapsed.get() && !pane.read(cx).is_loading()
    })
    .await;
    cx.executor().run_until_parked();
    assert_eq!(pane.read_with(cx, |pane, _| pane.path.clone()), "/");
    assert!(pane.read_with(cx, |pane, _| pane.history.previous().is_none()));
    assert!(pane.read_with(cx, |pane, _| pane.history.next().is_none()));
    assert!(pane.read_with(cx, |pane, _| !pane.tree.node("/child").unwrap().expanded));
    assert!(pane.read_with(cx, |pane, _| pane.tree.node("/child/text.txt").is_none()));
    assert_eq!(
        pane.read_with(cx, |pane, _| pane.session_id()),
        Some(session.id)
    );
    assert_eq!(session.state(), ConnectionState::Connected);
    session.shutdown().await;
    for command in ["RETR", "STOR", "APPE", "DELE", "RMD", "MKD", "RNFR", "RNTO"] {
        assert_eq!(server.commands(command), 0, "unexpected {command}");
    }
}
