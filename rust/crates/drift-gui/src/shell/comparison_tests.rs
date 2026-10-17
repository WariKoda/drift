use super::*;
use crate::{actions::bind_keys, sftp_test_support as support};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{
    fs,
    time::{Duration, UNIX_EPOCH},
};

#[gpui_kit::test]
async fn real_comparison_direction_folding_refresh_and_scope_keep_the_browser(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let before = "before\n".repeat(12);
    let after = "after\n".repeat(12);
    let local_text = before.clone() + "local\n" + &after;
    let remote_text = before + "remote\n" + &after;
    fs::write(local.path().join("changed.txt"), local_text).unwrap();
    let remote_path = server.dir.path().join("files/changed.txt");
    fs::write(&remote_path, remote_text).unwrap();
    fs::File::open(local.path().join("changed.txt"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(100)))
        .unwrap();
    fs::File::open(&remote_path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(110)))
        .unwrap();
    fs::write(local.path().join("only.txt"), "local only").unwrap();
    fs::write(local.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(server.dir.path().join("files/.gitignore"), "ignored.txt\n").unwrap();
    fs::write(
        server.dir.path().join("files/ignored.txt"),
        "ignored remote",
    )
    .unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1400.), px(900.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    let service = BrowserService::new().unwrap();
                    Shell::new(
                        w,
                        cx,
                        store,
                        service.clone(),
                        RemoteService::with_options(service, server.options()),
                        local.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.click("remote", cx);
        w.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    assert!(shell.read_with(cx, |s, cx| s.comparison.read(cx).visible()));
    assert_eq!(
        shell.read_with(cx, |s, cx| s
            .comparison
            .read(cx)
            .session
            .as_ref()
            .unwrap()
            .entries
            .len()),
        2
    );
    cx.update_window(handle, |_, w, cx| {
        w.press("shift-a", cx); // Cycle every valid decision, including other files.
        w.press("end", cx);
        w.click("copy-diff", cx);
        assert!(
            cx.read_from_clipboard()
                .unwrap()
                .text()
                .unwrap()
                .contains("- local only")
        );
        w.press("shift-a", cx);
        w.press("shift-a", cx); // Three steps restore both two-sided and local-only decisions.
        w.press("home", cx);
        for key in ["end", "shift-g", "n"] {
            w.press(key, cx);
            w.click("copy-diff", cx);
            assert!(
                cx.read_from_clipboard()
                    .unwrap()
                    .text()
                    .unwrap()
                    .contains("local only")
            );
        }
        w.press("tab", cx); // List -> diff; file navigation bubbles to the comparison.
        w.press("p", cx);
        w.press("shift-tab", cx);
        assert!(shell.read(cx).comparison.focus_handle(cx).is_focused(w));
        w.press("home", cx);
        w.press("g", cx);
        w.click("copy-diff", cx);
        let copy = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(copy.contains("- local") && copy.contains("+ remote"));
        assert!(copy.contains("9 unchanged lines"));
        w.click("comparison-action", cx); // Download -> Skip
        w.click("comparison-action", cx); // Skip -> Upload
        w.click("copy-diff", cx);
        let copy = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(copy.contains("- remote") && copy.contains("+ local"));
        w.click(("diff-row", 0usize), cx); // Expand leading equal run.
        w.click("copy-diff", cx);
        let copy = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(copy.starts_with("@@") && copy.contains("before"));
        w.click(("diff-row", 2usize), cx);
        w.press("ctrl-c", cx);
        let copy = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(copy.contains("before") && !copy.contains("@@") && !copy.contains("local"));
        w.press("i", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    assert_eq!(
        shell.read_with(cx, |s, cx| s
            .comparison
            .read(cx)
            .session
            .as_ref()
            .unwrap()
            .entries
            .len()),
        3
    );
    fs::write(server.dir.path().join("files/new.txt"), "new remote").unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("comparison-filter", cx);
        w.input("jknprudissgGA", cx); // Navigation, refresh and sync leave text inputs alone.
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("copy-diff", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap_or_default(),
            ""
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| w.press("f5", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    assert!(shell.read_with(cx, |s, cx| {
        s.comparison
            .read(cx)
            .session
            .as_ref()
            .unwrap()
            .request
            .scope
            .include_ignored
    }));
    assert_eq!(
        shell.read_with(cx, |s, cx| s
            .comparison
            .read(cx)
            .session
            .as_ref()
            .unwrap()
            .entries
            .len()),
        4
    );
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).comparison.focus_handle(cx).focus(w, cx);
        w.press("shift-s", cx);
        w.press("escape", cx);
        w.press("ctrl-enter", cx); // Cancelled confirmation must never start sync.
        assert!(shell.read(cx).comparison.read(cx).visible());
        assert!(!shell.read(cx).comparison.read(cx).is_loading());
    })
    .unwrap();
    assert!(!local.path().join("ignored.txt").exists());
    cx.update_window(handle, |_, w, cx| {
        w.press("shift-s", cx);
        w.press("ctrl-enter", cx);
        w.click("comparison-progress", cx); // Keep transfers running while hidden.
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
            && !shell.read(cx).browser.read(cx).is_loading()
            && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    shell.read_with(cx, |s, cx| {
        let pane = s.comparison.read(cx);
        let report = pane.sync_result_for_test().unwrap();
        assert!(report.stopped.is_none());
        assert_eq!(
            report
                .outcomes
                .iter()
                .filter(|o| **o == drift_app::sync::ItemOutcome::Completed)
                .count(),
            4
        );
        assert!(pane.session.as_ref().unwrap().entries.is_empty());
        assert!(pane.session.as_ref().unwrap().request.scope.include_ignored);
    });
    assert_eq!(
        fs::read(local.path().join("ignored.txt")).unwrap(),
        b"ignored remote"
    );
    assert_eq!(
        fs::read(local.path().join("new.txt")).unwrap(),
        b"new remote"
    );
    assert_eq!(
        fs::read(server.dir.path().join("files/only.txt")).unwrap(),
        b"local only"
    );
    assert_eq!(
        fs::read(local.path().join("changed.txt")).unwrap(),
        fs::read(&remote_path).unwrap()
    );
    cx.update_window(handle, |_, w, cx| w.click("comparison-back", cx))
        .unwrap();
    assert!(!shell.read_with(cx, |s, cx| s.comparison.read(cx).visible()));
    assert!(shell.read_with(cx, |s, cx| s.remote.read(cx).has_session()));
    assert_eq!(
        shell.read_with(cx, |s, cx| s
            .browser
            .read(cx)
            .location()
            .unwrap()
            .directory
            .clone()),
        local.path()
    );
    cx.update_window(handle, |_, w, cx| {
        w.click("compare-project", cx);
        w.click("comparison-progress", cx); // Hiding progress does not cancel.
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    assert!(shell.read_with(cx, |s, cx| s.remote.read(cx).has_session()));
    let other = tempfile::tempdir().unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("comparison-refresh", cx);
        assert!(shell.read(cx).comparison.read(cx).is_loading());
        shell.update(cx, |s, cx| s.open_project(other.path().into(), w, cx))
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    assert!(!shell.read_with(cx, |s, cx| s.comparison.read(cx).visible()));
    assert!(!shell.read_with(cx, |s, cx| s.remote.read(cx).has_session()));
}

#[gpui_kit::test]
async fn marked_local_and_remote_scope_survives_refresh_and_sync(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    fs::create_dir(local.path().join("folder")).unwrap();
    fs::write(local.path().join("folder/child.txt"), "child").unwrap();
    fs::write(
        local.path().join("folder/.gitignore"),
        "ignored-child.txt\n",
    )
    .unwrap();
    fs::write(
        local.path().join("folder/ignored-child.txt"),
        "excluded child",
    )
    .unwrap();
    fs::write(local.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(local.path().join("ignored.txt"), "direct ignored selection").unwrap();
    fs::write(local.path().join("unselected.txt"), "leave local").unwrap();
    fs::write(remote.join("remote.txt"), "download remote").unwrap();
    fs::write(remote.join("unselected.txt"), "leave remote").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1400.), px(900.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    let service = BrowserService::new().unwrap();
                    Shell::new(
                        w,
                        cx,
                        store,
                        service.clone(),
                        RemoteService::with_options(service, server.options()),
                        local.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.click("remote", cx);
        w.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;

    // Reveal ignored files and mark a directory plus one explicit ignored file.
    cx.update_window(handle, |_, w, cx| {
        let browser = shell.read(cx).browser.clone();
        browser.update(cx, |pane, cx| pane.command(BrowserCommand::Ignored, w, cx));
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("down", cx);
        w.press("space", cx);
        w.press("right", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.press("right", cx);
        w.press("space", cx);
        w.press("left", cx); // The marked child remains in comparison scope while collapsed.
        w.press("down", cx);
        w.press("space", cx);
        assert_eq!(
            shell.read(cx).browser.read(cx).marked(),
            ["folder", "folder/child.txt", "ignored.txt"]
        );
        w.click("remote", cx);
        w.press("down", cx);
        w.press("space", cx);
        assert_eq!(
            shell.read(cx).remote.read(cx).marked(),
            [remote.join("remote.txt").to_string_lossy().into_owned()]
        );
        w.press("s", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    shell.read_with(cx, |s, cx| {
        let comparison = s.comparison.read(cx);
        let session = comparison.session.as_ref().unwrap();
        assert_eq!(
            session.request.local,
            [
                PathBuf::from("folder"),
                PathBuf::from("folder/child.txt"),
                PathBuf::from("ignored.txt")
            ]
        );
        assert_eq!(session.request.remote.len(), 1);
        let names: Vec<_> = session
            .entries
            .iter()
            .map(|entry| {
                entry
                    .local
                    .strip_prefix(local.path())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(names.contains(&"ignored.txt".into()));
        assert!(names.contains(&"folder/child.txt".into()));
        assert!(names.contains(&"remote.txt".into()));
        assert!(!names.contains(&"unselected.txt".into()));
        assert!(!names.contains(&"folder/ignored-child.txt".into()));
    });
    // An unrelated file created later must not broaden refresh or post-sync scope.
    fs::write(remote.join("later.txt"), "not selected").unwrap();
    cx.update_window(handle, |_, w, cx| w.press("f5", cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.click("sync-all", cx);
        w.click("sync-confirm", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).comparison.read(cx).is_loading()
            && !shell.read(cx).browser.read(cx).is_loading()
            && !shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    shell.read_with(cx, |s, cx| {
        let session = s.comparison.read(cx).session.as_ref().unwrap();
        assert!(session.entries.is_empty());
        assert_eq!(session.request.local.len(), 3);
        assert_eq!(session.request.remote.len(), 1);
        assert_eq!(
            s.browser.read(cx).marked(),
            ["folder", "folder/child.txt", "ignored.txt"]
        );
        assert_eq!(s.remote.read(cx).marked().len(), 1);
    });
    assert_eq!(
        fs::read(remote.join("ignored.txt")).unwrap(),
        b"direct ignored selection"
    );
    assert_eq!(
        fs::read(local.path().join("remote.txt")).unwrap(),
        b"download remote"
    );
    assert!(!local.path().join("later.txt").exists());
    assert!(!remote.join("folder/ignored-child.txt").exists());
    assert_eq!(
        fs::read(remote.join("unselected.txt")).unwrap(),
        b"leave remote"
    );
}
