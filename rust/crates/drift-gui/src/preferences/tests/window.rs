use super::*;

#[test]
fn frame_content_difference_never_grows_the_saved_size() {
    let content = size(px(1100.), px(720.));
    for titlebar in [0., 22., 28., 40.] {
        let frame = Bounds::new(
            point(px(-300.), px(10.)),
            size(content.width, content.height + px(titlebar)),
        );
        let saved = window_preference(
            WindowPreference::default(),
            WindowBounds::Windowed(frame),
            false,
            content,
        );
        assert_eq!(saved.width, 1100.);
        assert_eq!(saved.height, 720.);
        assert_eq!(
            fitted_content_size(content, frame.size, size(px(1920.), px(1080.))),
            content
        );
        assert_eq!(
            fitted_content_size(content, frame.size, size(px(900.), px(600.))),
            size(px(900.), px(600. - titlebar))
        );
    }
}

#[test]
fn clamped_startup_and_state_only_maximize_restore_preserve_preferred_content_size() {
    let preferred = WindowPreference::default();
    let observed = WindowPreference {
        width: 800.,
        height: 572.,
        maximized: false,
    };
    let normal = Bounds::new(point(px(0.), px(0.)), size(px(800.), px(600.)));
    let screen = Bounds::new(point(px(0.), px(0.)), size(px(900.), px(700.)));
    let content = size(px(800.), px(572.));
    let mut state = WindowState {
        preferred,
        observed,
        pending_maximize: false,
        pending_restore: false,
    };
    assert_eq!(
        state.observe(WindowBounds::Windowed(normal), false, content),
        None
    );
    assert_eq!(
        state.observe(WindowBounds::Maximized(screen), true, screen.size),
        Some(WindowPreference {
            maximized: true,
            ..preferred
        })
    );
    assert_eq!(
        state.observe(WindowBounds::Fullscreen(screen), false, screen.size),
        None
    );
    assert_eq!(
        state.observe(WindowBounds::Windowed(normal), false, content),
        Some(preferred)
    );
    assert_eq!(state.preferred, preferred);
    assert_eq!(state.observed, observed);
    let resized = size(px(790.), px(550.));
    assert_eq!(
        state.observe(WindowBounds::Windowed(normal), false, resized),
        Some(WindowPreference {
            width: 790.,
            height: 550.,
            maximized: false
        })
    );
}

#[test]
fn pending_startup_maximize_and_unchanged_position_do_not_save_false_normal_state() {
    let preferred = WindowPreference {
        maximized: true,
        ..Default::default()
    };
    let observed = WindowPreference {
        width: 800.,
        height: 572.,
        maximized: false,
    };
    let moved = Bounds::new(point(px(100.), px(-300.)), size(px(800.), px(600.)));
    let content = size(px(800.), px(572.));
    let mut state = WindowState {
        preferred,
        observed,
        pending_maximize: true,
        pending_restore: false,
    };
    assert_eq!(
        state.observe(WindowBounds::Windowed(moved), false, content),
        None
    );
    assert_eq!(
        state.observe(WindowBounds::Maximized(moved), true, content),
        None
    );
    assert_eq!(state.preferred, preferred);
}
