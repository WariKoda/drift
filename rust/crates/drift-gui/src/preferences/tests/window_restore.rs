use super::*;

#[test]
fn unmaximize_state_before_restore_geometry_never_saves_the_screen_as_normal_size() {
    let preferred = WindowPreference {
        maximized: true,
        ..Default::default()
    };
    let observed = WindowPreference {
        width: 800.,
        height: 572.,
        maximized: true,
    };
    let mut state = WindowState {
        preferred,
        observed,
        pending_maximize: false,
        pending_restore: false,
    };
    let screen = Bounds::new(point(px(0.), px(0.)), size(px(900.), px(700.)));
    let normal = size(px(800.), px(572.));
    let restored_preference = WindowPreference {
        maximized: false,
        ..preferred
    };
    assert_eq!(
        state.observe(WindowBounds::Windowed(screen), false, screen.size),
        Some(restored_preference)
    );
    assert!(state.pending_restore);
    assert_eq!(state.observed.width, 800.);
    assert_eq!(state.observed.height, 572.);
    assert_eq!(
        state.observe(
            WindowBounds::Windowed(screen),
            false,
            size(px(890.), px(680.))
        ),
        None
    );
    assert_eq!(
        state.observe(WindowBounds::Fullscreen(screen), false, screen.size),
        None
    );
    assert!(state.pending_restore);
    assert_eq!(
        state.observe(WindowBounds::Windowed(screen), false, normal),
        None
    );
    assert!(!state.pending_restore);
    assert_eq!(state.preferred, restored_preference);
    assert_eq!(
        state.observe(
            WindowBounds::Windowed(screen),
            false,
            size(px(790.), px(550.))
        ),
        Some(WindowPreference {
            width: 790.,
            height: 550.,
            maximized: false
        })
    );
}

#[test]
fn restore_geometry_before_unmaximize_state_preserves_preference_and_allows_next_user_resize() {
    let preferred = WindowPreference {
        maximized: true,
        ..Default::default()
    };
    let observed = WindowPreference {
        width: 800.,
        height: 572.,
        maximized: true,
    };
    let mut state = WindowState {
        preferred,
        observed,
        pending_maximize: false,
        pending_restore: false,
    };
    let frame = Bounds::new(point(px(0.), px(0.)), size(px(800.), px(600.)));
    let normal = size(px(800.), px(572.));
    assert_eq!(
        state.observe(WindowBounds::Maximized(frame), true, normal),
        None
    );
    assert_eq!(
        state.observe(WindowBounds::Windowed(frame), false, normal),
        Some(WindowPreference {
            maximized: false,
            ..preferred
        })
    );
    assert!(!state.pending_restore);
    assert_eq!(
        state.observe(
            WindowBounds::Windowed(frame),
            false,
            size(px(790.), px(550.))
        ),
        Some(WindowPreference {
            width: 790.,
            height: 550.,
            maximized: false
        })
    );
}
