use super::*;

#[gpui_kit::test]
async fn closing_snapshot_rejects_unsettled_geometry_but_captures_settled_resize(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
    let (handle, harness) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
            cx.new(|cx| Harness {
                preferences: cx.new(|cx| {
                    Preferences::new(&GuiPreferences::default(), writer.clone(), None, w, cx)
                }),
                input: cx.new(|cx| InputState::new(w, cx)),
            })
        })
        .unwrap()
    });
    cx.simulate_window_resize(handle, size(px(760.), px(540.)));
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        harness
            .read(cx)
            .preferences
            .clone()
            .update(cx, |preferences, cx| {
                preferences.sample_window(w, true, cx.background_executor().now());
                assert_eq!(preferences.preferred_window(), WindowPreference::default());
            });
    })
    .unwrap();
    assert!(!store.dir().join("gui.toml").exists());
    cx.simulate_window_resize(handle, size(px(750.), px(530.)));
    cx.run_until_parked();
    cx.executor().timer(Duration::from_millis(170)).await;
    cx.update_window(handle, |_, w, cx| {
        harness
            .read(cx)
            .preferences
            .clone()
            .update(cx, |preferences, cx| {
                preferences.sample_window(w, true, cx.background_executor().now());
                assert_eq!(
                    preferences.preferred_window(),
                    WindowPreference {
                        width: 750.,
                        height: 530.,
                        maximized: false
                    }
                );
            });
    })
    .unwrap();
    writer.finish().unwrap();
    assert_eq!(
        store.gui_preferences().unwrap().window,
        WindowPreference {
            width: 750.,
            height: 530.,
            maximized: false
        }
    );
}
