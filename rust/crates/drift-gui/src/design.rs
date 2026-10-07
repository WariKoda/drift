//! Presentation tokens for the browser and project form; native controls own interaction.
use gpui_kit::Styled;
use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{App, Hsla, Pixels, Rems, rems};

/// Kit's Root sets rem_size from this same font_size; do not scale twice.
pub fn metric(value: f32, cx: &App) -> Pixels {
    cx.theme().font_size * (value / 16.)
}

/// Small Kit buttons already scale their text, padding, gap and icons in rems.
/// Keep that public sizing path: Button overrides a supplied icon's size.
pub fn control(button: Button, cx: &App) -> Button {
    button
        .small()
        .ghost()
        .h(metric(size::CONTROL, cx))
        .rounded(metric(size::RADIUS, cx))
        .min_w_0()
        .max_w_full()
        .overflow_hidden()
}

pub mod space {
    pub const TIGHT: f32 = 4.;
    pub const CONTROL: f32 = 8.;
    pub const PANEL: f32 = 12.;
    pub const INDENT: f32 = 16.;
}

pub mod size {
    pub const ROW: f32 = 24.;
    pub const CONTROL: f32 = 28.;
    pub const HEADER: f32 = 36.;
    pub const ICON: f32 = 16.;
    pub const DISCLOSURE: f32 = 20.;
    pub const FILTER: f32 = 300.;
    pub const PROJECT_FORM: f32 = 560.;
    pub const RADIUS: f32 = 4.;
}

#[derive(Clone, Copy)]
pub enum Text {
    Body,
    Metadata,
    Title,
}
impl Text {
    pub fn rems(self) -> Rems {
        rems(match self {
            Self::Body => 14. / 16.,
            Self::Metadata => 12. / 16.,
            Self::Title => 1.,
        })
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub canvas: Hsla,
    pub chrome: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub border: Hsla,
    pub selected: Hsla,
    pub hover: Hsla,
    pub focus: Hsla,
}
impl Palette {
    pub fn current(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            canvas: theme.background,
            chrome: theme.list_head,
            text: theme.foreground,
            muted: theme.muted_foreground,
            border: theme.border,
            selected: theme.list_active,
            hover: theme.list_hover,
            focus: theme.ring,
        }
    }
}

#[cfg(test)]
mod tests;
