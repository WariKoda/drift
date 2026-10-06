use drift_core::{
    error::Error,
    gui_preferences::{
        GuiPreferenceChange, GuiPreferences, PanePreferences, ThemePreference, WindowPreference,
    },
    store::Store,
};
use fs2::FileExt;
use std::{
    fs,
    io::Read,
    os::unix::fs::{PermissionsExt, symlink},
    sync::{Arc, Barrier, mpsc},
    time::{Duration, Instant},
};

#[test]
fn defaults_and_serde_schema_are_gui_only() {
    let expected = GuiPreferences {
        theme: ThemePreference::System,
        window: WindowPreference {
            width: 1100.0,
            height: 720.0,
            maximized: false,
        },
        panes: PanePreferences {
            browser: None,
            comparison: None,
        },
    };
    assert_eq!(GuiPreferences::default(), expected);
    expected.validate().unwrap();
    for text in ["", "[window]\n[panes]\n", "[window]\nmaximized = false\n"] {
        assert_eq!(toml::from_str::<GuiPreferences>(text).unwrap(), expected);
    }
    let partial: GuiPreferences = toml::from_str("[window]\nheight = 900\n").unwrap();
    assert_eq!(partial.window.width, 1100.0);
    assert_eq!(partial.window.height, 900.0);
    assert!(!partial.window.maximized);
    for (theme, name) in [
        (ThemePreference::System, "system"),
        (ThemePreference::Dark, "dark"),
        (ThemePreference::Light, "light"),
    ] {
        let preferences = GuiPreferences {
            theme,
            ..expected.clone()
        };
        let encoded = toml::to_string(&preferences).unwrap();
        let document: toml::Value = toml::from_str(&encoded).unwrap();
        assert_eq!(document["theme"].as_str(), Some(name));
        assert_eq!(document.as_table().unwrap().len(), 3);
        assert_eq!(document["window"].as_table().unwrap().len(), 3);
        assert_eq!(
            toml::from_str::<GuiPreferences>(&encoded).unwrap(),
            preferences
        );
    }
}

#[test]
fn apply_is_field_scoped_and_infallible_while_validation_is_explicit() {
    let mut preferences = GuiPreferences::default();
    preferences.apply(GuiPreferenceChange::Theme(ThemePreference::Dark));
    assert_eq!(preferences.window, WindowPreference::default());
    assert_eq!(preferences.panes, PanePreferences::default());
    let window = WindowPreference {
        width: 16384.0,
        height: 0.5,
        maximized: true,
    };
    preferences.apply(GuiPreferenceChange::Window(window));
    preferences.apply(GuiPreferenceChange::BrowserSplit(Some(0.25)));
    preferences.apply(GuiPreferenceChange::ComparisonSplit(Some(0.75)));
    assert_eq!(preferences.theme, ThemePreference::Dark);
    assert_eq!(preferences.window, window);
    assert_eq!(preferences.panes.browser, Some(0.25));
    assert_eq!(preferences.panes.comparison, Some(0.75));
    preferences.validate().unwrap();
    preferences.apply(GuiPreferenceChange::BrowserSplit(None));
    assert_eq!(preferences.panes.browser, None);
    assert_eq!(preferences.panes.comparison, Some(0.75));
    preferences.apply(GuiPreferenceChange::Window(WindowPreference {
        width: f32::NAN,
        ..window
    }));
    assert!(preferences.window.width.is_nan());
    assert!(matches!(preferences.validate(), Err(Error::Invalid(_))));
}

#[test]
fn dimensions_and_ratios_reject_boundaries_and_nonfinite_values() {
    for size in [
        0.0,
        -0.0,
        -1.0,
        16385.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        for width in [true, false] {
            let mut preferences = GuiPreferences::default();
            if width {
                preferences.window.width = size;
            } else {
                preferences.window.height = size;
            }
            assert!(matches!(preferences.validate(), Err(Error::Invalid(_))));
        }
    }
    for ratio in [
        0.0,
        -0.0,
        -0.1,
        1.0,
        1.1,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        for change in [
            GuiPreferenceChange::BrowserSplit(Some(ratio)),
            GuiPreferenceChange::ComparisonSplit(Some(ratio)),
        ] {
            let mut preferences = GuiPreferences::default();
            preferences.apply(change);
            assert!(matches!(preferences.validate(), Err(Error::Invalid(_))));
        }
    }
    let preferences = GuiPreferences {
        window: WindowPreference {
            width: f32::MIN_POSITIVE,
            height: 16384.0,
            maximized: false,
        },
        panes: PanePreferences {
            browser: Some(f32::MIN_POSITIVE),
            comparison: Some(0.999),
        },
        ..GuiPreferences::default()
    };
    preferences.validate().unwrap();
}

#[test]
fn absent_load_and_empty_patch_do_not_create_files_or_directories() {
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("absent");
    let store = Store::new(absent.clone());
    assert_eq!(store.gui_preferences().unwrap(), GuiPreferences::default());
    assert_eq!(
        store.update_gui_preferences(&[]).unwrap(),
        GuiPreferences::default()
    );
    assert!(!absent.exists());
    let existing = Store::new(dir.path().into());
    assert_eq!(
        existing.gui_preferences().unwrap(),
        GuiPreferences::default()
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn roundtrip_is_atomic_private_and_does_not_touch_go_shared_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    fs::create_dir(dir.path().join("projects")).unwrap();
    let unrelated = [
        (
            "config.toml",
            "# Go-owned\n[defaults]\nuser = 'deploy'\npassword = 'secret'\n",
        ),
        ("projects.toml", "# registry\nfuture = true\n"),
        (
            "projects/demo.toml",
            "# project hosts\nroot_path = '/srv'\n",
        ),
        (
            "trusted-certificates.toml",
            "# trust store\ncertificates = []\n",
        ),
    ];
    for (name, text) in unrelated {
        fs::write(dir.path().join(name), text).unwrap();
    }
    // GUI defaults must not inherit any host configuration.
    assert_eq!(store.gui_preferences().unwrap(), GuiPreferences::default());
    let desired = GuiPreferences {
        theme: ThemePreference::Light,
        window: WindowPreference {
            width: 1400.0,
            height: 950.0,
            maximized: true,
        },
        panes: PanePreferences {
            browser: Some(0.25),
            comparison: Some(0.75),
        },
    };
    assert_eq!(
        store
            .update_gui_preferences(&[
                GuiPreferenceChange::Theme(desired.theme),
                GuiPreferenceChange::Window(desired.window),
                GuiPreferenceChange::BrowserSplit(desired.panes.browser),
                GuiPreferenceChange::ComparisonSplit(desired.panes.comparison),
            ])
            .unwrap(),
        desired
    );
    assert_eq!(store.gui_preferences().unwrap(), desired);
    let gui = dir.path().join("gui.toml");
    assert_eq!(
        fs::metadata(&gui).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let old_bytes = fs::read(&gui).unwrap();
    let mut old_file = fs::File::open(&gui).unwrap();
    fs::set_permissions(&gui, fs::Permissions::from_mode(0o644)).unwrap();
    store
        .update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)])
        .unwrap();
    let mut retained = Vec::new();
    old_file.read_to_end(&mut retained).unwrap();
    assert_eq!(retained, old_bytes);
    assert_ne!(fs::read(&gui).unwrap(), old_bytes);
    assert_eq!(
        fs::metadata(&gui).unwrap().permissions().mode() & 0o777,
        0o600
    );
    for (name, text) in unrelated {
        assert_eq!(fs::read(dir.path().join(name)).unwrap(), text.as_bytes());
    }
}

#[test]
fn patches_preserve_unknown_tables_and_only_write_changed_known_fields() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let gui = dir.path().join("gui.toml");
    let original = "future = ['one', 'two']\n[plugin]\nsecret = 'opaque'\n[window]\nheight = 800\nfuture = { mode = 'keep' }\n[panes]\nbrowser = 0.25\ncomparison = 0.75\nfuture = { depth = 3 }\n";
    fs::write(&gui, original).unwrap();
    let before: toml::Value = toml::from_str(original).unwrap();
    store
        .update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)])
        .unwrap();
    let themed: toml::Value = toml::from_str(&fs::read_to_string(&gui).unwrap()).unwrap();
    assert_eq!(themed["window"], before["window"]);
    assert_eq!(themed["panes"], before["panes"]);
    store
        .update_gui_preferences(&[
            GuiPreferenceChange::Window(WindowPreference {
                height: 900.0,
                ..WindowPreference::default()
            }),
            GuiPreferenceChange::BrowserSplit(None),
        ])
        .unwrap();
    let patched: toml::Value = toml::from_str(&fs::read_to_string(&gui).unwrap()).unwrap();
    assert_eq!(patched["future"], before["future"]);
    assert_eq!(patched["plugin"], before["plugin"]);
    assert_eq!(patched["window"]["future"], before["window"]["future"]);
    assert_eq!(patched["panes"]["future"], before["panes"]["future"]);
    assert_eq!(
        patched["panes"]["comparison"],
        before["panes"]["comparison"]
    );
    assert!(patched["panes"].get("browser").is_none());
    assert!(patched["window"].get("width").is_none());
    assert!(patched["window"].get("maximized").is_none());
    assert_eq!(patched["window"]["height"].as_float(), Some(900.0));
    store
        .update_gui_preferences(&[GuiPreferenceChange::ComparisonSplit(None)])
        .unwrap();
    let reset: toml::Value = toml::from_str(&fs::read_to_string(&gui).unwrap()).unwrap();
    assert!(reset["panes"].get("comparison").is_none());
    assert_eq!(reset["panes"]["future"], before["panes"]["future"]);
    assert_eq!(
        store.gui_preferences().unwrap().panes,
        PanePreferences::default()
    );
}

#[test]
fn absent_fields_are_not_materialized_by_unrelated_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store
        .update_gui_preferences(&[GuiPreferenceChange::BrowserSplit(Some(0.4))])
        .unwrap();
    let document: toml::Value =
        toml::from_str(&fs::read_to_string(dir.path().join("gui.toml")).unwrap()).unwrap();
    assert_eq!(document.as_table().unwrap().len(), 1);
    assert_eq!(document["panes"].as_table().unwrap().len(), 1);
    assert_eq!(
        store.gui_preferences().unwrap().window,
        WindowPreference::default()
    );
}

#[test]
fn malformed_stored_preferences_are_never_repaired_and_errors_are_safe() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let gui = dir.path().join("gui.toml");
    for text in [
        "theme = 'SECRET_NOT_A_THEME'",
        "theme = 'Dark'",
        "theme = 7",
        "theme = ['SECRET_TOKEN']",
        "theme = { dark = {} }",
        "SECRET_TOKEN = [",
        "window = 'SECRET_TOKEN'",
        "window = []",
        "window = [1100, 720, false]",
        "panes = []",
        "panes = [0.25, 0.75]",
        "panes = 3",
        "[window]\nwidth = 'SECRET_TOKEN'",
        "[window]\nheight = []",
        "[window]\nmaximized = 'SECRET_TOKEN'",
        "[window]\nwidth = 0",
        "[window]\nheight = -1",
        "[window]\nwidth = 16385",
        "[window]\nwidth = nan",
        "[window]\nheight = inf",
        "[window]\nheight = -inf",
        "[window]\nwidth = 1e300",
        "[panes]\nbrowser = 'SECRET_TOKEN'",
        "[panes]\ncomparison = {}",
        "[panes]\nbrowser = 0",
        "[panes]\ncomparison = 1",
        "[panes]\nbrowser = -0.1",
        "[panes]\ncomparison = 1.1",
        "[panes]\nbrowser = nan",
        "[panes]\ncomparison = inf",
    ] {
        fs::write(&gui, text).unwrap();
        for result in [
            store.gui_preferences(),
            store.update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)]),
            store.update_gui_preferences(&[GuiPreferenceChange::Window(
                WindowPreference::default(),
            )]),
            store.update_gui_preferences(&[
                GuiPreferenceChange::BrowserSplit(None),
                GuiPreferenceChange::ComparisonSplit(None),
            ]),
            store.update_gui_preferences(&[]),
        ] {
            let error = result.unwrap_err();
            assert!(matches!(error, Error::Invalid(_)), "{text}: {error}");
            assert!(!error.to_string().contains("SECRET"));
            assert!(!format!("{error:?}").contains("SECRET"));
        }
        assert_eq!(fs::read(&gui).unwrap(), text.as_bytes());
    }
    fs::write(&gui, [0xff, 0xfe, 0xfd]).unwrap();
    assert!(matches!(store.gui_preferences(), Err(Error::Invalid(_))));
    assert!(matches!(
        store.update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)]),
        Err(Error::Invalid(_))
    ));
    assert_eq!(fs::read(&gui).unwrap(), [0xff, 0xfe, 0xfd]);
}

#[test]
fn invalid_changes_leave_the_document_untouched_and_batch_validation_is_final() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let gui = dir.path().join("gui.toml");
    let original = "# keep bytes\ntheme = 'light'\nfuture = true\n";
    fs::write(&gui, original).unwrap();
    for change in [
        GuiPreferenceChange::Window(WindowPreference {
            width: 0.0,
            ..WindowPreference::default()
        }),
        GuiPreferenceChange::Window(WindowPreference {
            height: f32::INFINITY,
            ..WindowPreference::default()
        }),
        GuiPreferenceChange::BrowserSplit(Some(f32::NAN)),
        GuiPreferenceChange::ComparisonSplit(Some(1.0)),
    ] {
        assert!(matches!(
            store.update_gui_preferences(&[
                GuiPreferenceChange::Theme(ThemePreference::Dark),
                change,
            ]),
            Err(Error::Invalid(_))
        ));
        assert_eq!(fs::read(&gui).unwrap(), original.as_bytes());
    }
    let valid = store
        .update_gui_preferences(&[
            GuiPreferenceChange::BrowserSplit(Some(f32::NAN)),
            GuiPreferenceChange::BrowserSplit(Some(0.5)),
        ])
        .unwrap();
    assert_eq!(valid.panes.browser, Some(0.5));
    assert_eq!(valid.theme, ThemePreference::Light);
}

#[test]
fn shared_lock_is_busy_for_updates_but_not_reads_or_empty_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let gui = dir.path().join("gui.toml");
    let original = "theme = 'light'\n";
    fs::write(&gui, original).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.path().join("write.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    assert!(matches!(
        store.update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)]),
        Err(Error::Busy)
    ));
    assert_eq!(
        store.gui_preferences().unwrap().theme,
        ThemePreference::Light
    );
    assert_eq!(
        store.update_gui_preferences(&[]).unwrap().theme,
        ThemePreference::Light
    );
    assert_eq!(fs::read(&gui).unwrap(), original.as_bytes());
    FileExt::unlock(&lock).unwrap();
    store
        .update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)])
        .unwrap();
    assert_eq!(
        store.gui_preferences().unwrap().theme,
        ThemePreference::Dark
    );
}

#[test]
fn non_regular_files_and_oversized_files_are_rejected_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let gui = dir.path().join("gui.toml");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&gui)
            .status()
            .unwrap()
            .success()
    );
    let (sender, receiver) = mpsc::channel();
    let fifo_store = store.clone();
    let worker = std::thread::spawn(move || {
        sender
            .send((
                fifo_store.gui_preferences(),
                fifo_store
                    .update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)]),
            ))
            .unwrap();
    });
    let (load, save) = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("FIFO access blocked");
    assert!(matches!(load, Err(Error::Invalid(_))));
    assert!(matches!(save, Err(Error::Invalid(_))));
    worker.join().unwrap();
    assert!(!fs::metadata(&gui).unwrap().is_file());
    fs::remove_file(&gui).unwrap();
    symlink("/dev/null", &gui).unwrap();
    assert!(matches!(store.gui_preferences(), Err(Error::Invalid(_))));
    assert!(matches!(
        store.update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)]),
        Err(Error::Invalid(_))
    ));
    fs::remove_file(&gui).unwrap();
    fs::create_dir(&gui).unwrap();
    assert!(matches!(store.gui_preferences(), Err(Error::Invalid(_))));
    fs::remove_dir(&gui).unwrap();
    let file = fs::File::create(&gui).unwrap();
    file.set_len(1024 * 1024 + 1).unwrap();
    assert!(matches!(store.gui_preferences(), Err(Error::Invalid(_))));
    assert!(matches!(
        store.update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)]),
        Err(Error::Invalid(_))
    ));
    assert_eq!(fs::metadata(&gui).unwrap().len(), 1024 * 1024 + 1);
    fs::write(&gui, vec![b'#'; 1024 * 1024]).unwrap();
    assert_eq!(store.gui_preferences().unwrap(), GuiPreferences::default());
}

#[test]
fn separate_stores_reread_fresh_fields_and_preserve_external_unknowns() {
    let dir = tempfile::tempdir().unwrap();
    let first = Store::new(dir.path().into());
    let second = Store::new(dir.path().into());
    let stale = first.gui_preferences().unwrap();
    second
        .update_gui_preferences(&[GuiPreferenceChange::BrowserSplit(Some(0.3))])
        .unwrap();
    let gui = dir.path().join("gui.toml");
    let mut document: toml::Value = toml::from_str(&fs::read_to_string(&gui).unwrap()).unwrap();
    document
        .as_table_mut()
        .unwrap()
        .insert("future".into(), toml::Value::String("new".into()));
    fs::write(&gui, toml::to_string(&document).unwrap()).unwrap();
    let changed = first
        .update_gui_preferences(&[GuiPreferenceChange::Theme(ThemePreference::Dark)])
        .unwrap();
    assert_eq!(stale.panes.browser, None);
    assert_eq!(changed.panes.browser, Some(0.3));
    assert_eq!(changed.theme, ThemePreference::Dark);
    let changed = second
        .update_gui_preferences(&[GuiPreferenceChange::ComparisonSplit(Some(0.7))])
        .unwrap();
    assert_eq!(changed.theme, ThemePreference::Dark);
    assert_eq!(changed.panes.browser, Some(0.3));
    let final_document: toml::Value = toml::from_str(&fs::read_to_string(&gui).unwrap()).unwrap();
    assert_eq!(final_document["future"].as_str(), Some("new"));
}

#[test]
fn parallel_independent_field_patches_survive_shared_lock_contention() {
    let dir = tempfile::tempdir().unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for change in [
        GuiPreferenceChange::Theme(ThemePreference::Dark),
        GuiPreferenceChange::BrowserSplit(Some(0.25)),
        GuiPreferenceChange::ComparisonSplit(Some(0.75)),
    ] {
        let store = Store::new(dir.path().into());
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match store.update_gui_preferences(&[change]) {
                    Ok(_) => break,
                    Err(Error::Busy) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("GUI preference patch failed: {error}"),
                }
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    let final_preferences = Store::new(dir.path().into()).gui_preferences().unwrap();
    assert_eq!(final_preferences.theme, ThemePreference::Dark);
    assert_eq!(final_preferences.panes.browser, Some(0.25));
    assert_eq!(final_preferences.panes.comparison, Some(0.75));
}
