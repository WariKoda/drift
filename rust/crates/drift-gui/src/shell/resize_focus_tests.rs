use super::*;
use crate::actions::bind_keys;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    Bounds, InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, TestAppContext,
    WindowBounds, WindowOptions, point, px, size,
};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn escape_from_the_header_filter_cancels_resize_not_the_live_preview(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("a.txt"), "preview survives resizing\n").unwrap();
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1100.), px(800.)),
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
                        Store::new(config.path().into()),
                        service.clone(),
                        RemoteService::new(service),
                        local.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
            && !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("space", cx);
        w.click("filter", cx);
        w.input("a", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let browser_id = shell.read(cx).browser.read(cx).id();
        let marks = shell.read(cx).browser.read(cx).marked();
        let status = shell.read(cx).status.clone();
        let location = shell.read(cx).browser.read(cx).location().unwrap().clone();
        let preview = shell.read(cx).preview.clone();
        preview.update(cx, |pane, cx| {
            pane.show(location, "a.txt".into(), browser_id.project, w, cx)
        });
        let preview_id = preview.read(cx).id();
        assert!(preview.read(cx).is_loading());
        w.render_frame(cx);
        let focus = w.focused(cx);
        let from = w
            .within("browser-split")
            .find("pane-divider")
            .bounds()
            .center();
        w.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        w.render_frame(cx);
        w.dispatch_event(
            MouseMoveEvent {
                position: point(from.x + px(80.), from.y),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        w.render_frame(cx);
        assert!(cx.has_active_drag());
        assert_eq!(w.focused(cx), focus);
        w.press("escape", cx);
        assert!(!cx.has_active_drag());
        assert_eq!(w.focused(cx), focus);
        assert_eq!(w.find("filter").value(), Some("a"));
        assert_eq!(shell.read(cx).browser.read(cx).id(), browser_id);
        assert_eq!(shell.read(cx).browser.read(cx).marked(), marks);
        assert_eq!(preview.read(cx).id(), preview_id);
        assert!(preview.read(cx).is_loading());
        assert_eq!(shell.read(cx).status, status);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !shell.read(cx).preview.read(cx).is_loading()
    })
    .await;
}
