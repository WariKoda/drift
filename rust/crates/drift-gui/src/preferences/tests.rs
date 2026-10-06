use super::*;
mod close;
mod window;
mod window_pending;
mod window_restore;
use drift_app::logging::Logger;
use drift_core::store::Store;
use gpui_kit::component::{ActiveTheme, input::InputState};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Focusable, IntoElement, ParentElement, Render, Styled, TestAppContext,
    WindowOptions, div,
};

#[test]
fn mode_resolves_system_including_vibrant_appearances_but_fixed_modes_ignore_os() {
    for (appearance, expected) in [
        (WindowAppearance::Dark, ThemeMode::Dark),
        (WindowAppearance::VibrantDark, ThemeMode::Dark),
        (WindowAppearance::Light, ThemeMode::Light),
        (WindowAppearance::VibrantLight, ThemeMode::Light),
    ] {
        assert_eq!(
            effective_theme(ThemePreference::System, appearance),
            expected
        );
        assert_eq!(
            effective_theme(ThemePreference::Dark, appearance),
            ThemeMode::Dark
        );
        assert_eq!(
            effective_theme(ThemePreference::Light, appearance),
            ThemeMode::Light
        );
    }
}

#[test]
fn maximized_fullscreen_and_invalid_rectangles_preserve_normal_size() {
    let normal = WindowPreference::default();
    let screen = Bounds::new(point(px(-900.), px(20.)), size(px(3000.), px(2000.)));
    let maximized = WindowPreference {
        maximized: true,
        ..normal
    };
    assert_eq!(
        window_preference(normal, WindowBounds::Maximized(screen), false, screen.size),
        maximized
    );
    assert_eq!(
        window_preference(normal, WindowBounds::Windowed(screen), true, screen.size),
        maximized
    );
    assert_eq!(
        window_preference(
            maximized,
            WindowBounds::Fullscreen(screen),
            false,
            screen.size
        ),
        maximized
    );
    for width in [0., -1., f32::NAN, f32::INFINITY, 20000.] {
        let invalid = Bounds::new(point(px(0.), px(0.)), size(px(width), px(100.)));
        assert_eq!(
            window_preference(normal, WindowBounds::Windowed(invalid), false, invalid.size),
            normal
        );
    }
    let restored = Bounds::new(point(px(800.), px(-30.)), size(px(850.), px(500.)));
    assert_eq!(
        window_preference(
            maximized,
            WindowBounds::Windowed(restored),
            false,
            restored.size
        ),
        WindowPreference {
            width: 850.,
            height: 500.,
            maximized: false
        }
    );
}

#[test]
fn startup_centers_and_clamps_to_visible_display_without_persisting_position() {
    let display = Bounds::new(point(px(-900.), px(40.)), size(px(800.), px(600.)));
    let bounds = centered_bounds(size(px(1100.), px(720.)), display);
    assert_eq!(bounds, display);
    let smaller = centered_bounds(size(px(500.), px(300.)), display);
    assert_eq!(smaller.origin, point(px(-750.), px(190.)));
    assert_eq!(smaller.size, size(px(500.), px(300.)));
}

struct Harness {
    preferences: Entity<Preferences>,
    input: Entity<InputState>,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(crate::form_input::guard(&self.input, |input| input))
    }
}

#[gpui_kit::test]
fn mode_and_appearance_resolution_preserve_native_input(cx: &mut TestAppContext) {
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
                input: cx.new(|cx| InputState::new(w, cx).default_value("に🦀 e\u{301} ")),
            })
        })
        .unwrap()
    });
    let input = harness.read_with(cx, |h, _| h.input.clone());
    cx.update_window(handle, |_, w, cx| {
        input.update(cx, |input, cx| input.focus(w, cx));
        w.render_frame(cx);
    })
    .unwrap();
    let native_id = input.entity_id();
    for (choice, appearance, expected) in [
        (
            ThemePreference::System,
            WindowAppearance::Dark,
            ThemeMode::Dark,
        ),
        (
            ThemePreference::Light,
            WindowAppearance::Dark,
            ThemeMode::Light,
        ),
        (
            ThemePreference::Dark,
            WindowAppearance::Light,
            ThemeMode::Dark,
        ),
        (
            ThemePreference::System,
            WindowAppearance::Light,
            ThemeMode::Light,
        ),
    ] {
        cx.update_window(handle, |_, w, cx| {
            harness
                .read(cx)
                .preferences
                .clone()
                .update(cx, |preferences, cx| preferences.select(choice, w, cx));
            Theme::change(effective_theme(choice, appearance), Some(w), cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert_eq!(cx.theme().mode, expected);
            assert_eq!(
                gpui_kit::base::Theme::global(cx).appearance,
                if expected == ThemeMode::Dark {
                    gpui_kit::base::ThemeAppearance::Dark
                } else {
                    gpui_kit::base::ThemeAppearance::Light
                }
            );
            assert_eq!(input.entity_id(), native_id);
            assert!(input.read(cx).focus_handle(cx).is_focused(w));
            assert_eq!(input.read(cx).value().as_ref(), "に🦀 e\u{301} ");
        })
        .unwrap();
    }
    writer.finish().unwrap();
    assert_eq!(
        store.gui_preferences().unwrap().theme,
        ThemePreference::System
    );
}
