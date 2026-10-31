//! Shared native-control interactions for headless GUI integration tests.
use gpui_kit::{
    AnyWindowHandle, App, AppContext, InputEvent as _, KeyDownEvent, KeyUpEvent, Keystroke,
    TestAppContext, Window, test::TestWindowExt,
};

pub(crate) fn enter_without_repaint(window: &mut Window, cx: &mut App) {
    window.dispatch_event(
        KeyDownEvent {
            keystroke: Keystroke::parse("enter").unwrap(),
            is_held: false,
            prefer_character_input: false,
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        KeyUpEvent {
            keystroke: Keystroke::parse("enter").unwrap(),
        }
        .to_platform_input(),
        cx,
    );
}

pub(crate) fn settle(window: &mut Window, cx: &mut App) {
    for _ in 0..3 {
        window.render_frame(cx);
    }
}

/// Open the real picker and move from its committed cursor before confirming.
/// `None` means the picker has no committed selection (Down selects its first item).
pub(crate) fn select(
    window: &mut Window,
    id: &'static str,
    current: Option<usize>,
    target: usize,
    cx: &mut App,
) {
    settle(window, cx);
    window.click(id, cx);
    assert_eq!(
        window.find(id).expanded(),
        Some(true),
        "picker {id} did not open: {:?}",
        window.find(id)
    );
    match current {
        Some(current) => {
            let key = if target < current { "up" } else { "down" };
            for _ in 0..target.abs_diff(current) {
                window.press(key, cx);
            }
        }
        None => {
            for _ in 0..=target {
                window.press("down", cx);
            }
        }
    }
    window.press("enter", cx);
    // Native Select commits in defer_in, after the enclosing window update returns.
}

pub(crate) fn theme(handle: impl Into<AnyWindowHandle>, choice: &str, cx: &mut TestAppContext) {
    const CHOICES: [&str; 4] = ["System", "Omarchy", "Dark", "Light"];
    let handle = handle.into();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let control = window.find("theme-select");
        let current = CHOICES
            .iter()
            .position(|choice| Some(*choice) == control.value());
        let target = CHOICES.iter().position(|item| *item == choice).unwrap();
        assert!(
            current.is_some(),
            "unknown theme value: {:?}",
            control.value()
        );
        select(window, "theme-select", current, target, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("theme-select").expanded(), Some(false));
        assert_eq!(window.find("theme-select").value(), Some(choice));
    })
    .unwrap();
}

pub(crate) fn connect_first(window: &mut Window, cx: &mut App) {
    select(window, "remote-host-select", None, 0, cx);
}
