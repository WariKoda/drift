use super::*;

#[test]
fn changed_content_before_startup_maximize_acknowledgement_is_not_a_resize_choice() {
    let preferred = WindowPreference {
        maximized: true,
        ..Default::default()
    };
    let observed = WindowPreference {
        width: 800.,
        height: 572.,
        maximized: false,
    };
    let mut state = WindowState {
        preferred,
        observed,
        pending_maximize: true,
        pending_restore: false,
    };
    let rectangle = Bounds::new(point(px(0.), px(0.)), size(px(900.), px(700.)));
    for transient in [size(px(790.), px(560.)), size(px(900.), px(672.))] {
        assert_eq!(
            state.observe(WindowBounds::Windowed(rectangle), false, transient),
            None
        );
        assert_eq!(state.preferred, preferred);
        assert_eq!(state.observed, observed);
        assert!(state.pending_maximize);
    }
    assert_eq!(
        state.observe(WindowBounds::Maximized(rectangle), true, rectangle.size),
        None
    );
    assert!(!state.pending_maximize);
    assert_eq!(state.preferred, preferred);
    let normal = size(px(800.), px(572.));
    assert_eq!(
        state.observe(WindowBounds::Windowed(rectangle), false, normal),
        Some(WindowPreference {
            maximized: false,
            ..preferred
        })
    );
    let resized = size(px(750.), px(550.));
    assert_eq!(
        state.observe(WindowBounds::Windowed(rectangle), false, resized),
        Some(WindowPreference {
            width: 750.,
            height: 550.,
            maximized: false
        })
    );
}

#[test]
fn starting_already_maximized_preserves_normal_preference_on_first_restore() {
    let preferred = WindowPreference {
        maximized: true,
        ..Default::default()
    };
    let mut state = WindowState {
        preferred,
        observed: preferred,
        pending_maximize: false,
        pending_restore: false,
    };
    let frame = Bounds::new(point(px(0.), px(0.)), size(px(800.), px(600.)));
    assert_eq!(
        state.observe(
            WindowBounds::Windowed(frame),
            false,
            size(px(800.), px(572.))
        ),
        Some(WindowPreference {
            maximized: false,
            ..preferred
        })
    );
    assert_eq!(state.preferred.width, preferred.width);
    assert_eq!(state.preferred.height, preferred.height);
}
