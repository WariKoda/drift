use super::*;
use crate::actions::bind_keys;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{TestAppContext, WindowOptions};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn finder_return_from_shell_filter_respects_popups_help_and_live_marks(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(root.path().join("alpha.txt"), "alpha").unwrap();
    fs::write(root.path().join("beta.txt"), "beta").unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                Shell::new(
                    window,
                    cx,
                    Store::new(config.path().into()),
                    BrowserService::new().unwrap(),
                    RemoteService::new(BrowserService::new().unwrap()),
                    root.path().to_path_buf(),
                )
            })
        })
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
            && !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.press("home", cx);
        w.press("/", cx);
        w.input("alpha", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("enter", cx);
        w.press("home", cx);
        w.press("space", cx);
        w.press("f", cx);
        w.input("bt", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).browser.read(cx).finder_active()
            && !shell.read(cx).browser.read(cx).is_loading()
            && shell.read(cx).browser.read(cx).len() == 1
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.press("enter", cx);
        w.press("home", cx);
        assert_eq!(shell.read(cx).browser.read(cx).selected(), Some("beta.txt"));
        w.press("space", cx);
        w.press("menu", cx);
        w.press("ctrl-alt-f", cx);
        assert!(shell.read(cx).browser.read(cx).finder_active());
        assert!(w.find("popup-menu").visible());
        w.press("escape", cx);
        w.press("f1", cx);
        w.press("ctrl-alt-f", cx);
        assert!(shell.read(cx).help.is_some());
        assert!(shell.read(cx).browser.read(cx).finder_active());
        w.press("escape", cx);
        w.press("ctrl-f", cx);
        w.press(
            if cfg!(target_os = "macos") {
                "cmd-alt-f"
            } else {
                "ctrl-alt-f"
            },
            cx,
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let browser = shell.read(cx).browser.read(cx);
        assert!(!browser.finder_active());
        assert_eq!(browser.len(), 1);
        assert_eq!(browser.selected(), Some("alpha.txt"));
        assert_eq!(browser.marked(), ["alpha.txt", "beta.txt"]);
        assert!(browser.focus_handle(cx).is_focused(w));
        assert!(!shell.read(cx).comparison.read(cx).visible());
        assert!(!shell.read(cx).preview.read(cx).is_loading());
        assert!(!shell.read(cx).remote.read(cx).has_session());
    })
    .unwrap();
}

#[cfg(unix)]
#[gpui_kit::test]
async fn finder_failure_under_hosts_does_not_focus_a_hidden_browser(cx: &mut TestAppContext) {
    use std::os::unix::ffi::OsStringExt;
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(root.path().join("kept.txt"), "kept").unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                Shell::new(
                    window,
                    cx,
                    Store::new(config.path().into()),
                    BrowserService::new().unwrap(),
                    RemoteService::new(BrowserService::new().unwrap()),
                    root.path().to_path_buf(),
                )
            })
        })
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
            && !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    fs::write(
        root.path().join(std::ffi::OsString::from_vec(vec![0xff])),
        "undisplayable",
    )
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .browser
                .update(cx, |pane, cx| pane.command(BrowserCommand::Find, w, cx));
            shell.open_hosts(w, cx);
        });
        assert!(shell.read(cx).hosts.is_some());
        w.press("ctrl-f", cx);
        w.input("before ", cx);
        w.press("f1", cx);
        assert!(shell.read(cx).help.is_some());
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert!(!shell.read(cx).browser.read(cx).finder_active());
        assert!(!shell.read(cx).browser.focus_handle(cx).is_focused(w));
        assert!(shell.read(cx).status.contains("not UTF-8"));
        assert!(shell.read(cx).help.is_some());
        w.press("escape", cx);
        assert_eq!(w.find("hosts-filter").focused(), Some(true));
        w.input("focus retained", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(
            w.find("hosts-filter").value(),
            Some("before focus retained")
        );
        assert_eq!(w.find("hosts-filter").focused(), Some(true));
    })
    .unwrap();
}

#[cfg(unix)]
#[gpui_kit::test]
async fn finder_failure_under_help_does_not_restore_a_disappeared_return_button(
    cx: &mut TestAppContext,
) {
    use std::os::unix::ffi::OsStringExt;
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(root.path().join("kept.txt"), "kept").unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                Shell::new(
                    window,
                    cx,
                    Store::new(config.path().into()),
                    BrowserService::new().unwrap(),
                    RemoteService::new(BrowserService::new().unwrap()),
                    root.path().to_path_buf(),
                )
            })
        })
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
            && !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    fs::write(
        root.path().join(std::ffi::OsString::from_vec(vec![0xff])),
        "undisplayable",
    )
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .browser
                .update(cx, |pane, cx| pane.command(BrowserCommand::Find, w, cx))
        });
        for _ in 0..40 {
            w.press("shift-tab", cx);
            if w.find("finder-return").focused() == Some(true) {
                break;
            }
        }
        assert_eq!(w.find("finder-return").focused(), Some(true));
        w.press("f1", cx);
        assert!(shell.read(cx).help.is_some());
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert!(!shell.read(cx).browser.read(cx).finder_active());
        assert!(shell.read(cx).help.is_some());
        w.press("escape", cx);
        assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
        w.press("home", cx);
        assert_eq!(shell.read(cx).browser.read(cx).selected(), Some("kept.txt"));
        w.press("space", cx);
        assert_eq!(shell.read(cx).browser.read(cx).marked(), ["kept.txt"]);
        assert!(!shell.read(cx).comparison.read(cx).visible());
        assert!(!shell.read(cx).remote.read(cx).has_session());
    })
    .unwrap();
}
