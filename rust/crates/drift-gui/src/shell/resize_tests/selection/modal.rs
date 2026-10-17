use super::*;
use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};

#[gpui_kit::test]
async fn text_selection_covering_routes_reject_prior_frame_pointer_without_stealing_focus(
    cx: &mut TestAppContext,
) {
    let (fixture, diff) = selected_diff(cx).await;
    let before = diff.read_with(cx, |pane, _| pane.snapshot());
    for route in 0..4 {
        cx.update_window(fixture.handle, |_, w, cx| {
            w.render_frame(cx);
            diff.focus_handle(cx).focus(w, cx);
            let bounds = w
                .find(("diff-row", before.selection.unwrap().anchor.source))
                .bounds();
            let position = point(bounds.origin.x + px(140.), bounds.center().y);
            let copy_button = w.find("copy-diff").bounds().center();
            w.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: copy_button,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            fixture.shell.update(cx, |shell, cx| match route {
                0 => shell.show_help(&ShowHelp, w, cx),
                1 => shell.toolbar_event(&ToolbarEvent::Projects, w, cx),
                2 => shell.open_hosts(w, cx),
                _ => shell
                    .comparison
                    .update(cx, |pane, cx| pane.cancel(&Cancel, w, cx)),
            });
            let focus = w.focused(cx);
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("sentinel".into()));
            let offset = state(&diff, cx).scroll.0.borrow().base_handle.offset();
            // Deliberately use the old dispatch tree, without drawing the covering route.
            w.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position: copy_button,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            w.dispatch_event(
                gpui_kit::ScrollWheelEvent {
                    position,
                    delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-120.))),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            w.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            w.dispatch_event(
                MouseMoveEvent {
                    position: position + point(px(80.), px(25.)),
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            w.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            assert_eq!(
                w.focused(cx),
                focus,
                "route {route} must retain its current input owner"
            );
            let after = state(&diff, cx);
            assert_eq!(after.selection, before.selection);
            assert_eq!(after.expanded, before.expanded);
            assert_eq!(after.scroll.0.borrow().base_handle.offset(), offset);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("sentinel")
            );
            w.render_frame(cx);
            if route < 3 {
                w.press("escape", cx);
            }
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(fixture.handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    assert_eq!(
        fs::read_to_string(fixture.local.path().join("keep-00.txt")).unwrap(),
        "before\n".repeat(60) + "local\n" + &"tail\n".repeat(60)
    );
    assert_eq!(
        fs::read_to_string(fixture.server.dir.path().join("files/keep-00.txt")).unwrap(),
        "before\n".repeat(60) + "remote\n" + &"tail\n".repeat(60)
    );
}
