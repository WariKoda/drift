use super::*;
mod covered;
mod settings;
mod warnings;
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
        w.click("local-files-tab", cx);
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
    let (before, rows) = cx
        .update_window(f.handle, |_, w, cx| {
            let preview = f.shell.read(cx).local_preview.clone();
            assert!(
                preview.update(cx, |preview, cx| preview.copy_text(cx)),
                "the real preview must be loaded before changing themes: {}",
                f.shell.read(cx).status
            );
            w.click("local-files-tab", cx);
            // The measured header slot shrinks from the preview controls one frame later.
            crate::test_support::settle(w, cx);
            let before = browser_state(f.shell.read(cx), cx);
            let rows = row_positions(w, "local-files");
            assert!(w.try_find("gui-preferences").is_none());
            (before, rows)
        })
        .unwrap();
    for (choice, mode) in [("Dark", ThemeMode::Dark), ("Light", ThemeMode::Light)] {
        cx.update_window(f.handle, |_, w, cx| {
            w.click("settings", cx);
            assert!(w.within("settings-view").find("gui-preferences").visible());
        })
        .unwrap();
        crate::test_support::theme(f.handle, choice, cx);
        cx.update_window(f.handle, |_, w, cx| {
            w.click("settings-close", cx);
            w.render_frame(cx);
            assert!(w.try_find("gui-preferences").is_none());
            assert_eq!(cx.theme().mode, mode);
            assert_eq!(browser_state(f.shell.read(cx), cx), before);
            assert_eq!(row_positions(w, "local-files"), rows);
            assert_eq!(w.find("filter").value(), Some("keep"));
        })
        .unwrap();
    }
    cx.update_window(f.handle, |_, w, cx| {
        // Enter/Space on the native theme control cannot activate a hidden file or sync.
        w.click("settings", cx);
        for _ in 0..64 {
            w.press("tab", cx);
            if w.find("theme-select").focused() == Some(true) {
                break;
            }
        }
        assert_eq!(w.find("theme-select").focused(), Some(true));
        w.press("enter", cx);
        assert_eq!(w.find("theme-select").expanded(), Some(true));
        w.press("space", cx);
        assert_eq!(cx.theme().mode, ThemeMode::Light);
        w.press("up", cx);
        w.press("enter", cx);
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(w.find("theme-select").value(), Some("Dark"));
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
        assert_eq!(browser_state(f.shell.read(cx), cx), before);
        w.click("settings-close", cx);
        assert!(w.try_find("theme-select").is_none());
        w.click("local-preview-tab", cx);
        w.within("local-workspace").click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("local 58\n")
        );
        w.click("local-files-tab", cx);
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
async fn settings_theme_change_preserves_diff_and_pending_confirmation_blocks_settings(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    cx.update_window(f.handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    f.idle(cx).await;
    let (pane, copy, entity) = cx
        .update_window(f.handle, |_, w, cx| {
            let pane = f.shell.read(cx).comparison.clone();
            pane.focus_handle(cx).focus(w, cx);
            w.press("home", cx);
            w.click("comparison-action", cx);
            w.click(("diff-row", 0usize), cx);
            w.press("ctrl-c", cx);
            let copy = cx.read_from_clipboard().unwrap().text();
            let entity = pane.entity_id();
            w.click("settings", cx);
            (pane, copy, entity)
        })
        .unwrap();
    crate::test_support::theme(f.handle, "Dark", cx);
    cx.update_window(f.handle, |_, w, cx| {
        w.click("settings-close", cx);
        w.render_frame(cx);
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
        assert_eq!(pane.entity_id(), entity);
        assert!(w.try_find("gui-preferences").is_none());
        pane.focus_handle(cx).focus(w, cx);
        w.press("shift-s", cx);
        w.render_frame(cx);
        assert!(w.find("sync-confirm").visible());
        assert!(
            !f.shell
                .read(cx)
                .toolbar_state(cx)
                .allows(&ToolbarEvent::Settings)
        );
        w.click("settings", cx);
        assert!(f.shell.read(cx).settings.is_none());
        assert!(w.try_find("theme-select").is_none());
        assert!(!pane.read(cx).is_loading());
        assert!(pane.read(cx).sync_result_for_test().is_none());
        w.click("copy-diff", cx);
        assert_eq!(cx.read_from_clipboard().unwrap().text(), copy);
        w.press("f1", cx);
        assert!(f.shell.read(cx).help.is_some());
        assert!(w.try_find("gui-preferences").is_none());
        w.press("escape", cx);
        assert!(f.shell.read(cx).help.is_none());
        assert!(w.find("sync-confirm").visible());
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
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
