use super::*;
use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, NavigationDirection};

#[gpui_kit::test]
async fn paired_headers_keep_mouse_back_forward_in_their_own_real_histories(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir(local.path().join("child")).unwrap();
    fs::write(local.path().join("child/local.txt"), "local unchanged").unwrap();
    let remote_child = server.dir.path().join("files/child");
    fs::create_dir(&remote_child).unwrap();
    fs::write(remote_child.join("remote.txt"), "remote unchanged").unwrap();
    let store = Store::new(config.path().into());
    let project = store
        .register("History", local.path().to_path_buf())
        .unwrap();
    store
        .save_host(Some(&project.slug), None, server.host())
        .unwrap();
    let f = Fixture::new(
        1400.,
        1000.,
        store,
        local.path(),
        Some(server.options()),
        cx,
    )
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote-files-tab", cx);
        crate::test_support::connect_first(w, cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).remote.read(cx).has_session()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    let session = f
        .shell
        .read_with(cx, |shell, cx| shell.remote.read(cx).session_id());
    cx.update_window(f.handle, |_, w, cx| {
        w.within("local-files").click(0usize, cx);
        w.press("alt-enter", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(
            f.shell.read(cx).remote.read(cx).session_id(),
            session,
            "local entry kept session"
        );
        w.within("remote-files").click(0usize, cx);
        w.press("alt-enter", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
    })
    .unwrap();
    f.shell.read_with(cx, |shell, cx| {
        assert_eq!(
            shell.remote.read(cx).session_id(),
            session,
            "remote entry kept session"
        );
        assert_eq!(
            shell.remote.read(cx).selected(),
            remote_child.join("remote.txt").to_str(),
            "remote entry selected leaf"
        );
    });
    for id in [
        "local-pane-tabs",
        "local-navigation-toolbar",
        "local-browser-header",
        "local-path-header",
        "remote-navigation-toolbar",
        "remote-browser-header",
        "remote-pane-tabs",
        "remote-path-header",
    ] {
        for direction in [NavigationDirection::Back, NavigationDirection::Forward] {
            cx.update_window(f.handle, |_, w, cx| {
                w.render_frame(cx);
                let position = w.find(id).bounds().center();
                w.dispatch_event(
                    MouseDownEvent {
                        button: MouseButton::Navigate(direction),
                        position,
                        modifiers: Default::default(),
                        click_count: 1,
                        first_mouse: false,
                    }
                    .to_platform_input(),
                    cx,
                );
            })
            .unwrap();
            cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
                !f.shell.read(cx).browser.read(cx).is_loading()
                    && !f.shell.read(cx).remote.read(cx).is_loading()
            })
            .await;
            cx.update_window(f.handle, |_, w, cx| {
                f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
                w.press("home", cx);
                let shell = f.shell.read(cx);
                let local_dir = &shell.browser.read(cx).location().unwrap().directory;
                let local_expected =
                    if id.starts_with("local-") && direction == NavigationDirection::Back {
                        local.path().to_path_buf()
                    } else {
                        local.path().join("child")
                    };
                assert_eq!(local_dir, &local_expected, "{id}: {direction:?}");
                let remote_expected =
                    if id.starts_with("remote-") && direction == NavigationDirection::Back {
                        remote_child.clone()
                    } else {
                        remote_child.join("remote.txt")
                    };
                assert_eq!(
                    shell.remote.read(cx).selected(),
                    remote_expected.to_str(),
                    "{id}: {direction:?}"
                );
                assert_eq!(shell.remote.read(cx).session_id(), session);
            })
            .unwrap();
        }
    }
    let connection = f.shell.read_with(cx, |shell, cx| {
        shell.remote.read(cx).comparison_target().unwrap().1
    });
    connection.shutdown().await;
    assert_eq!(
        fs::read_to_string(local.path().join("child/local.txt")).unwrap(),
        "local unchanged"
    );
    assert_eq!(
        fs::read_to_string(remote_child.join("remote.txt")).unwrap(),
        "remote unchanged"
    );
}
