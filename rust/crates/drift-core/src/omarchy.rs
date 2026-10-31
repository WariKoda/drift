//! Validated, data-only subset of Omarchy's semantic/ANSI colors.toml resolver.
use std::collections::BTreeMap;

pub const MAX_PALETTE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u32);
impl Rgb {
    pub fn mix(self, other: Self, percent: u32) -> Self {
        let channel = |shift: u32| {
            (((self.0 >> shift) & 255) * (100 - percent.min(100))
                + ((other.0 >> shift) & 255) * percent.min(100)
                + 50)
                / 100
        };
        Self((channel(16) << 16) | (channel(8) << 8) | channel(0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteMode {
    Dark,
    Light,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    pub mode: PaletteMode,
    pub background: Rgb,
    pub foreground: Rgb,
    pub dark_background: Rgb,
    pub darker_background: Rgb,
    pub lighter_background: Rgb,
    pub bright_foreground: Rgb,
    pub muted: Rgb,
    pub selection: Rgb,
    pub selection_foreground: Rgb,
    pub accent: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
    pub bright_red: Rgb,
    pub bright_green: Rgb,
    pub bright_yellow: Rgb,
    pub bright_blue: Rgb,
    pub bright_magenta: Rgb,
    pub bright_cyan: Rgb,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PaletteError(pub String);

impl Palette {
    pub fn parse(text: &str, light_marker: bool) -> Result<Self, PaletteError> {
        let invalid = |message: &str| PaletteError(message.into());
        if text.len() > MAX_PALETTE_BYTES {
            return Err(invalid("palette exceeds 64 KiB"));
        }
        let table: toml::Table =
            toml::from_str(text).map_err(|_| invalid("invalid palette TOML"))?;
        let keys = [
            "background",
            "bg",
            "foreground",
            "fg",
            "dark_background",
            "dark_bg",
            "darker_background",
            "darker_bg",
            "lighter_background",
            "lighter_bg",
            "dark_foreground",
            "dark_fg",
            "light_foreground",
            "light_fg",
            "bright_foreground",
            "bright_fg",
            "muted",
            "accent",
            "selection",
            "selection_background",
            "selection_foreground",
            "cursor",
            "red",
            "green",
            "yellow",
            "blue",
            "cyan",
            "magenta",
            "purple",
            "orange",
            "brown",
            "bright_red",
            "bright_green",
            "bright_yellow",
            "bright_blue",
            "bright_cyan",
            "bright_magenta",
            "bright_purple",
        ];
        let mut colors = BTreeMap::new();
        for key in keys
            .into_iter()
            .map(str::to_owned)
            .chain((0..16).map(|n| format!("color{n}")))
        {
            if let Some(value) = table.get(&key) {
                let hex = value.as_str().and_then(|value| value.strip_prefix('#'));
                let rgb = hex
                    .filter(|hex| hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .ok_or_else(|| {
                        PaletteError(format!("invalid color: {key} (expected #RRGGBB)"))
                    })?;
                colors.insert(key, Rgb(rgb));
            }
        }
        let get = |names: &[&str]| names.iter().find_map(|key| colors.get(*key).copied());
        let required = |names: &[&str]| {
            get(names).ok_or_else(|| PaletteError(format!("missing color: {}", names[0])))
        };
        let background = required(&["background", "bg", "color0"])?;
        let foreground = required(&["foreground", "fg", "color7"])?;
        let red = required(&["red", "color1"])?;
        let green = required(&["green", "color2"])?;
        let yellow = required(&["yellow", "color3"])?;
        let blue = required(&["blue", "color4"])?;
        let magenta = required(&["magenta", "color5", "purple"])?;
        let cyan = required(&["cyan", "color6"])?;
        let bright_foreground =
            get(&["bright_foreground", "bright_fg", "color15"]).unwrap_or(foreground);
        let muted = get(&["muted", "color8", "dark_foreground", "dark_fg"]).unwrap_or(foreground);
        let selection = get(&["selection", "selection_background", "color8"]).unwrap_or(background);
        let mode = match table.get("mode").or_else(|| table.get("theme_type")) {
            Some(value) => match value.as_str() {
                Some("dark") => PaletteMode::Dark,
                Some("light") => PaletteMode::Light,
                _ => return Err(invalid("invalid palette mode (expected dark or light)")),
            },
            None if light_marker => PaletteMode::Light,
            None => {
                let rgb = background.0;
                if (rgb >> 16) + ((rgb >> 8) & 255) + (rgb & 255) > 382 {
                    PaletteMode::Light
                } else {
                    PaletteMode::Dark
                }
            }
        };
        let bright = |names: &[&str], base: Rgb| get(names).unwrap_or(base.mix(Rgb(0xffffff), 20));
        Ok(Self {
            mode,
            background,
            foreground,
            bright_foreground,
            muted,
            selection,
            selection_foreground: get(&["selection_foreground"]).unwrap_or(bright_foreground),
            accent: get(&["accent"]).unwrap_or(blue),
            dark_background: get(&["dark_background", "dark_bg"])
                .unwrap_or(background.mix(Rgb(0), 25)),
            darker_background: get(&["darker_background", "darker_bg"])
                .unwrap_or(background.mix(Rgb(0), 50)),
            lighter_background: get(&["lighter_background", "lighter_bg"]).unwrap_or(background),
            red,
            green,
            yellow,
            blue,
            magenta,
            cyan,
            bright_red: bright(&["bright_red", "color9"], red),
            bright_green: bright(&["bright_green", "color10"], green),
            bright_yellow: bright(&["bright_yellow", "color11"], yellow),
            bright_blue: bright(&["bright_blue", "color12"], blue),
            bright_magenta: bright(&["bright_magenta", "color13", "bright_purple"], magenta),
            bright_cyan: bright(&["bright_cyan", "color14"], cyan),
        })
    }
}

#[cfg(test)]
mod tests;
