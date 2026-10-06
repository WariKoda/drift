use super::*;
mod covered;
use drift_core::gui_preferences::{
    GuiPreferences, PanePreferences, ThemePreference, WindowPreference,
};
use gpui_kit::component::{ActiveTheme, ThemeMode};

#[gpui_kit::test]
async fn native_theme_controls_preserve_browser_session_range_filter_and_preview(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("local-browser", cx);
        w.click("filter", cx);
        w.input("keep", cx);
        f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("end", cx);
        w.press("space", cx);
        w.press("v", cx);
        w.press("up", cx);
        assert!(f.shell.read(cx).browser.focus_handle(cx).is_focused(w));
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.press("p", cx);
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let preview = f.shell.read(cx).preview.clone();
        assert!(
            preview.update(cx, |preview, cx| preview.copy_text(cx)),
            "the real preview must be loaded before changing themes: {}",
            f.shell.read(cx).status
        );
        let before = browser_state(f.shell.read(cx), cx);
        let rows = row_positions(w, "local-files");
        for (id, mode) in [
            ("theme-dark", ThemeMode::Dark),
            ("theme-light", ThemeMode::Light),
        ] {
            w.click(id, cx);
            w.render_frame(cx);
            assert_eq!(cx.theme().mode, mode);
            assert_eq!(browser_state(f.shell.read(cx), cx), before);
            assert_eq!(row_positions(w, "local-files"), rows);
            assert_eq!(w.find("filter").value(), Some("keep"));
        }
        // Enter/Space on the native theme control cannot activate a file or sync.
        w.click("theme-dark", cx);
        w.click("filter", cx);
        for _ in 0..64 {
            w.press("shift-tab", cx);
            if w.find("theme-dark").focused() == Some(true) {
                break;
            }
        }
        assert_eq!(w.find("theme-dark").focused(), Some(true));
        w.press("enter", cx);
        w.press("space", cx);
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
        assert_eq!(browser_state(f.shell.read(cx), cx), before);
        w.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("local 58\n")
        );
        f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("v", cx);
        assert_eq!(
            f.shell.read(cx).browser.read(cx).marked(),
            ["keep-58.txt", "keep-59.txt"]
        );
    })
    .unwrap();
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
    let saved = Store::new(f._config.path().into())
        .gui_preferences()
        .unwrap();
    assert_eq!(saved.theme, ThemePreference::Dark);
    assert_eq!(saved.panes, PanePreferences::default());
}

#[gpui_kit::test]
async fn restored_splits_and_window_resize_save_independent_preferences_without_transfers(
    cx: &mut TestAppContext,
) {
    let initial = GuiPreferences {
        window: WindowPreference {
            width: 1400.,
            height: 900.,
            maximized: false,
        },
        panes: PanePreferences {
            browser: Some(0.72),
            comparison: Some(0.29),
        },
        ..Default::default()
    };
    let f = Fixture::build(cx, Some(initial)).await;
    let browser_ratio = cx
        .update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            let width = w.find("browser-split").bounds().size.width;
            assert!((first_width(w, "browser-split") / width - 0.72).abs() < 0.001);
            drag(w, "browser-split", -80., cx);
            let ratio = first_width(w, "browser-split") / width;
            w.click("compare-project", cx);
            ratio
        })
        .unwrap();
    f.idle(cx).await;
    let comparison_ratio = cx
        .update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            let width = w.find("comparison-split").bounds().size.width;
            assert!((first_width(w, "comparison-split") / width - 0.29).abs() < 0.001);
            drag(w, "comparison-split", 70., cx);
            let ratio = first_width(w, "comparison-split") / width;
            w.click("comparison-back", cx);
            ratio
        })
        .unwrap();
    cx.simulate_window_resize(f.handle, size(px(1300.), px(800.)));
    cx.wait_for(f.handle, Duration::from_secs(60), |_, cx| {
        f.shell
            .read(cx)
            .preferences
            .as_ref()
            .unwrap()
            .read(cx)
            .preferred_window()
            == WindowPreference {
                width: 1300.,
                height: 800.,
                maximized: false,
            }
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!f.shell.read(cx).comparison.read(cx).visible());
        assert!(f.shell.read(cx).remote.read(cx).has_session());
        assert_eq!(
            fs::read_to_string(f.server.dir.path().join("files/keep-00.txt")).unwrap(),
            "remote 0\n"
        );
    })
    .unwrap();
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
    let saved = Store::new(f._config.path().into())
        .gui_preferences()
        .unwrap();
    assert!((saved.panes.browser.unwrap() - browser_ratio).abs() < 0.001);
    assert!((saved.panes.comparison.unwrap() - comparison_ratio).abs() < 0.001);
    assert_eq!(
        saved.window,
        WindowPreference {
            width: 1300.,
            height: 800.,
            maximized: false
        }
    );
}

#[gpui_kit::test]
async fn theme_changes_preserve_diff_copy_and_pending_confirmation_and_old_help_scene_cannot_choose(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    cx.update_window(f.handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let pane = f.shell.read(cx).comparison.clone();
        pane.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.click("comparison-action", cx);
        w.click(("diff-row", 0usize), cx);
        w.press("ctrl-c", cx);
        let copy = cx.read_from_clipboard().unwrap().text();
        pane.focus_handle(cx).focus(w, cx);
        w.press("shift-s", cx);
        w.render_frame(cx);
        assert!(w.find("sync-confirm").visible());
        let entity = pane.entity_id();
        w.click("theme-dark", cx);
        w.render_frame(cx);
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
        assert_eq!(pane.entity_id(), entity);
        assert!(w.find("sync-confirm").visible());
        assert!(!pane.read(cx).is_loading());
        assert!(pane.read(cx).sync_result_for_test().is_none());
        w.click("copy-diff", cx);
        assert_eq!(cx.read_from_clipboard().unwrap().text(), copy);
        let light_position = w.find("theme-light").bounds().center();
        f.shell.update(cx, |s, cx| s.show_help(&ShowHelp, w, cx));
        let focus = w.focused(cx);
        for input in [
            MouseDownEvent {
                button: MouseButton::Left,
                position: light_position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            MouseUpEvent {
                button: MouseButton::Left,
                position: light_position,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
        ] {
            w.dispatch_event(input, cx);
        }
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
        assert_eq!(w.focused(cx), focus);
        assert!(f.shell.read(cx).help.is_some());
        assert!(!pane.read(cx).is_loading());
        assert!(pane.read(cx).sync_result_for_test().is_none());
        assert_eq!(
            fs::read_to_string(f.local.path().join("keep-00.txt")).unwrap(),
            "local 0\n"
        );
        assert_eq!(
            fs::read_to_string(f.server.dir.path().join("files/keep-00.txt")).unwrap(),
            "remote 0\n"
        );
    })
    .unwrap();
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
    assert_eq!(
        Store::new(f._config.path().into())
            .gui_preferences()
            .unwrap()
            .theme,
        ThemePreference::Dark
    );
}
