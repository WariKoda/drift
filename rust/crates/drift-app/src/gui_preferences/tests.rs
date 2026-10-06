use super::*;
use drift_core::gui_preferences::{ThemePreference, WindowPreference};
use std::sync::Barrier;

#[test]
fn flood_keeps_only_four_latest_fields_and_does_not_extend_max_wait() {
    let mut pending = Pending::default();
    pending.push(GuiPreferenceChange::Theme(ThemePreference::Dark));
    let first = pending.first.unwrap();
    for index in 0..100_000 {
        pending.push(GuiPreferenceChange::Theme(ThemePreference::Light));
        pending.push(GuiPreferenceChange::Window(WindowPreference {
            width: 1100.0 + (index % 100) as f32,
            ..WindowPreference::default()
        }));
        pending.push(GuiPreferenceChange::BrowserSplit(Some(0.4)));
        pending.push(GuiPreferenceChange::ComparisonSplit(Some(0.6)));
    }
    // None is a requested reset, not an empty queue slot.
    pending.push(GuiPreferenceChange::BrowserSplit(None));
    pending.push(GuiPreferenceChange::ComparisonSplit(None));
    assert_eq!(pending.changes.into_iter().flatten().count(), 4);
    assert_eq!(pending.first, Some(first));
    assert_eq!(
        pending.deadline(),
        Some((pending.latest.unwrap() + DEBOUNCE).min(first + MAX_WAIT))
    );
    assert!(pending.deadline().unwrap() <= first + MAX_WAIT);
    assert_eq!(
        pending.changes,
        [
            Some(GuiPreferenceChange::Theme(ThemePreference::Light)),
            Some(GuiPreferenceChange::Window(WindowPreference {
                width: 1199.0,
                ..WindowPreference::default()
            })),
            Some(GuiPreferenceChange::BrowserSplit(None)),
            Some(GuiPreferenceChange::ComparisonSplit(None)),
        ]
    );
    let drained = std::mem::take(&mut pending);
    assert!(pending.deadline().is_none());
    assert_eq!(drained.changes.into_iter().flatten().count(), 4);
}

#[test]
fn warnings_never_include_untrusted_error_text() {
    let secret = "private-path password token\nTOML source";
    for error in [
        Error::Invalid(secret.into()),
        Error::Connection(secret.into()),
        Error::Conflict(secret.into()),
        Error::Io(io::Error::other(secret)),
        Error::Busy,
    ] {
        let warning = safe_warning(&error);
        assert!(warning.starts_with("GUI preference save could not be confirmed:"));
        assert!(!warning.contains(secret));
        assert!(!warning.contains('\n'));
    }
}

#[test]
fn core_write_between_queue_and_drain_preserves_unpatched_fields() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
    // Hold only the queue mutex to order two real, independently owned writers.
    // The worker cannot take this patch until the Core thread has committed.
    let mut state = writer.0.shared.state.lock().unwrap();
    state
        .pending
        .push(GuiPreferenceChange::BrowserSplit(Some(0.35)));
    let core = std::thread::spawn(move || {
        store
            .update_gui_preferences(&[
                GuiPreferenceChange::Theme(ThemePreference::Dark),
                GuiPreferenceChange::ComparisonSplit(Some(0.7)),
            ])
            .unwrap();
        store
    });
    let store = core.join().unwrap();
    drop(state);
    writer.finish().unwrap();
    let saved = store.gui_preferences().unwrap();
    assert_eq!(saved.theme, ThemePreference::Dark);
    assert_eq!(saved.panes.browser, Some(0.35));
    assert_eq!(saved.panes.comparison, Some(0.7));
}

#[test]
fn concurrent_finish_calls_join_the_same_worker_and_retain_failure() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let writer = PreferencesWriter::open(store, Logger::default()).unwrap();
    writer.queue(GuiPreferenceChange::BrowserSplit(Some(f32::NAN)));
    let barrier = Arc::new(Barrier::new(5));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let writer = writer.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                writer.finish().unwrap_err().to_string()
            })
        })
        .collect();
    barrier.wait();
    let expected = writer.finish().unwrap_err().to_string();
    for worker in workers {
        assert_eq!(worker.join().unwrap(), expected);
    }
    assert_eq!(
        writer.failures().borrow().as_deref(),
        Some(expected.as_str())
    );
}

#[tokio::test]
async fn an_invalid_batch_marks_every_field_until_each_is_explicitly_saved() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
    let mut failures = writer.failures();
    let window = WindowPreference {
        width: 1400.0,
        ..WindowPreference::default()
    };
    // Hold the queue lock so all four fields belong to one rejected real batch.
    {
        let mut state = writer.0.shared.state.lock().unwrap();
        state
            .pending
            .push(GuiPreferenceChange::Theme(ThemePreference::Dark));
        state.pending.push(GuiPreferenceChange::Window(window));
        state
            .pending
            .push(GuiPreferenceChange::BrowserSplit(Some(f32::NAN)));
        state
            .pending
            .push(GuiPreferenceChange::ComparisonSplit(Some(0.6)));
        state.pending.latest = Some(Instant::now() - MAX_WAIT);
    }
    writer.0.shared.wake.notify_one();
    tokio::time::timeout(Duration::from_secs(60), failures.changed())
        .await
        .expect("invalid batch did not complete")
        .unwrap();
    let warning = failures.borrow_and_update().clone().unwrap();
    assert!(warning.contains("invalid"));
    assert!(!dir.path().join("gui.toml").exists());
    {
        let state = writer.0.shared.state.lock().unwrap();
        assert_eq!(
            state.unsaved,
            [Some(safe_warning(&Error::Invalid(String::new()))); 4]
        );
        assert_eq!(state.pending.changes, [None; 4]);
    }
    let retries = [
        GuiPreferenceChange::Theme(ThemePreference::Dark),
        GuiPreferenceChange::Window(window),
        GuiPreferenceChange::BrowserSplit(None),
        GuiPreferenceChange::ComparisonSplit(None),
    ];
    for (index, change) in retries.into_iter().enumerate() {
        writer.queue(change);
        tokio::time::timeout(Duration::from_secs(60), failures.changed())
            .await
            .expect("explicit retry did not complete")
            .unwrap();
        let outcome = failures.borrow_and_update().clone();
        assert_eq!(outcome.as_deref(), (index < 3).then_some(warning.as_str()));
        let state = writer.0.shared.state.lock().unwrap();
        assert!(state.unsaved[..=index].iter().all(Option::is_none));
        assert!(state.unsaved[index + 1..].iter().all(Option::is_some));
    }
    writer.finish().unwrap();
    let saved = store.gui_preferences().unwrap();
    assert_eq!(saved.theme, ThemePreference::Dark);
    assert_eq!(saved.window, window);
    assert_eq!(saved.panes.browser, None);
    assert_eq!(saved.panes.comparison, None);
}

#[test]
fn rejecting_a_request_during_final_drain_is_permanent() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
    {
        let mut state = writer.0.shared.state.lock().unwrap();
        state
            .pending
            .push(GuiPreferenceChange::Theme(ThemePreference::Light));
        state.closed = true;
    }
    writer.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
    assert_eq!(writer.finish().unwrap_err().to_string(), CLOSED_WARNING);
    assert_eq!(writer.finish().unwrap_err().to_string(), CLOSED_WARNING);
    assert_eq!(writer.failures().borrow().as_deref(), Some(CLOSED_WARNING));
    assert_eq!(
        store.gui_preferences().unwrap().theme,
        ThemePreference::Light
    );
}

#[test]
fn poisoned_worker_state_is_reported_without_panicking_in_finish_or_drop() {
    let dir = tempfile::tempdir().unwrap();
    let writer = PreferencesWriter::open(Store::new(dir.path().into()), Logger::default()).unwrap();
    let shared = writer.0.shared.clone();
    let panic = std::thread::spawn(move || {
        let _state = shared.state.lock().unwrap();
        panic!("simulate a panic while holding the worker state");
    });
    assert!(panic.join().is_err());
    assert_eq!(writer.finish().unwrap_err().to_string(), WORKER_WARNING);
    assert_eq!(writer.finish().unwrap_err().to_string(), WORKER_WARNING);
    assert_eq!(writer.failures().borrow().as_deref(), Some(WORKER_WARNING));
    writer.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
    assert_eq!(writer.finish().unwrap_err().to_string(), CLOSED_WARNING);
    drop(writer);
    assert!(!dir.path().join("gui.toml").exists());
}

#[test]
fn final_handle_drop_closes_and_wakes_without_joining() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let writer = PreferencesWriter::open(store.clone(), Logger::default()).unwrap();
    writer.queue(GuiPreferenceChange::Theme(ThemePreference::Dark));
    // Retain the join handle in the test only to observe completion. Production
    // Drop leaves it detached, so transport/filesystem latency cannot delay UI.
    let worker = writer.0.worker.lock().unwrap().take().unwrap();
    let shared = writer.0.shared.clone();
    drop(writer);
    assert!(shared.state.lock().unwrap().closed);
    worker.join().unwrap();
    assert_eq!(
        store.gui_preferences().unwrap().theme,
        ThemePreference::Dark
    );
}
