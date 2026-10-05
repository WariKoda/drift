use super::*;
mod sync;
use gpui_kit::{ClipboardItem, EntityInputHandler as _, component::WindowExt as _};

#[gpui_kit::test]
async fn resize_escape_precedes_guarded_filter_composition_and_never_clears_marks_or_session(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let handle = fixture.handle;
    cx.update_window(handle, |_, w, cx| {
        let browser = fixture.shell.read(cx).browser.clone();
        browser.update(cx, |b, cx| b.focus(w, cx));
        w.press("home", cx);
        w.press("space", cx);
        w.click("filter", cx);
    })
    .unwrap();
    let input = cx
        .update_window(handle, |_, w, cx| {
            let focused = w.focused_input(cx).unwrap();
            let input = focused.as_input().unwrap().clone();
            input.update(cx, |state, cx| {
                state.set_value("", w, cx);
                state.replace_and_mark_text_in_range(Some(0..0), "keep", Some(4..4), w, cx);
            });
            input
        })
        .unwrap();
    cx.run_until_parked();
    let before = fixture.shell.read_with(cx, browser_state);
    assert!(
        !fixture
            .shell
            .read_with(cx, |s, cx| s.browser.read(cx).marked())
            .is_empty()
    );
    cx.update_window(handle, |_, w, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("private\nreplacement".into()));
        w.press(
            if cfg!(target_os = "macos") {
                "cmd-v"
            } else {
                "ctrl-v"
            },
            cx,
        );
        assert_eq!(input.read(cx).value(), "keep");
        assert_eq!(
            input.update(cx, |s, cx| s.marked_text_range(w, cx)),
            Some(0..4)
        );
        begin_drag(w, "browser-split", cx);
        w.press("escape", cx);
        assert_eq!(
            input.update(cx, |s, cx| s.marked_text_range(w, cx)),
            Some(0..4)
        );
        assert!(input.focus_handle(cx).is_focused(w));
        assert!(!fixture.shell.read(cx).split.is_resizing());
        w.press("escape", cx);
        assert_eq!(input.update(cx, |s, cx| s.marked_text_range(w, cx)), None);
        assert!(input.focus_handle(cx).is_focused(w));
        assert_eq!(input.read(cx).value(), "keep");
        w.press("escape", cx);
        assert!(!input.focus_handle(cx).is_focused(w));
    })
    .unwrap();
    assert_eq!(fixture.shell.read_with(cx, browser_state), before);
    assert!(
        fixture
            .shell
            .read_with(cx, |s, cx| s.remote.read(cx).has_session())
    );
}
