use drift_app::{
    gui_preferences::PreferencesWriter,
    logging::{Logger, Options},
};
use drift_core::{
    gui_preferences::{GuiPreferenceChange, GuiPreferences, ThemePreference, WindowPreference},
    store::Store,
};
use std::{fs, path::Path, time::Duration};
use tokio::sync::watch;

async fn failed(receiver: &mut watch::Receiver<Option<String>>) -> String {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let Some(warning) = receiver.borrow_and_update().clone() {
                return warning;
            }
            receiver.changed().await.unwrap();
        }
    })
    .await
    .expect("preference worker did not report its failure")
}

async fn next_outcome(receiver: &mut watch::Receiver<Option<String>>) -> Option<String> {
    tokio::time::timeout(Duration::from_secs(60), receiver.changed())
        .await
        .expect("preference worker did not report its batch outcome")
        .expect("preference worker warning channel closed");
    receiver.borrow_and_update().clone()
}

fn lock_store(path: &Path) -> fs::File {
    fs::create_dir_all(path).unwrap();
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.join("write.lock"))
        .unwrap();
    file.lock().unwrap();
    file
}

#[test]
fn opening_and_finishing_an_idle_writer_never_touches_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not-created");
    let writer = PreferencesWriter::open(Store::new(path.clone()), Logger::default()).unwrap();
    assert!(!path.exists());
    assert!(writer.failures().borrow().is_none());
    writer.finish().unwrap();
    writer.finish().unwrap();
    assert!(!path.exists());
}

#[test]
fn flood_and_final_drain_save_latest_values_from_all_clones() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
    let clone = writer.clone();
    for index in 0..20_000 {
        clone.queue(GuiPreferenceChange::Window(WindowPreference {
            width: 800.0 + (index % 500) as f32,
            height: 600.0,
            maximized: false,
        }));
        writer.queue(GuiPreferenceChange::BrowserSplit(Some(0.4)));
    }
    let window = WindowPreference {
        width: 1300.0,
        height: 900.0,
        maximized: true,
    };
    writer.queue(GuiPreferenceChange::Theme(ThemePreference::System));
    writer.queue(GuiPreferenceChange::Window(window));
    clone.queue(GuiPreferenceChange::BrowserSplit(None));
    clone.queue(GuiPreferenceChange::ComparisonSplit(Some(0.65)));
    clone.finish().unwrap();
    assert_eq!(
        store.gui_preferences().unwrap(),
        GuiPreferences {
            theme: ThemePreference::System,
            window,
            panes: drift_core::gui_preferences::PanePreferences {
                browser: None,
                comparison: Some(0.65),
            },
        }
    );
    assert!(writer.failures().borrow().is_none());
    writer.finish().unwrap();
}

#[test]
fn independent_core_patch_and_unknown_fields_survive_app_save() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    fs::write(
        dir.path().join("gui.toml"),
        "theme = 'system'\nfuture = 'preserve-me'\n[window]\nwidth = 1200.0\nfuture = 'window-field'\n[panes]\nfuture = 'pane-field'\n",
    )
    .unwrap();
    let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
    // The independent instance changes fields after App opens. An App snapshot
    // write would revert them; a field patch must re-read under Core's lock.
    let independent = Store::new(dir.path().into());
    std::thread::spawn(move || {
        independent
            .update_gui_preferences(&[
                GuiPreferenceChange::Theme(ThemePreference::Light),
                GuiPreferenceChange::ComparisonSplit(Some(0.75)),
            ])
            .unwrap();
    })
    .join()
    .unwrap();
    writer.queue(GuiPreferenceChange::BrowserSplit(Some(0.3)));
    writer.finish().unwrap();
    let saved = store.gui_preferences().unwrap();
    assert_eq!(saved.theme, ThemePreference::Light);
    assert_eq!(saved.window.width, 1200.0);
    assert_eq!(saved.panes.browser, Some(0.3));
    assert_eq!(saved.panes.comparison, Some(0.75));
    let text = fs::read_to_string(dir.path().join("gui.toml")).unwrap();
    for unknown in ["preserve-me", "window-field", "pane-field"] {
        assert!(text.contains(unknown));
    }
}

#[tokio::test]
async fn corrupt_first_load_is_retained_and_reported_safely_through_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let private = "private-password-secret";
    let corrupt = format!("theme = '{private}'\n[broken = '");
    fs::write(dir.path().join("gui.toml"), &corrupt).unwrap();
    assert!(store.gui_preferences().is_err());
    let log_path = dir.path().join("diagnostic.log");
    let logger = Logger::open(Options {
        path: log_path.clone(),
        debug: false,
    })
    .unwrap();
    let writer = PreferencesWriter::open(store, logger.clone()).unwrap();
    let mut failures = writer.failures();
    writer.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
    let warning = failed(&mut failures).await;
    assert!(warning.contains("decoded") || warning.contains("invalid"));
    assert!(!warning.contains(private));
    assert!(!warning.contains(&dir.path().display().to_string()));
    assert_eq!(writer.finish().unwrap_err().to_string(), warning);
    assert_eq!(writer.finish().unwrap_err().to_string(), warning);
    assert_eq!(
        fs::read_to_string(dir.path().join("gui.toml")).unwrap(),
        corrupt
    );
    logger.finish().unwrap();
    let log = fs::read_to_string(log_path).unwrap();
    assert!(log.contains("GUI preferences save failed"));
    assert!(log.contains("error_kind=\"config_decode\"") || log.contains("error_kind=\"invalid\""));
    assert!(!log.contains(private));
    assert!(!log.contains("[broken"));
    assert!(!log.contains(&dir.path().display().to_string()));
}

#[tokio::test]
async fn releasing_busy_lock_does_not_retry_even_during_final_drain() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let lock = lock_store(dir.path());
    let writer = PreferencesWriter::open(store, Logger::default()).unwrap();
    let mut failures = writer.failures();
    writer.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
    let warning = failed(&mut failures).await;
    assert!(warning.contains("busy"));
    drop(lock);
    assert_eq!(
        writer.failures().borrow().as_deref(),
        Some(warning.as_str())
    );
    assert_eq!(writer.finish().unwrap_err().to_string(), warning);
    assert!(!dir.path().join("gui.toml").exists());
}

#[tokio::test]
async fn failed_theme_is_not_retried_or_forgotten_by_a_successful_window_batch() {
    for retry_theme in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        let lock = lock_store(dir.path());
        let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
        let mut failures = writer.failures();
        writer.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
        let warning = failed(&mut failures).await;
        assert!(warning.contains("busy"));
        drop(lock);
        let window = WindowPreference {
            width: 1300.0,
            height: 900.0,
            maximized: true,
        };
        writer.queue(GuiPreferenceChange::Window(window));
        assert_eq!(
            next_outcome(&mut failures).await.as_deref(),
            Some(warning.as_str())
        );
        assert_eq!(
            writer.failures().borrow().as_deref(),
            Some(warning.as_str())
        );
        let saved = store.gui_preferences().unwrap();
        assert_eq!(saved.theme, ThemePreference::System);
        assert_eq!(saved.window, window);
        if retry_theme {
            // The same UI selection must be explicitly queued again to retry.
            writer.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
            assert!(next_outcome(&mut failures).await.is_none());
            writer.finish().unwrap();
            assert_eq!(
                store.gui_preferences().unwrap().theme,
                ThemePreference::Dark
            );
        } else {
            assert_eq!(writer.finish().unwrap_err().to_string(), warning);
            assert_eq!(writer.finish().unwrap_err().to_string(), warning);
            assert_eq!(
                store.gui_preferences().unwrap().theme,
                ThemePreference::System
            );
        }
    }
}

#[tokio::test]
async fn a_successful_other_pane_does_not_resolve_a_failed_pane() {
    for failed_browser in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        let lock = lock_store(dir.path());
        let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
        let mut failures = writer.failures();
        writer.queue(if failed_browser {
            GuiPreferenceChange::BrowserSplit(Some(0.35))
        } else {
            GuiPreferenceChange::ComparisonSplit(Some(0.35))
        });
        let warning = failed(&mut failures).await;
        drop(lock);
        writer.queue(if failed_browser {
            GuiPreferenceChange::ComparisonSplit(Some(0.7))
        } else {
            GuiPreferenceChange::BrowserSplit(Some(0.7))
        });
        assert_eq!(
            next_outcome(&mut failures).await.as_deref(),
            Some(warning.as_str())
        );
        assert_eq!(writer.finish().unwrap_err().to_string(), warning);
        let saved = store.gui_preferences().unwrap();
        if failed_browser {
            assert_eq!(saved.panes.browser, None);
            assert_eq!(saved.panes.comparison, Some(0.7));
        } else {
            assert_eq!(saved.panes.comparison, None);
            assert_eq!(saved.panes.browser, Some(0.7));
        }
    }
}

#[tokio::test]
async fn retrying_one_failed_field_leaves_the_other_failure_until_an_explicit_reset() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
    let mut failures = writer.failures();
    writer.queue(GuiPreferenceChange::BrowserSplit(Some(f32::NAN)));
    let warning = failed(&mut failures).await;
    assert!(warning.contains("invalid"));
    let lock = lock_store(dir.path());
    writer.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
    let busy = next_outcome(&mut failures).await.unwrap();
    assert!(busy.contains("busy"));
    assert_ne!(busy, warning);
    drop(lock);
    writer.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
    assert_eq!(
        next_outcome(&mut failures).await.as_deref(),
        Some(warning.as_str())
    );
    let saved = store.gui_preferences().unwrap();
    assert_eq!(saved.theme, ThemePreference::Dark);
    assert_eq!(saved.panes.browser, None);
    writer.queue(GuiPreferenceChange::ComparisonSplit(Some(0.6)));
    assert_eq!(
        next_outcome(&mut failures).await.as_deref(),
        Some(warning.as_str())
    );
    // A reset is a real field patch, even when the persisted value is already None.
    writer.queue(GuiPreferenceChange::BrowserSplit(None));
    assert!(next_outcome(&mut failures).await.is_none());
    writer.finish().unwrap();
    let saved = store.gui_preferences().unwrap();
    assert_eq!(saved.panes.browser, None);
    assert_eq!(saved.panes.comparison, Some(0.6));
}

#[test]
fn invalid_batch_is_not_saved_and_shutdown_error_is_static() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let writer = PreferencesWriter::open(store, Logger::default()).unwrap();
    writer.queue(GuiPreferenceChange::BrowserSplit(Some(f32::INFINITY)));
    let error = writer.finish().unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(error.to_string().contains("invalid"));
    assert!(!dir.path().join("gui.toml").exists());
}

#[test]
fn requests_after_finish_are_rejected_and_visible_to_late_subscribers() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
    let clone = writer.clone();
    writer.queue(GuiPreferenceChange::Theme(ThemePreference::Light));
    writer.finish().unwrap();
    clone.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
    let warning = clone.failures().borrow().clone().unwrap();
    assert!(warning.contains("closed"));
    assert_eq!(clone.finish().unwrap_err().to_string(), warning);
    assert_eq!(
        store.gui_preferences().unwrap().theme,
        ThemePreference::Light
    );
}
