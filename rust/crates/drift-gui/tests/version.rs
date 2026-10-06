//! Release identifiers are build-time values; environment at runtime cannot replace them.
#[path = "../build_support.rs"]
mod build_support;

#[test]
fn accepts_exact_semver_including_prerelease_and_build_identifiers() {
    for value in [
        "0.1.0",
        "1.2.3-alpha.1",
        "1.2.3-alpha.1+build.07",
        "65535.65535.65535",
        "1.2.3-0",
        "1.2.3-a-b",
    ] {
        assert_eq!(build_support::checked_version(value), Some(value));
    }
}

#[test]
fn rejects_unsafe_noncanonical_and_bundle_incompatible_versions() {
    for value in [
        "",
        "v1.2.3",
        "1.2",
        "01.2.3",
        "1.2.3-01",
        "1.2.3-",
        "1.2.3+",
        "1.2.3-a..b",
        "65536.0.0",
        "1.2.3/../../file",
        "1.2.3\nInjected",
        "1.2.3\r",
        "1.2.3\0",
        "1.2.3\t",
        " 1.2.3",
        "1.2.3 ",
        "1.2.3-é",
    ] {
        assert!(build_support::checked_version(value).is_none());
    }
    assert!(build_support::checked_version(&format!("1.2.3-{}", "a".repeat(128))).is_none());
}

#[test]
fn runtime_version_override_is_ignored_without_configuration_or_display() {
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_drift-gui"))
        .arg("version")
        .env("DRIFT_GUI_VERSION", "9.9.9-do-not-use-runtime-value")
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("HOME", root.path())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DRIFT_LOG")
        .env_remove("DRIFT_DEBUG")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        format!("drift-gui {}\n", env!("DRIFT_GUI_VERSION")).as_bytes()
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
