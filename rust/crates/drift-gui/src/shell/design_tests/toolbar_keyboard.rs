use super::*;

#[gpui_kit::test]
async fn focused_persistent_toolbar_keeps_browser_shortcuts_without_reaching_hidden_views(
    cx: &mut TestAppContext,
) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("keep.txt"), "unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.register("Review", local.path().into()).unwrap();
    let f = Fixture::new(1200., 900., store, local.path(), None, cx).await;
    for key in ["f5", "ctrl-f", "ctrl-escape"] {
        cx.update_window(f.handle, |_, w, cx| {
            for _ in 0..50 {
                w.focus_next(cx);
                w.render_frame(cx);
                if w.find("refresh").focused() == Some(true) {
                    break;
                }
            }
            assert_eq!(w.find("refresh").focused(), Some(true));
            let before = f.shell.read(cx).browser.read(cx).id();
            if key == "ctrl-escape" {
                let browser = f.shell.read(cx).browser.clone();
                browser.update(cx, |browser, cx| browser.refresh(&Refresh, w, cx));
                assert!(f.shell.read(cx).browser.read(cx).is_loading());
                assert_eq!(w.find("refresh").focused(), Some(true));
            }
            w.press(key, cx);
            match key {
                "f5" => {
                    assert!(f.shell.read(cx).browser.read(cx).id().operation > before.operation)
                }
                "ctrl-f" => {
                    assert_eq!(w.find("filter").focused(), Some(true));
                    assert_eq!(f.shell.read(cx).browser.read(cx).id(), before);
                }
                "ctrl-escape" => assert!(!f.shell.read(cx).browser.read(cx).is_loading()),
                _ => unreachable!(),
            }
        })
        .unwrap();
        cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
            !f.shell.read(cx).browser.read(cx).is_loading()
        })
        .await;
    }
    cx.update_window(f.handle, |_, w, cx| w.click("projects", cx))
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        for _ in 0..50 {
            w.focus_next(cx);
            w.render_frame(cx);
            if w.find("settings").focused() == Some(true) {
                break;
            }
        }
        assert_eq!(w.find("settings").focused(), Some(true));
        toolbar_scopes::assert_browser_scopes_absent(w);
        let before = f.shell.read(cx).browser.read(cx).id();
        for key in ["f5", "ctrl-f", "ctrl-escape"] {
            w.press(key, cx);
        }
        assert_eq!(f.shell.read(cx).browser.read(cx).id(), before);
        assert!(f.shell.read(cx).projects.read(cx).visible());
        assert!(f.shell.read(cx).hosts.is_none());
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(local.path().join("keep.txt")).unwrap(),
        "unchanged"
    );
}
