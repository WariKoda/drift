use super::*;
use drift_core::omarchy::{Palette, Rgb};
use gpui_kit::{
    AnyWindowHandle,
    component::input::{Editor, EditorState},
    test::TestAppContextExt,
};
use std::{fs, os::unix::fs::symlink};

fn palette(accent: &str, mode: &str) -> String {
    format!(
        "mode='{mode}'\nbackground='#102030'\nforeground='#d0e0f0'\nmuted='#8090a0'\nselection='#304050'\nred='#b02030'\ngreen='#20b030'\nyellow='#b0a030'\nblue='#2030b0'\nmagenta='#a030b0'\ncyan='#20a0b0'\naccent='{accent}'\n"
    )
}

struct ThemeHarness {
    preferences: Entity<Preferences>,
    editor: Entity<EditorState>,
}
impl Render for ThemeHarness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(cx.theme().background)
            .child(Editor::new(&self.editor).size_full())
    }
}
struct Fixture {
    _state: tempfile::TempDir,
    _config: tempfile::TempDir,
    source: PaletteSource,
    handle: AnyWindowHandle,
    harness: Entity<ThemeHarness>,
    preferences: Entity<Preferences>,
    writer: PreferencesWriter,
    store: Store,
}
impl Fixture {
    fn new(cx: &mut TestAppContext, mode: ThemePreference, initial: Option<&str>) -> Self {
        cx.executor().allow_parking();
        let state = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let source = PaletteSource::from_roots(Some(state.path().as_os_str()), None);
        if let Some(text) = initial {
            fs::create_dir_all(source.theme_directory().unwrap()).unwrap();
            fs::write(source.theme_directory().unwrap().join("colors.toml"), text).unwrap();
        }
        let store = Store::new(config.path().into());
        let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
        let (handle, harness) = cx.update(|cx| {
            gpui_kit::init(cx);
            Theme::update(cx, |theme| {
                theme.font_size = px(19.);
                theme.mono_font_size = px(17.);
            });
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| ThemeHarness {
                    preferences: cx.new(|cx| {
                        Preferences::with_source(
                            &GuiPreferences {
                                theme: mode,
                                ..Default::default()
                            },
                            writer.clone(),
                            Some("Initial preference load warning.".into()),
                            source.clone(),
                            w,
                            cx,
                        )
                    }),
                    editor: cx.new(|cx| {
                        EditorState::new(w, cx).default_value("fn main() { /* に🦀 */ }")
                    }),
                })
            })
            .unwrap()
        });
        let preferences = harness.read_with(cx, |h, _| h.preferences.clone());
        Self {
            _state: state,
            _config: config,
            source,
            handle,
            harness,
            preferences,
            writer,
            store,
        }
    }

    async fn wait_accent(&self, accent: u32, cx: &mut TestAppContext) {
        cx.wait_for(self.handle, Duration::from_secs(10), |_, cx| {
            self.preferences
                .read(cx)
                .theme_snapshot
                .as_ref()
                .is_some_and(|value| {
                    value
                        .as_ref()
                        .is_ok_and(|palette| palette.accent == Rgb(accent))
                })
                && cx.theme().ring == gpui_kit::rgb(accent).into()
        })
        .await;
    }

    fn select(&self, mode: ThemePreference, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, w, cx| {
            self.preferences
                .update(cx, |preferences, cx| preferences.select(mode, w, cx));
        })
        .unwrap();
    }
}

#[gpui_kit::test]
async fn omarchy_async_start_live_content_atomic_switch_and_recovery_preserve_native_editor(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(
        cx,
        ThemePreference::Omarchy,
        Some(&palette("#112233", "dark")),
    );
    let editor = f.harness.read_with(cx, |h, _| h.editor.clone());
    let editor_id = editor.entity_id();
    cx.update_window(f.handle, |_, w, cx| {
        editor.update(cx, |editor, cx| {
            editor.focus(w, cx);
            editor.select_all(w, cx);
        });
    })
    .unwrap();
    let selected = editor.read_with(cx, |editor, _| editor.selected_range());
    let preferred = f.preferences.read_with(cx, |p, _| p.preferred_window());
    f.wait_accent(0x112233, cx).await;
    let changes = std::rc::Rc::new(std::cell::Cell::new(0));
    let _subscription = cx.update(|cx| {
        let changes = changes.clone();
        cx.observe(&f.preferences, move |_, _| changes.set(changes.get() + 1))
    });
    let highlights = cx.update(|cx| cx.theme().highlight_theme.clone());
    let theme = f.source.theme_directory().unwrap();
    // Equivalent parsed data must not notify observers or rebuild native themes.
    fs::write(
        theme.join("colors.toml"),
        palette("#112233", "dark") + "ignored_gradient='rgba(aabbccdd) 45deg'\n",
    )
    .unwrap();
    cx.executor().timer(Duration::from_secs(2)).await;
    assert_eq!(changes.get(), 0);
    cx.update(|cx| {
        assert!(std::sync::Arc::ptr_eq(
            &highlights,
            &cx.theme().highlight_theme
        ))
    });
    fs::write(theme.join("colors.toml"), palette("#445566", "dark")).unwrap();
    f.wait_accent(0x445566, cx).await;
    // Replacing a parent symlink must be detected, not just changes to a watched inode.
    let current = theme.parent().unwrap();
    let next = f._state.path().join("new-theme");
    fs::create_dir_all(next.join("theme")).unwrap();
    fs::write(next.join("theme/colors.toml"), palette("#778899", "light")).unwrap();
    fs::rename(current, current.with_file_name("old")).unwrap();
    symlink(current.with_file_name("old"), current).unwrap();
    symlink(&next, current.with_file_name("next")).unwrap();
    fs::rename(current.with_file_name("next"), current).unwrap();
    f.wait_accent(0x778899, cx).await;
    fs::write(theme.join("colors.toml"), "background='bad'").unwrap();
    cx.wait_for(f.handle, Duration::from_secs(10), |_, cx| {
        f.preferences
            .read(cx)
            .theme_warning
            .as_ref()
            .is_some_and(|s| s.contains("unavailable"))
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(cx.theme().mode, ThemeMode::from(w.appearance()));
        assert!(
            f.preferences
                .read(cx)
                .warning
                .as_ref()
                .unwrap()
                .contains("Initial preference load warning.")
        );
    })
    .unwrap();
    fs::write(theme.join("recovered.toml"), palette("#aabbcc", "light")).unwrap();
    fs::rename(theme.join("recovered.toml"), theme.join("colors.toml")).unwrap();
    f.wait_accent(0xaabbcc, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let p = f.preferences.read(cx);
        assert_eq!(p.mode, ThemePreference::Omarchy);
        assert_eq!(
            p.warning.as_deref(),
            Some("Initial preference load warning.")
        );
        assert_eq!(p.preferred_window(), preferred);
        assert_eq!(cx.theme().font_size, px(19.));
        assert_eq!(cx.theme().mono_font_size, px(17.));
        assert_eq!(cx.theme().mode, ThemeMode::Light);
        assert_eq!(editor.entity_id(), editor_id);
        assert_eq!(editor.read(cx).selected_range(), selected);
        assert_eq!(editor.read(cx).value().as_ref(), "fn main() { /* に🦀 */ }");
        assert!(editor.read(cx).focus_handle(cx).is_focused(w));
        assert_eq!(cx.theme().success, gpui_kit::rgb(0x20b030).into());
        assert_eq!(cx.theme().danger, gpui_kit::rgb(0xb02030).into());
        assert_eq!(cx.theme().warning, gpui_kit::rgb(0xb0a030).into());
        assert_eq!(
            cx.theme().highlight_theme.style.editor_background,
            Some(cx.theme().background)
        );
        assert_eq!(
            cx.theme().highlight_theme.style.editor_foreground,
            Some(cx.theme().foreground)
        );
        assert_eq!(
            cx.theme()
                .highlight_theme
                .style
                .syntax
                .style("keyword")
                .unwrap()
                .color,
            Some(cx.theme().magenta)
        );
        assert_eq!(
            gpui_kit::base::Theme::global(cx).appearance,
            gpui_kit::base::ThemeAppearance::Light
        );
    })
    .unwrap();
    f.writer.finish().unwrap();
    // Live changes are presentation-only, not preference writes.
    assert!(!f.store.dir().join("gui.toml").exists());
}

#[gpui_kit::test]
async fn unavailable_start_recovers_and_composes_writer_and_initial_warnings(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx, ThemePreference::Omarchy, None);
    cx.wait_for(f.handle, Duration::from_secs(10), |_, cx| {
        f.preferences
            .read(cx)
            .theme_snapshot
            .as_ref()
            .is_some_and(Result::is_err)
    })
    .await;
    // Force a real writer failure, without mocks or global config/environment.
    fs::create_dir_all(f.store.dir().join("gui.toml")).unwrap();
    f.writer.queue(GuiPreferenceChange::BrowserSplit(Some(0.4)));
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.preferences.read(cx).writer_warning.is_some()
    })
    .await;
    f.preferences.read_with(cx, |p, _| {
        let warning = p.warning.as_ref().unwrap();
        assert!(warning.contains("Initial preference load warning."));
        assert!(warning.contains("Omarchy theme unavailable"));
        assert!(warning.contains("save could not be confirmed"));
    });
    fs::create_dir_all(f.source.theme_directory().unwrap()).unwrap();
    fs::write(
        f.source.theme_directory().unwrap().join("colors.toml"),
        palette("#abcdef", "dark"),
    )
    .unwrap();
    f.wait_accent(0xabcdef, cx).await;
    f.preferences.read_with(cx, |p, _| {
        let warning = p.warning.as_ref().unwrap();
        assert!(!warning.contains("Omarchy"));
        assert!(warning.contains("Initial preference load warning."));
        assert!(warning.contains("save could not be confirmed"));
    });
    fs::remove_dir(f.store.dir().join("gui.toml")).unwrap();
    f.writer.queue(GuiPreferenceChange::BrowserSplit(Some(0.4)));
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.preferences.read(cx).writer_warning.is_none()
    })
    .await;
    assert_eq!(
        f.preferences
            .read_with(cx, |p, _| p.warning.clone())
            .as_deref(),
        Some("Initial preference load warning.")
    );
    f.writer.finish().unwrap();
}

#[gpui_kit::test]
async fn selecting_omarchy_waits_for_update_fixed_modes_stop_watching_and_unchanged_selection_does_not_write(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(
        cx,
        ThemePreference::System,
        Some(&palette("#112233", "dark")),
    );
    for mode in [
        ThemePreference::System,
        ThemePreference::Dark,
        ThemePreference::Light,
    ] {
        f.select(mode, cx);
        f.preferences
            .read_with(cx, |p, _| assert!(p.theme_monitor.is_none()));
    }
    f.select(ThemePreference::Omarchy, cx);
    f.wait_accent(0x112233, cx).await;
    f.select(ThemePreference::Dark, cx);
    let before = cx.update(|cx| cx.theme().colors);
    fs::write(
        f.source.theme_directory().unwrap().join("colors.toml"),
        palette("#ffffff", "light"),
    )
    .unwrap();
    cx.executor().timer(Duration::from_secs(2)).await;
    cx.update(|cx| assert_eq!(cx.theme().colors, before));
    f.preferences.read_with(cx, |p, _| {
        assert!(p.theme_monitor.is_none());
        assert!(p.theme_warning.is_none());
    });
    f.writer.finish().unwrap();
    assert_eq!(
        f.store.gui_preferences().unwrap().theme,
        ThemePreference::Dark
    );
    // Closed writers warn on every queued request, making accidental no-op writes visible.
    f.select(ThemePreference::Dark, cx);
    cx.executor().timer(Duration::from_millis(100)).await;
    assert!(f.writer.failures().borrow().is_none());
}

#[test]
fn semantic_and_ansi_native_projection_match_and_tokens_are_not_stock_colors() {
    let semantic = Palette::parse(&palette("#2030b0", "dark"), false).unwrap();
    let ansi = "mode='dark'\ncolor0='#102030'\ncolor7='#d0e0f0'\ncolor8='#8090a0'\nselection='#304050'\ncolor1='#b02030'\ncolor2='#20b030'\ncolor3='#b0a030'\ncolor4='#2030b0'\ncolor5='#a030b0'\ncolor6='#20a0b0'\n";
    let ansi = Palette::parse(ansi, false).unwrap();
    let c = super::super::omarchy::colors(&semantic);
    assert_eq!(c, super::super::omarchy::colors(&ansi));
    assert_eq!(c.background, gpui_kit::rgb(0x102030).into());
    assert_eq!(c.list_active, gpui_kit::rgb(0x304050).into());
    assert_eq!(c.ring, gpui_kit::rgb(0x2030b0).into());
    assert_eq!(c.muted_foreground, gpui_kit::rgb(0x8090a0).into());
    assert_eq!(c.popover_foreground, c.foreground);
    assert_eq!(c.status_bar_border, c.border);
    assert_eq!(c.button_success, c.success);
}
