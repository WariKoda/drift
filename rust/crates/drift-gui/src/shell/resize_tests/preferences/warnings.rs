use super::*;

#[gpui_kit::test]
async fn failed_preference_save_stays_visible_without_a_theme_footer(cx: &mut TestAppContext) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    let target = f._config.path().join("gui.toml");
    if target.is_file() {
        fs::remove_file(&target).unwrap();
    }
    fs::create_dir(&target).unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        assert!(w.try_find("gui-preferences").is_none());
        assert!(w.try_find("preferences-warning").is_none());
        w.click("settings", cx);
        assert!(w.within("settings-view").find("gui-preferences").visible());
    })
    .unwrap();
    crate::test_support::theme(f.handle, "Dark", cx);
    cx.update_window(f.handle, |_, w, cx| w.click("settings-close", cx))
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell
            .read(cx)
            .preferences
            .as_ref()
            .unwrap()
            .read(cx)
            .warning
            .is_some()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.find("preferences-warning").visible());
        for id in ["gui-preferences", "theme-select"] {
            assert!(w.try_find(id).is_none());
        }
        w.click("projects", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.find("preferences-warning").visible());
        assert!(w.try_find("gui-preferences").is_none());
        w.click("settings", cx);
        assert!(w.find("preferences-warning").visible());
        assert!(w.within("settings-view").find("theme-select").visible());
    })
    .unwrap();
    assert!(f.preferences_writer.as_ref().unwrap().finish().is_err());
}

#[gpui_kit::test]
async fn native_theme_picker_retries_a_failed_write_and_keeps_the_selected_value(
    cx: &mut TestAppContext,
) {
    let f = Fixture::build(cx, Some(GuiPreferences::default())).await;
    let target = f._config.path().join("gui.toml");
    if target.is_file() {
        fs::remove_file(&target).unwrap();
    }
    fs::create_dir(&target).unwrap();
    cx.update_window(f.handle, |_, w, cx| w.click("settings", cx))
        .unwrap();
    crate::test_support::theme(f.handle, "Dark", cx);
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell
            .read(cx)
            .preferences
            .as_ref()
            .unwrap()
            .read(cx)
            .warning
            .is_some()
    })
    .await;
    fs::remove_dir(&target).unwrap();
    crate::test_support::theme(f.handle, "Light", cx);
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell
            .read(cx)
            .preferences
            .as_ref()
            .unwrap()
            .read(cx)
            .warning
            .is_none()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(w.find("theme-select").value(), Some("Light"));
        assert_eq!(cx.theme().mode, ThemeMode::Light);
        assert!(w.try_find("preferences-warning").is_none());
        w.click("settings-close", cx);
    })
    .unwrap();
    f.preferences_writer.as_ref().unwrap().finish().unwrap();
    assert_eq!(
        Store::new(f._config.path().into())
            .gui_preferences()
            .unwrap()
            .theme,
        ThemePreference::Light
    );
}
