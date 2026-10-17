use super::*;
use crate::sftp_test_support as support;
use drift_core::{project::Project, remote::ConnectionState};
use gpui_kit::test::{ElementSnapshot, TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, process::Command, time::Duration};

fn menu_item(window: &mut Window, label: &str) -> (usize, ElementSnapshot) {
    let menu = window.within("popup-menu");
    (0..32)
        .find_map(|index| {
            let item = menu.try_find(index)?;
            (item.label() == Some(label)).then_some((index, item))
        })
        .unwrap_or_else(|| panic!("missing remote menu item {label}"))
}

fn ignored_requested(window: &Window, expected: bool) {
    // Native buttons expose the requested state in their label, not checked metadata.
    assert_eq!(
        window.find("remote-ignored").label(),
        Some(if expected {
            "Hide ignored"
        } else {
            "Show ignored"
        })
    );
}

fn ignored_menu(window: &mut Window, expected: bool, cx: &mut gpui_kit::App) {
    window.press("menu", cx);
    assert!(window.find("popup-menu").visible());
    assert!(menu_item(window, "Show ignored").1.visible());
    assert!(menu_item(window, "Disconnect").1.visible());
    let cancel = menu_item(window, "Cancel loading").0;
    window.within("popup-menu").click(cancel, cx);
    assert!(window.find("popup-menu").visible());
    ignored_requested(window, expected);
    window.press("escape", cx);
}

#[gpui_kit::test]
async fn native_remote_filter_menu_and_shell_help_do_not_dispatch_uppercase_i_as_a_letter_command(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::ftp::Server::new(4);
    let remote_file = server.dir.path().join("files/cache.log");
    fs::write(&remote_file, "remote").unwrap();
    let local = tempfile::tempdir().unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec!["config", "core.excludesFile", "/dev/null"],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(local.path())
            .args(&args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("Git is required for visibility tests");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(local.path().join(".gitignore"), "*.log\n").unwrap();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let now = drift_core::project::now();
    store
        .save_project(
            None,
            Project {
                slug: "visibility".into(),
                name: "Visibility".into(),
                path: local.path().into(),
                archived: false,
                created_at: now,
                updated_at: now,
                opened_at: None,
            },
        )
        .unwrap();
    store
        .save_host(Some("visibility"), None, server.host())
        .unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1400.), px(1000.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let browser = BrowserService::new().unwrap();
                    Shell::new(
                        window,
                        cx,
                        store.clone(),
                        browser.clone(),
                        RemoteService::with_options(browser, server.options()),
                        local.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    // The local pane is rendered before asynchronous project opening completes.
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        let browser = shell.read(cx).browser.read(cx);
        browser.location().is_some() && !browser.is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| window.press("@", cx))
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert!(shell.read(cx).show_remote);
        window.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    // Readiness includes both the real connection and local Git classification.
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        let remote = shell.read(cx).remote.read(cx);
        remote.has_session() && !remote.is_loading()
    })
    .await;
    let (session, connection) = shell.read_with(cx, |s, cx| {
        let remote = s.remote.read(cx);
        assert_eq!(remote.directory(), "/");
        (remote.comparison_target().unwrap().1, remote.connection())
    });
    let commands = fs::read(server.dir.path().join("commands")).unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert!(shell.read(cx).remote.focus_handle(cx).is_focused(window));
        ignored_menu(window, false, cx);
        window.press("i", cx);
        ignored_menu(window, false, cx);
        window.press("shift-i", cx);
        ignored_requested(window, true);
        ignored_menu(window, true, cx);
        window.press("menu", cx);
        window.press("shift-i", cx);
        window.press("i", cx);
        assert!(window.find("popup-menu").visible());
        assert!(menu_item(window, "Show ignored").1.visible());
        ignored_requested(window, true);
        window.press("escape", cx);
        window.click("remote-filter", cx);
        window.input("I", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(window.find("remote-filter").value(), Some("I"));
        assert_eq!(window.find("remote-filter").focused(), Some(true));
        ignored_requested(window, true);
        window.press("enter", cx);
        assert!(shell.read(cx).remote.focus_handle(cx).is_focused(window));
        ignored_menu(window, true, cx);
        window.press("f1", cx);
        assert!(window.find("shortcut-help").visible());
        window.press("shift-i", cx);
        window.press("i", cx);
        assert!(window.find("shortcut-help").visible());
        assert_eq!(
            shell.read(cx).remote.read(cx).session_id(),
            Some(session.id)
        );
        window.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert!(shell.read(cx).help.is_none());
        assert!(window.try_find("shortcut-help").is_none());
        assert!(shell.read(cx).remote.focus_handle(cx).is_focused(window));
        ignored_menu(window, true, cx);
        assert_eq!(window.find("remote-filter").value(), Some("I"));
        window.press("shift-i", cx);
        ignored_menu(window, false, cx);
        window.press("menu", cx);
        let (index, item) = menu_item(window, "Show ignored");
        assert!(item.visible());
        window.within("popup-menu").click(index, cx);
        ignored_requested(window, true);
        window.click("remote-ignored", cx);
        ignored_requested(window, false);
        let s = shell.read(cx);
        let remote = s.remote.read(cx);
        assert!(remote.has_session());
        assert!(!remote.is_loading());
        assert_eq!(remote.session_id(), Some(session.id));
        assert_eq!(remote.connection(), connection);
        assert_eq!(remote.directory(), "/");
        assert!(remote.marked().is_empty());
        assert!(!s.comparison.read(cx).visible());
        assert!(!s.comparison.read(cx).is_loading());
        assert!(!s.preview.read(cx).is_loading());
        assert!(!s.remote_preview);
    })
    .unwrap();
    assert_eq!(session.state(), ConnectionState::Connected);
    assert_eq!(
        fs::read(server.dir.path().join("commands")).unwrap(),
        commands
    );
    for command in ["RETR", "STOR", "APPE", "DELE", "RMD", "MKD", "RNFR", "RNTO"] {
        assert_eq!(server.commands(command), 0, "unexpected {command}");
    }
    assert!(!local.path().join("cache.log").exists());
    assert_eq!(fs::read_to_string(remote_file).unwrap(), "remote");
}
