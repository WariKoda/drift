use super::{APP_ID, APP_NAME, main_window_options};
use gpui_kit::{Bounds, WindowBounds, WindowOptions, point, px, size};

#[test]
fn window_identity_sets_static_app_id_and_title() {
    let options = main_window_options(WindowBounds::default());
    assert_eq!(options.app_id.as_deref(), Some(APP_ID));
    assert_eq!(APP_ID, "io.github.WariKoda.drift-gui");
    assert_eq!(options.titlebar.unwrap().title.as_deref(), Some(APP_NAME));
    assert_eq!(APP_NAME, "Drift GUI");
}

#[test]
fn window_identity_preserves_bounds_and_native_decoration_defaults() {
    let bounds = Bounds {
        origin: point(px(25.), px(40.)),
        size: size(px(1100.), px(720.)),
    };
    let defaults = WindowOptions::default();
    let default_titlebar = defaults.titlebar.unwrap();
    for state in [
        WindowBounds::Windowed(bounds),
        WindowBounds::Maximized(bounds),
        WindowBounds::Fullscreen(bounds),
    ] {
        let options = main_window_options(state);
        assert_eq!(options.window_bounds, Some(state));
        let titlebar = options.titlebar.unwrap();
        assert_eq!(
            titlebar.appears_transparent,
            default_titlebar.appears_transparent
        );
        assert_eq!(
            titlebar.traffic_light_position,
            default_titlebar.traffic_light_position
        );
        assert_eq!(options.window_decorations, defaults.window_decorations);
        assert_eq!(
            options.app_owns_titlebar_drag,
            defaults.app_owns_titlebar_drag
        );
        assert_eq!(options.is_movable, defaults.is_movable);
        assert_eq!(options.is_resizable, defaults.is_resizable);
        assert_eq!(options.is_minimizable, defaults.is_minimizable);
        assert_eq!(options.focus, defaults.focus);
        assert_eq!(options.show, defaults.show);
        assert_eq!(options.kind, defaults.kind);
        assert_eq!(options.window_min_size, defaults.window_min_size);
        assert_eq!(
            options.inactive_frame_interval,
            defaults.inactive_frame_interval
        );
        assert_eq!(options.window_background, defaults.window_background);
        assert_eq!(options.display_id, defaults.display_id);
        assert_eq!(options.tabbing_identifier, defaults.tabbing_identifier);
        assert!(options.icon.is_none());
    }
}
