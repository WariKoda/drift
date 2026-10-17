//! GUI-only preferences, independent of host configuration and runtime defaults.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct GuiPreferences {
    pub theme: ThemePreference,
    pub window: WindowPreference,
    pub panes: PanePreferences,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct WindowPreference {
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

impl Default for WindowPreference {
    fn default() -> Self {
        Self {
            width: 1100.0,
            height: 720.0,
            maximized: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct PanePreferences {
    pub browser: Option<f32>,
    pub comparison: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GuiPreferenceChange {
    Theme(ThemePreference),
    Window(WindowPreference),
    BrowserSplit(Option<f32>),
    ComparisonSplit(Option<f32>),
}

impl GuiPreferences {
    pub fn validate(&self) -> Result<()> {
        for size in [self.window.width, self.window.height] {
            if !size.is_finite() || size <= 0.0 || size > 16384.0 {
                return Err(Error::Invalid(
                    "GUI window dimensions must be finite, positive and at most 16384".into(),
                ));
            }
        }
        for ratio in [self.panes.browser, self.panes.comparison]
            .into_iter()
            .flatten()
        {
            if !ratio.is_finite() || ratio <= 0.0 || ratio >= 1.0 {
                return Err(Error::Invalid(
                    "GUI pane ratios must be finite and strictly between 0 and 1".into(),
                ));
            }
        }
        Ok(())
    }

    /// Apply one field change; callers validate the resulting preferences.
    pub fn apply(&mut self, change: GuiPreferenceChange) {
        match change {
            GuiPreferenceChange::Theme(theme) => self.theme = theme,
            GuiPreferenceChange::Window(window) => self.window = window,
            GuiPreferenceChange::BrowserSplit(ratio) => self.panes.browser = ratio,
            GuiPreferenceChange::ComparisonSplit(ratio) => self.panes.comparison = ratio,
        }
    }
}
