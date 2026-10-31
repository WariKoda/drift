use super::*;
use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, NavigationDirection};

#[gpui_kit::test]
async fn path_header_is_inside_the_local_pane_and_keeps_keyboard_and_mouse_navigation(
    cx: &mut TestAppContext,
) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let deep = local.path().join("child/deep");
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("file.txt"), "unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.register("Pane review", local.path().into()).unwrap();
    let f = Fixture::new(1200., 900., store, &deep, None, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.press("home", cx);
        w.press("space", cx);
        w.press("ctrl-f", cx);
        w.input("file", cx);
        let browser = f.shell.read(cx).browser.clone();
        let preview = f.shell.read(cx).local_preview.clone();
        let id = browser.read(cx).id();
        let marks = browser.read(cx).marked();
        let focus = w.focused(cx);
        for (width, font, mode) in [
            (1200., 16., ThemeMode::Dark),
            (420., 16., ThemeMode::Light),
            (420., 24., ThemeMode::Dark),
        ] {
            w.resize(size(px(width), px(900.)));
            w.bounds_changed(cx);
            Theme::change(mode, Some(w), cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            for _ in 0..3 {
                w.render_frame(cx);
            }
            let pane = w.find("local-workspace").bounds();
            let header = w
                .within("local-workspace")
                .find("local-path-header")
                .bounds();
            let path = w
                .within("local-workspace")
                .find("local-breadcrumbs")
                .bounds();
            assert!(
                w.within("browser-pane")
                    .try_find("local-path-header")
                    .is_none()
            );
            let files = w.find("local-files-pane").bounds();
            assert!(header.left() >= pane.left() && header.right() <= pane.right());
            assert!(header.top() >= pane.top());
            assert_eq!(header.bottom(), files.top());
            assert!(path.left() >= header.left() && path.right() <= header.right());
            assert!(path.top() >= header.top() && path.bottom() <= header.bottom());
            assert!(
                w.within("local-browser-header")
                    .try_find("local-breadcrumbs")
                    .is_none()
            );
            assert!(header.size.height < pane.size.height / 2.);
            assert_eq!(f.shell.read(cx).browser, browser);
            assert_eq!(f.shell.read(cx).local_preview, preview);
            assert_eq!(browser.read(cx).id(), id);
            assert_eq!(browser.read(cx).marked(), marks);
            assert_eq!(w.find("filter").value(), Some("file"));
            assert_eq!(w.focused(cx), focus);
        }
        w.resize(size(px(1200.), px(900.)));
        w.bounds_changed(cx);
        Theme::update(cx, |theme| theme.font_size = px(16.));
        w.render_frame(cx);
        for _ in 0..50 {
            w.focus_next(cx);
            w.render_frame(cx);
            if w.find(("local-crumb", 0usize)).focused() == Some(true) {
                break;
            }
        }
        assert_eq!(w.find(("local-crumb", 0usize)).focused(), Some(true));
        w.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    assert_eq!(
        f.shell.read_with(cx, |s, cx| s
            .browser
            .read(cx)
            .location()
            .unwrap()
            .directory
            .clone()),
        local.path()
    );
    for (direction, expected) in [
        (NavigationDirection::Back, deep.clone()),
        (NavigationDirection::Forward, local.path().to_path_buf()),
    ] {
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            let position = w.find("local-path-header").bounds().center();
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
        })
        .await;
        assert_eq!(
            f.shell.read_with(cx, |s, cx| s
                .browser
                .read(cx)
                .location()
                .unwrap()
                .directory
                .clone()),
            expected
        );
    }
    assert_eq!(
        fs::read_to_string(deep.join("file.txt")).unwrap(),
        "unchanged"
    );
}

#[gpui_kit::test]
async fn home_returns_from_management_and_finder_without_resetting_the_browser(
    cx: &mut TestAppContext,
) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let deep = local.path().join("child/deep");
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("file.txt"), "unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.register("Home review", local.path().into()).unwrap();
    let f = Fixture::new(1200., 900., store, &deep, None, cx).await;
    let (browser, id, marks) = cx
        .update_window(f.handle, |_, w, cx| {
            w.press("home", cx);
            w.press("space", cx);
            let browser = f.shell.read(cx).browser.clone();
            let id = browser.read(cx).id();
            let marks = browser.read(cx).marked();
            w.click("projects", cx);
            (browser, id, marks)
        })
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("home", cx);
        assert!(f.shell.read(cx).browser_screen_active(cx));
        assert_eq!(browser.read(cx).id(), id);
        assert_eq!(browser.read(cx).location().unwrap().directory, deep);
        w.click("hosts", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell
            .read(cx)
            .hosts
            .as_ref()
            .is_some_and(|h| h.read(cx).can_leave())
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("home", cx);
        assert!(f.shell.read(cx).browser_screen_active(cx));
        assert_eq!(f.shell.read(cx).browser, browser);
        assert_eq!(browser.read(cx).id(), id);
        assert_eq!(browser.read(cx).marked(), marks);
        w.click("find", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        browser.read(cx).finder_active() && !browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("home", cx);
        assert!(!browser.read(cx).finder_active());
        assert_eq!(browser.read(cx).location().unwrap().directory, deep);
        assert_eq!(browser.read(cx).marked(), marks);
        assert!(browser.focus_handle(cx).is_focused(w));
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(deep.join("file.txt")).unwrap(),
        "unchanged"
    );
}
