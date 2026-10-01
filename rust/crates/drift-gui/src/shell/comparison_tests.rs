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
                        local.path().into(),
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
        w.click("comparison-ignored", cx);
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
        w.input("jk", cx); // Letter navigation must leave this text input alone.
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
