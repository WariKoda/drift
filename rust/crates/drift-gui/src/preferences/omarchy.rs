//! Projection onto Kit's native colors/tokens and editor highlight theme.
use super::change_theme;
use drift_core::omarchy::{Palette, PaletteMode, Rgb};
use gpui_kit::component::highlighter::{HighlightTheme, HighlightThemeStyle};
use gpui_kit::component::{Theme, ThemeColor, ThemeMode, ThemeRegistry};
use gpui_kit::{App, Hsla, Window, WindowAppearance, rgb};
use std::sync::Arc;

fn color(rgb_value: Rgb) -> Hsla {
    rgb(rgb_value.0).into()
}

pub(super) fn apply(p: &Palette, window: &mut Window, cx: &mut App) -> Result<(), String> {
    let style = editor_style(p)?;
    let mode = match p.mode {
        PaletteMode::Dark => ThemeMode::Dark,
        PaletteMode::Light => ThemeMode::Light,
    };
    cx.set_window_appearance(Some(if mode == ThemeMode::Dark {
        WindowAppearance::Dark
    } else {
        WindowAppearance::Light
    }));
    change_theme(mode, window, cx);
    Theme::update(cx, |theme| {
        theme.colors = colors(p);
        theme.highlight_theme = Arc::new(HighlightTheme {
            name: "Omarchy".into(),
            appearance: mode,
            style,
        });
    });
    Ok(())
}

pub(super) fn colors(p: &Palette) -> ThemeColor {
    let bg = color(p.background);
    let fg = color(p.foreground);
    let chrome = color(p.lighter_background);
    let muted = color(p.muted);
    let border = color(p.background.mix(p.foreground, 20));
    let hover = color(p.background.mix(p.accent, 12));
    let selected = color(p.selection);
    let selected_fg = color(p.selection_foreground);
    let focus = color(p.accent);
    let on_accent = color(contrast(p.accent));
    let mut c = ThemeColor::default();
    macro_rules! assign {
        ($value:expr; $($field:ident),+ $(,)?) => { $(c.$field = $value;)+ };
    }
    assign!(bg; background, accordion, list, table, tab_active, scrollbar);
    assign!(fg; foreground, button_foreground, group_box_foreground, popover_foreground,
        secondary_foreground, sidebar_foreground, tab_active_foreground,
        table_head_foreground, table_foot_foreground);
    assign!(chrome; button, group_box, description_list_label, list_head, muted,
        popover, secondary, sidebar, skeleton, switch, tab, tab_bar, tab_bar_segmented,
        table_head, table_foot, title_bar, status_bar);
    assign!(muted; muted_foreground, description_list_label_foreground, tab_foreground,
        scrollbar_thumb, switch_thumb);
    assign!(border; border, input, sidebar_border, table_row_border, title_bar_border,
        status_bar_border, window_border, chart_grid);
    assign!(hover; button_hover, list_hover, secondary_hover, table_hover, list_even, table_even);
    assign!(selected; accent, button_active, list_active, secondary_active, selection,
        sidebar_accent, table_active);
    assign!(selected_fg; accent_foreground, sidebar_accent_foreground);
    assign!(focus; caret, drag_border, list_active_border, primary, progress_bar, ring,
        sidebar_primary, slider_bar, slider_thumb, table_active_border, button_primary);
    assign!(on_accent; primary_foreground, sidebar_primary_foreground, button_primary_foreground);
    assign!(color(p.accent.mix(p.foreground, 15)); primary_hover, button_primary_hover);
    assign!(color(p.accent.mix(p.background, 15)); primary_active, button_primary_active);
    assign!(chrome; button_secondary);
    assign!(fg; button_secondary_foreground);
    assign!(hover; button_secondary_hover);
    assign!(selected; button_secondary_active);
    assign!(color(p.blue); info, button_info, link, chart_1, blue);
    assign!(color(contrast(p.blue)); info_foreground, button_info_foreground);
    assign!(color(p.blue.mix(p.foreground, 15)); info_hover, button_info_hover, link_hover);
    assign!(color(p.blue.mix(p.background, 15)); info_active, button_info_active, link_active);
    assign!(color(p.red); danger, button_danger, red, chart_bearish);
    assign!(color(contrast(p.red)); danger_foreground, button_danger_foreground);
    assign!(color(p.red.mix(p.foreground, 15)); danger_hover, button_danger_hover);
    assign!(color(p.red.mix(p.background, 15)); danger_active, button_danger_active);
    assign!(color(p.green); success, button_success, green, chart_bullish, chart_2);
    assign!(color(contrast(p.green)); success_foreground, button_success_foreground);
    assign!(color(p.green.mix(p.foreground, 15)); success_hover, button_success_hover);
    assign!(color(p.green.mix(p.background, 15)); success_active, button_success_active);
    assign!(color(p.yellow); warning, button_warning, yellow, chart_3);
    assign!(color(contrast(p.yellow)); warning_foreground, button_warning_foreground);
    assign!(color(p.yellow.mix(p.foreground, 15)); warning_hover, button_warning_hover);
    assign!(color(p.yellow.mix(p.background, 15)); warning_active, button_warning_active);
    assign!(color(p.magenta); magenta, chart_4);
    assign!(color(p.cyan); cyan, chart_5);
    c.red_light = color(p.bright_red);
    c.green_light = color(p.bright_green);
    c.yellow_light = color(p.bright_yellow);
    c.blue_light = color(p.bright_blue);
    c.magenta_light = color(p.bright_magenta);
    c.cyan_light = color(p.bright_cyan);
    c.scrollbar_thumb_hover = focus;
    c.drop_target = focus.alpha(0.2);
    c.overlay = color(p.darker_background).alpha(0.65);
    c
}

fn contrast(rgb: Rgb) -> Rgb {
    let value = rgb.0;
    // Relative channel weighting avoids white labels on bright yellow/green.
    if (value >> 16) * 299 + ((value >> 8) & 255) * 587 + (value & 255) * 114 > 128_000 {
        Rgb(0x000000)
    } else {
        Rgb(0xffffff)
    }
}

fn editor_style(p: &Palette) -> Result<HighlightThemeStyle, String> {
    // Kit's ThemeStyle fields are private. Its public data-only theme loader is
    // the supported route to syntax colors; a local registry never changes the
    // application's registered System/Dark/Light themes or installs font assets.
    let syntax = [
        ("attribute", p.yellow),
        ("boolean", p.magenta),
        ("comment", p.muted),
        ("comment_doc", p.muted),
        ("constant", p.yellow),
        ("constructor", p.blue),
        ("embedded", p.foreground),
        ("emphasis", p.foreground),
        ("emphasis.strong", p.bright_foreground),
        ("enum", p.cyan),
        ("function", p.blue),
        ("hint", p.cyan),
        ("keyword", p.magenta),
        ("label", p.yellow),
        ("link_text", p.blue),
        ("link_uri", p.cyan),
        ("number", p.yellow),
        ("operator", p.cyan),
        ("predictive", p.muted),
        ("preproc", p.magenta),
        ("primary", p.foreground),
        ("property", p.cyan),
        ("punctuation", p.foreground),
        ("string", p.green),
        ("string.escape", p.cyan),
        ("string.regex", p.yellow),
        ("string.special", p.cyan),
        ("tag", p.red),
        ("text.code.span", p.green),
        ("text.literal", p.green),
        ("title", p.blue),
        ("type", p.cyan),
        ("variable", p.foreground),
        ("variable.special", p.red),
        ("variant", p.cyan),
    ]
    .into_iter()
    .map(|(name, value)| format!("\"{name}\":{{\"color\":\"#{:06x}\"}}", value.0))
    .collect::<Vec<_>>()
    .join(",");
    let document = [
        r#"{"themes":[{"name":"Omarchy","highlight":{"syntax":{"#,
        &syntax,
        "}}}]}",
    ]
    .concat();
    let mut registry = ThemeRegistry::default();
    registry.load_themes_from_str(&document).map_err(|_| {
        "Omarchy editor palette could not be applied. Using System appearance.".to_string()
    })?;
    let mut style = registry
        .themes()
        .values()
        .next()
        .and_then(|config| config.highlight.clone())
        .ok_or_else(|| "Omarchy editor palette is missing. Using System appearance.".to_string())?;
    style.editor_background = Some(color(p.background));
    style.editor_foreground = Some(color(p.foreground));
    style.editor_active_line = Some(color(p.background.mix(p.accent, 8)));
    style.editor_line_number = Some(color(p.muted));
    style.editor_active_line_number = Some(color(p.accent));
    style.editor_invisible = Some(color(p.muted));
    style.editor_gutter_background = Some(color(p.background));
    Ok(style)
}
