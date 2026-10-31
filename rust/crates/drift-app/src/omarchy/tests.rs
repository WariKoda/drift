use super::*;
use drift_core::omarchy::{PaletteMode, Rgb};
use std::os::unix::fs::symlink;

fn fixture(accent: &str) -> String {
    format!(
        "background='#102030'\nforeground='#d0e0f0'\nred='#b02030'\ngreen='#20b030'\nyellow='#b0a030'\nblue='#2030b0'\nmagenta='#a030b0'\ncyan='#20a0b0'\naccent='{accent}'\n"
    )
}

#[test]
fn canonical_state_root_and_home_fallback_without_global_environment_changes() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let home = dir.path().join("home");
    let source = PaletteSource::from_roots(Some(state.as_os_str()), Some(home.as_os_str()));
    assert_eq!(
        source.theme_directory(),
        Some(state.join("omarchy/current/theme").as_path())
    );
    let source = PaletteSource::from_roots(None, Some(home.as_os_str()));
    assert_eq!(
        source.theme_directory(),
        Some(home.join(".local/state/omarchy/current/theme").as_path())
    );
    for state in [OsStr::new(""), OsStr::new("relative")] {
        assert_eq!(
            PaletteSource::from_roots(Some(state), Some(home.as_os_str())).theme_directory(),
            source.theme_directory()
        );
    }
    assert!(PaletteSource::from_roots(None, None).load().is_err());
    // The obsolete config path must never be consulted.
    let old = home.join(".config/omarchy/current/theme");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("colors.toml"), fixture("#abcdef")).unwrap();
    assert!(source.load().is_err());
}

#[test]
fn real_files_content_atomic_file_directory_symlink_swaps_and_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let source = PaletteSource::from_roots(Some(dir.path().as_os_str()), None);
    let theme = source.theme_directory().unwrap();
    fs::create_dir_all(theme).unwrap();
    assert!(source.load().is_err());
    let file = theme.join("colors.toml");
    fs::write(&file, fixture("#112233")).unwrap();
    let first = source.load().unwrap();
    assert_eq!(first, source.load().unwrap());
    fs::write(&file, fixture("#445566")).unwrap();
    assert_eq!(source.load().unwrap().accent, Rgb(0x445566));
    fs::write(theme.join("next.toml"), fixture("#778899")).unwrap();
    fs::rename(theme.join("next.toml"), &file).unwrap();
    assert_eq!(source.load().unwrap().accent, Rgb(0x778899));
    fs::write(theme.join("light.mode"), "").unwrap();
    assert_eq!(source.load().unwrap().mode, PaletteMode::Light);
    fs::remove_file(theme.join("light.mode")).unwrap();
    assert_eq!(source.load().unwrap().mode, PaletteMode::Dark);
    let next = theme.with_file_name("next");
    fs::create_dir(&next).unwrap();
    fs::write(next.join("colors.toml"), fixture("#aabbcc")).unwrap();
    fs::rename(theme, theme.with_file_name("previous")).unwrap();
    fs::rename(&next, theme).unwrap();
    assert_eq!(source.load().unwrap().accent, Rgb(0xaabbcc));
    // Replace a parent symlink, as Omarchy does for current/theme.
    let current = dir.path().join("omarchy/current");
    let target = dir.path().join("replacement");
    fs::create_dir_all(target.join("theme")).unwrap();
    fs::write(target.join("theme/colors.toml"), fixture("#ddeeff")).unwrap();
    fs::rename(&current, current.with_file_name("old")).unwrap();
    symlink(current.with_file_name("old"), &current).unwrap();
    symlink(&target, current.with_file_name("next")).unwrap();
    fs::rename(current.with_file_name("next"), &current).unwrap();
    assert_eq!(source.load().unwrap().accent, Rgb(0xddeeff));
    fs::write(theme.join("colors.toml"), "red='invalid'").unwrap();
    assert!(
        source
            .load()
            .unwrap_err()
            .contains("Using System appearance")
    );
    fs::write(theme.join("colors.toml"), fixture("#112233")).unwrap();
    assert_eq!(source.load().unwrap(), first);
}

#[test]
fn bounded_regular_utf8_toml_data_only_loading() {
    let dir = tempfile::tempdir().unwrap();
    let source = PaletteSource::from_roots(Some(dir.path().as_os_str()), None);
    let theme = source.theme_directory().unwrap();
    fs::create_dir_all(theme).unwrap();
    let file = theme.join("colors.toml");
    for bytes in [
        vec![b'#'; MAX_PALETTE_BYTES + 1],
        vec![0xff, 0xfe],
        b"background='#000000'".to_vec(),
    ] {
        fs::write(&file, bytes).unwrap();
        assert!(source.load().is_err());
    }
    fs::remove_file(&file).unwrap();
    fs::create_dir(&file).unwrap();
    assert!(source.load().is_err());
    fs::remove_dir(&file).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(&file).unwrap();
    assert!(source.load().is_err());
    fs::remove_file(&file).unwrap();
    fs::write(&file, fixture("#aabbcc")).unwrap();
    // Executable theme files are irrelevant and must never be launched.
    fs::write(theme.join("theme.sh"), "exit 99").unwrap();
    assert!(source.load().is_ok());
}
