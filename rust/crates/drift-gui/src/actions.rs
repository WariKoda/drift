use gpui_kit::{App, KeyBinding};

gpui_kit::actions!(
    drift,
    [
        FocusFilter,
        CopySelection,
        Refresh,
        Cancel,
        GoUp,
        GoBack,
        GoForward,
        CursorUp,
        CursorDown,
        Activate
    ]
);
pub fn bind_keys(cx: &mut App) {
    crate::hosts::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("ctrl-f", FocusFilter, Some("Drift")),
        KeyBinding::new("cmd-f", FocusFilter, Some("Drift")),
        KeyBinding::new("ctrl-shift-c", CopySelection, Some("Drift")),
        KeyBinding::new("cmd-shift-c", CopySelection, Some("Drift")),
        KeyBinding::new("f5", Refresh, Some("Drift")),
        KeyBinding::new("escape", Cancel, Some("Drift")),
        KeyBinding::new("alt-left", GoBack, Some("DriftBrowser")),
        KeyBinding::new("alt-right", GoForward, Some("DriftBrowser")),
        KeyBinding::new("alt-up", GoUp, Some("DriftBrowser")),
        KeyBinding::new("backspace", GoUp, Some("DriftBrowser")),
        KeyBinding::new("left", GoUp, Some("DriftBrowser")),
        KeyBinding::new("h", GoUp, Some("DriftBrowser")),
        KeyBinding::new("up", CursorUp, Some("DriftBrowser")),
        KeyBinding::new("k", CursorUp, Some("DriftBrowser")),
        KeyBinding::new("down", CursorDown, Some("DriftBrowser")),
        KeyBinding::new("j", CursorDown, Some("DriftBrowser")),
        KeyBinding::new("enter", Activate, Some("DriftBrowser")),
        KeyBinding::new("right", Activate, Some("DriftBrowser")),
        KeyBinding::new("l", Activate, Some("DriftBrowser")),
    ]);
}
