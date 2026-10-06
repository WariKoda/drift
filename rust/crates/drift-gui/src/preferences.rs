//! Window-owned presentation preferences; file work belongs to the app writer.
use drift_app::gui_preferences::PreferencesWriter;
use drift_core::gui_preferences::{
    GuiPreferenceChange, GuiPreferences, ThemePreference, WindowPreference,
};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{
    App, Bounds, Context, Pixels, Size, Subscription, Task, Window, WindowAppearance, WindowBounds,
    point, px, size,
};
use std::time::{Duration, Instant};

pub(crate) struct Preferences {
    pub mode: ThemePreference,
    pub warning: Option<String>,
    window: WindowState,
    writer: PreferencesWriter,
    geometry_changed: Option<Instant>,
    _window_monitor: Task<()>,
    _subscriptions: Vec<Subscription>,
}
impl Preferences {
    pub fn new(
        preferences: &GuiPreferences,
        writer: PreferencesWriter,
        warning: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.set_window_appearance(match preferences.theme {
            ThemePreference::System => None,
            ThemePreference::Dark => Some(WindowAppearance::Dark),
            ThemePreference::Light => Some(WindowAppearance::Light),
        });
        apply_theme(preferences.theme, window, cx);
        let mut content = window.viewport_size();
        if !window.is_maximized()
            && !window.is_fullscreen()
            && let Some(display) = window.display(cx)
        {
            let fitted =
                fitted_content_size(content, window.bounds().size, display.visible_bounds().size);
            if fitted != content {
                window.resize(fitted);
                content = fitted;
            }
        }
        let observed = window_preference(
            preferences.window,
            window.window_bounds(),
            window.is_maximized(),
            content,
        );
        let window_state = WindowState {
            preferred: preferences.window,
            observed,
            pending_maximize: preferences.window.maximized && !observed.maximized,
            pending_restore: false,
        };
        let subscriptions = vec![
            cx.observe_window_appearance(window, |this, window, cx| {
                if this.mode == ThemePreference::System {
                    apply_theme(this.mode, window, cx);
                }
            }),
            cx.observe_window_bounds(window, |this, _, cx| {
                this.geometry_changed = Some(cx.background_executor().now());
            }),
            cx.on_app_quit({
                let handle = window.window_handle();
                move |this, cx| {
                    let _ = handle.update(cx, |_, window, cx| {
                        this.sample_window(window, true, cx.background_executor().now())
                    });
                    async {}
                }
            }),
        ];
        let closing = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = closing.update(cx, |this, cx| {
                this.sample_window(window, true, cx.background_executor().now())
            });
            true
        });
        // X11 property notifications can change maximization without a bounds
        // callback. Sample settled state independently of geometry notifications.
        let monitor = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        if this.sample_window(window, false, cx.background_executor().now()) {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        if window_state.pending_maximize {
            window.zoom_window();
        }
        let mut failures = writer.failures();
        let writer_warning = failures.borrow_and_update().clone();
        cx.spawn(async move |this, cx| {
            while failures.changed().await.is_ok() {
                let warning = failures.borrow_and_update().clone();
                if this
                    .update(cx, |this, cx| {
                        this.warning = warning;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            mode: preferences.theme,
            warning: writer_warning.or(warning),
            window: window_state,
            writer,
            geometry_changed: None,
            _window_monitor: monitor,
            _subscriptions: subscriptions,
        }
    }

    fn sample_window(&mut self, window: &Window, closing: bool, now: Instant) -> bool {
        if !window.is_visible() || window.is_fullscreen() {
            return false;
        }
        let unsettled = self.geometry_changed.is_some_and(|changed| {
            now.saturating_duration_since(changed) < Duration::from_millis(150)
        });
        if !closing && unsettled {
            return false;
        }
        let maximized = window.is_maximized();
        if !closing
            && self.geometry_changed.is_none()
            && maximized == self.window.observed.maximized
            && !self.window.pending_maximize
            && !self.window.pending_restore
        {
            return false;
        }
        self.geometry_changed = None;
        let pending = (self.window.pending_maximize, self.window.pending_restore);
        // Closing cannot distinguish a fresh user resize from configure geometry
        // preceding the maximization property. Keep normal size until settled.
        let content = if unsettled {
            size(
                px(self.window.observed.width),
                px(self.window.observed.height),
            )
        } else {
            window.viewport_size()
        };
        if let Some(desired) = self
            .window
            .observe(window.window_bounds(), maximized, content)
        {
            self.writer.queue(GuiPreferenceChange::Window(desired));
        }
        pending != (self.window.pending_maximize, self.window.pending_restore)
    }

    #[cfg(test)]
    pub fn preferred_window(&self) -> WindowPreference {
        self.window.preferred
    }

    pub fn awaiting_geometry(&self) -> bool {
        self.window.pending_maximize || self.window.pending_restore
    }

    pub fn select(&mut self, mode: ThemePreference, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            cx.set_window_appearance(match mode {
                ThemePreference::System => None,
                ThemePreference::Dark => Some(WindowAppearance::Dark),
                ThemePreference::Light => Some(WindowAppearance::Light),
            });
            apply_theme(mode, window, cx);
            cx.notify();
        }
        self.writer.queue(GuiPreferenceChange::Theme(mode));
    }
}

pub(crate) fn initial_window_bounds(preference: WindowPreference, cx: &App) -> WindowBounds {
    let requested = size(px(preference.width), px(preference.height));
    let bounds = cx.primary_display().map_or_else(
        || Bounds::centered(None, requested, cx),
        |display| centered_bounds(requested, display.visible_bounds()),
    );
    // Measure and retain the normal content rectangle before requesting zoom.
    WindowBounds::Windowed(bounds)
}

fn centered_bounds(requested: gpui_kit::Size<Pixels>, display: Bounds<Pixels>) -> Bounds<Pixels> {
    let dimensions = requested.min(&display.size);
    let center = display.center();
    Bounds::new(
        point(
            center.x - dimensions.width / 2.,
            center.y - dimensions.height / 2.,
        ),
        dimensions,
    )
}

fn fitted_content_size(
    content: Size<Pixels>,
    frame: Size<Pixels>,
    visible: Size<Pixels>,
) -> Size<Pixels> {
    let overhead = size(
        (frame.width - content.width).max(px(0.)),
        (frame.height - content.height).max(px(0.)),
    );
    content.min(&size(
        (visible.width - overhead.width).max(px(1.)),
        (visible.height - overhead.height).max(px(1.)),
    ))
}

struct WindowState {
    preferred: WindowPreference,
    observed: WindowPreference,
    pending_maximize: bool,
    pending_restore: bool,
}
impl WindowState {
    fn observe(
        &mut self,
        bounds: WindowBounds,
        maximized: bool,
        content: Size<Pixels>,
    ) -> Option<WindowPreference> {
        if matches!(bounds, WindowBounds::Fullscreen(_)) {
            return None;
        }
        let mut next = window_preference(self.observed, bounds, maximized, content);
        if self.pending_maximize {
            // X11 geometry and state acknowledgements arrive separately. Until
            // maximization is acknowledged, normal configure events are ambiguous.
            if !next.maximized {
                return None;
            }
            self.pending_maximize = false;
        }
        if self.observed.maximized && !next.maximized {
            self.pending_restore =
                next.width != self.observed.width || next.height != self.observed.height;
            next.width = self.observed.width;
            next.height = self.observed.height;
        } else if self.pending_restore {
            if next.maximized
                || (next.width == self.observed.width && next.height == self.observed.height)
            {
                self.pending_restore = false;
            } else {
                return None;
            }
        }
        if next == self.observed {
            return None;
        }
        let desired = if next.maximized != self.observed.maximized
            || (next.width == self.observed.width && next.height == self.observed.height)
        {
            WindowPreference {
                maximized: next.maximized,
                ..self.preferred
            }
        } else {
            next
        };
        self.observed = next;
        if desired == self.preferred {
            return None;
        }
        self.preferred = desired;
        Some(desired)
    }
}

fn apply_theme(mode: ThemePreference, window: &mut Window, cx: &mut App) {
    Theme::change(effective_theme(mode, window.appearance()), Some(window), cx);
}
fn effective_theme(mode: ThemePreference, appearance: WindowAppearance) -> ThemeMode {
    match mode {
        ThemePreference::Dark => ThemeMode::Dark,
        ThemePreference::Light => ThemeMode::Light,
        ThemePreference::System => appearance.into(),
    }
}

fn window_preference(
    previous: WindowPreference,
    bounds: WindowBounds,
    maximized: bool,
    content: Size<Pixels>,
) -> WindowPreference {
    // X11 reports maximized screen bounds, not the restore rectangle. Preserve
    // our last normal size on every backend instead of persisting that rectangle.
    match bounds {
        WindowBounds::Fullscreen(_) => previous,
        WindowBounds::Maximized(_) => WindowPreference {
            maximized: true,
            ..previous
        },
        WindowBounds::Windowed(_) if maximized => WindowPreference {
            maximized: true,
            ..previous
        },
        WindowBounds::Windowed(_) => {
            // macOS opens with a content rectangle, but bounds() includes its
            // native titlebar. Persist the drawable logical size on all backends.
            let width = content.width.as_f32();
            let height = content.height.as_f32();
            if width.is_finite()
                && height.is_finite()
                && width > 0.
                && height > 0.
                && width <= 16384.
                && height <= 16384.
            {
                WindowPreference {
                    width,
                    height,
                    maximized: false,
                }
            } else {
                previous
            }
        }
    }
}

#[cfg(test)]
mod tests;
