use super::*;
use crate::{
    gui_preferences::{GuiPreferenceChange, GuiPreferences, ThemePreference},
    store::Store,
};

fn semantic() -> String {
    [
        ("background", "#102030"),
        ("foreground", "#d0e0f0"),
        ("red", "#b02030"),
        ("green", "#20b030"),
        ("yellow", "#b0a030"),
        ("blue", "#2030b0"),
        ("magenta", "#a030b0"),
        ("cyan", "#20a0b0"),
    ]
    .into_iter()
    .map(|(key, value)| format!("{key} = '{value}'\n"))
    .collect()
}

#[test]
fn semantic_aliases_canonical_precedence_and_unrelated_keys() {
    let text = semantic()
        + "accent='#aabbcc'\nmuted='#8899aa'\nselection='#334455'\nbg='#ffffff'\npurple='#ffffff'\nhyprland_active_border='rgba(aabbccdd) rgba(112233ff) 45deg'\n[unrelated]\nx=[1,2,3]\n";
    let p = Palette::parse(&text, false).unwrap();
    assert_eq!(p.background, Rgb(0x102030));
    assert_eq!(p.accent, Rgb(0xaabbcc));
    assert_eq!(p.muted, Rgb(0x8899aa));
    assert_eq!(p.selection, Rgb(0x334455));
    assert_eq!(p.magenta, Rgb(0xa030b0));
    let aliases = semantic()
        .replace("background", "bg")
        .replace("foreground", "fg")
        .replace("magenta", "purple")
        + "dark_bg='#010203'\ndarker_bg='#010101'\nlighter_bg='#223344'\nbright_fg='#fefefe'\ndark_fg='#777777'\n";
    let p = Palette::parse(&aliases, false).unwrap();
    assert_eq!(p.dark_background, Rgb(0x010203));
    assert_eq!(p.darker_background, Rgb(0x010101));
    assert_eq!(p.lighter_background, Rgb(0x223344));
    assert_eq!(p.selection_foreground, Rgb(0xfefefe));
    assert_eq!(p.muted, Rgb(0x777777));
}

#[test]
fn ansi_palette_resolves_all_16_colors_and_semantic_overrides() {
    let text: String = (0..16)
        .map(|n| format!("color{n}='#{:06x}'\n", n * 0x101010))
        .collect();
    let p = Palette::parse(&text, false).unwrap();
    assert_eq!(p.background, Rgb(0));
    assert_eq!(p.foreground, Rgb(0x707070));
    assert_eq!(p.accent, Rgb(0x404040));
    assert_eq!(p.muted, Rgb(0x808080));
    assert_eq!(p.selection, Rgb(0x808080));
    assert_eq!(p.bright_foreground, Rgb(0xf0f0f0));
    assert_eq!(
        [
            p.bright_red,
            p.bright_green,
            p.bright_yellow,
            p.bright_blue,
            p.bright_magenta,
            p.bright_cyan
        ],
        [9, 10, 11, 12, 13, 14].map(|n| Rgb(n * 0x101010))
    );
    let p = Palette::parse(&(text + "red='#123456'\nbackground='#abcdef'\n"), false).unwrap();
    assert_eq!(p.red, Rgb(0x123456));
    assert_eq!(p.background, Rgb(0xabcdef));
}

#[test]
fn mode_precedence_mode_then_legacy_then_marker_then_brightness() {
    let dark = semantic();
    assert_eq!(
        Palette::parse(&dark, false).unwrap().mode,
        PaletteMode::Dark
    );
    assert_eq!(
        Palette::parse(&dark, true).unwrap().mode,
        PaletteMode::Light
    );
    assert_eq!(
        Palette::parse(&(dark.clone() + "theme_type='dark'\n"), true)
            .unwrap()
            .mode,
        PaletteMode::Dark
    );
    assert_eq!(
        Palette::parse(&(dark.clone() + "mode='light'\ntheme_type='dark'\n"), false)
            .unwrap()
            .mode,
        PaletteMode::Light
    );
    let light = dark.replace("#102030", "#f0f0f0");
    assert_eq!(
        Palette::parse(&light, false).unwrap().mode,
        PaletteMode::Light
    );
    assert_eq!(
        Palette::parse(&(light + "mode='dark'\n"), true)
            .unwrap()
            .mode,
        PaletteMode::Dark
    );
}

#[test]
fn invalid_missing_incomplete_duplicate_and_oversized_palettes_are_rejected() {
    for text in [
        String::new(),
        "background='#000000'".into(),
        "not toml".into(),
        semantic().replace("red = '#b02030'", "red = '#bad'"),
        semantic().replace("red = '#b02030'", "red = 42"),
        semantic().replace("red = '#b02030'", "red = '#gg0000'"),
        semantic().replace("red = '#b02030'", "red = '#12345678'"),
        semantic() + "color15='invalid'\n",
        semantic() + "muted=''\n",
        semantic() + "mode='auto'\n",
        semantic() + "mode=42\n",
        semantic() + "background='#112233'\n",
        "#".repeat(MAX_PALETTE_BYTES + 1),
    ] {
        assert!(Palette::parse(&text, false).is_err(), "accepted {text}");
    }
}

#[test]
fn preference_roundtrip_patch_and_default_are_unchanged() {
    assert_eq!(GuiPreferences::default().theme, ThemePreference::System);
    let preferences = GuiPreferences {
        theme: ThemePreference::Omarchy,
        ..Default::default()
    };
    let encoded = toml::to_string(&preferences).unwrap();
    assert!(encoded.contains("theme = \"omarchy\""));
    assert_eq!(
        toml::from_str::<GuiPreferences>(&encoded).unwrap(),
        preferences
    );
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    for mode in [
        ThemePreference::Omarchy,
        ThemePreference::Dark,
        ThemePreference::Light,
        ThemePreference::System,
    ] {
        store
            .update_gui_preferences(&[GuiPreferenceChange::Theme(mode)])
            .unwrap();
        assert_eq!(store.gui_preferences().unwrap().theme, mode);
    }
}
