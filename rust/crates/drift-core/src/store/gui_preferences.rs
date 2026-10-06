use super::Store;
use crate::{
    error::{Error, Result},
    gui_preferences::{GuiPreferenceChange, GuiPreferences, ThemePreference},
};
use std::{fs, io::Read, os::unix::fs::OpenOptionsExt};
use toml::{Table, Value};

const MAX_GUI_BYTES: u64 = 1024 * 1024;

impl Store {
    /// Missing GUI preferences use defaults without creating the store or lock.
    pub fn gui_preferences(&self) -> Result<GuiPreferences> {
        let (_, preferences) = self.read_gui_preferences()?;
        Ok(preferences)
    }

    /// Patch the freshest valid document under the shared configuration lock.
    /// Lock release errors follow the store's semantics, even after a write.
    pub fn update_gui_preferences(
        &self,
        changes: &[GuiPreferenceChange],
    ) -> Result<GuiPreferences> {
        if changes.is_empty() {
            return self.gui_preferences();
        }
        self.with_lock(|| {
            let (mut document, before) = self.read_gui_preferences()?;
            let mut preferences = before.clone();
            for &change in changes {
                preferences.apply(change);
            }
            preferences.validate()?;
            if preferences == before {
                return Ok(preferences);
            }
            if preferences.theme != before.theme {
                let theme = match preferences.theme {
                    ThemePreference::System => "system",
                    ThemePreference::Dark => "dark",
                    ThemePreference::Light => "light",
                };
                document.insert("theme".into(), Value::String(theme.into()));
            }
            if preferences.window != before.window {
                let window = document
                    .entry("window")
                    .or_insert_with(|| Value::Table(Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| Error::Invalid("invalid GUI window table".into()))?;
                if preferences.window.width != before.window.width {
                    window.insert(
                        "width".into(),
                        Value::Float(preferences.window.width.into()),
                    );
                }
                if preferences.window.height != before.window.height {
                    window.insert(
                        "height".into(),
                        Value::Float(preferences.window.height.into()),
                    );
                }
                if preferences.window.maximized != before.window.maximized {
                    window.insert(
                        "maximized".into(),
                        Value::Boolean(preferences.window.maximized),
                    );
                }
            }
            if preferences.panes != before.panes {
                let panes = document
                    .entry("panes")
                    .or_insert_with(|| Value::Table(Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| Error::Invalid("invalid GUI panes table".into()))?;
                for (field, old, new) in [
                    ("browser", before.panes.browser, preferences.panes.browser),
                    (
                        "comparison",
                        before.panes.comparison,
                        preferences.panes.comparison,
                    ),
                ] {
                    if old != new {
                        match new {
                            Some(ratio) => {
                                panes.insert(field.into(), Value::Float(ratio.into()));
                            }
                            None => {
                                panes.remove(field);
                            }
                        }
                    }
                }
            }
            self.write(&self.dir.join("gui.toml"), &document)?;
            Ok(preferences)
        })
    }

    fn read_gui_preferences(&self) -> Result<(Table, GuiPreferences)> {
        let path = self.dir.join("gui.toml");
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Table::new(), GuiPreferences::default()));
            }
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() {
            return Err(Error::Invalid(
                "GUI preferences must be a regular file".into(),
            ));
        }
        if metadata.len() > MAX_GUI_BYTES {
            return Err(Error::Invalid(
                "GUI preferences exceed the size limit".into(),
            ));
        }
        // Recheck the opened file, and never block if the path became a FIFO
        // between the metadata check and open. Bound reads even if it grows.
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NONBLOCK)
            .open(&path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(Error::Invalid(
                "GUI preferences must be a regular file".into(),
            ));
        }
        if metadata.len() > MAX_GUI_BYTES {
            return Err(Error::Invalid(
                "GUI preferences exceed the size limit".into(),
            ));
        }
        let mut bytes = Vec::new();
        file.take(MAX_GUI_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_GUI_BYTES {
            return Err(Error::Invalid(
                "GUI preferences exceed the size limit".into(),
            ));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::Invalid("GUI preferences must be UTF-8".into()))?;
        let document: Table = toml::from_str(text)
            .map_err(|_| Error::Invalid("invalid GUI preferences TOML".into()))?;
        if document.get("theme").is_some_and(|value| !value.is_str())
            || ["window", "panes"]
                .into_iter()
                .any(|field| document.get(field).is_some_and(|value| !value.is_table()))
        {
            return Err(Error::Invalid("invalid GUI preference fields".into()));
        }
        let preferences: GuiPreferences = Value::Table(document.clone())
            .try_into()
            .map_err(|_| Error::Invalid("invalid GUI preference fields".into()))?;
        preferences.validate()?;
        Ok((document, preferences))
    }
}
