//! Run the actual executable without a display or GUI initialization.
use drift_core::{config::Host, store::Store};
use fs2::FileExt;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn invoke(config: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_drift-gui"))
        .args(args)
        .current_dir(cwd)
        .env("XDG_CONFIG_HOME", config)
        .env("HOME", cwd)
        .env_remove("DRIFT_LOG")
        .env_remove("DRIFT_DEBUG")
        .env("DISPLAY", "")
        .env("WAYLAND_DISPLAY", "")
        .output()
        .unwrap()
}
fn success(config: &Path, cwd: &Path, args: &[&str]) -> String {
    let output = invoke(config, cwd, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
#[test]
fn help_and_version_need_neither_display_nor_configuration() {
    let root = tempfile::tempdir().unwrap();
    let inaccessible_config = root.path().join("not-a-directory");
    fs::write(&inaccessible_config, "not a config directory").unwrap();
    for args in [
        vec!["--help"],
        vec!["projects"],
        vec!["projects", "add", "--help"],
        vec!["open", "--help"],
    ] {
        assert!(success(&inaccessible_config, root.path(), &args).contains("Usage: drift-gui"));
    }
    assert_eq!(
        success(&inaccessible_config, root.path(), &["version"]),
        format!("drift-gui {}\n", env!("CARGO_PKG_VERSION"))
    );
    let output = Command::new(env!("CARGO_BIN_EXE_drift-gui"))
        .arg("--help")
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DRIFT_LOG")
        .env_remove("DRIFT_DEBUG")
        .env("DISPLAY", "")
        .env("WAYLAND_DISPLAY", "")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}
#[test]
fn real_cli_mutations_roundtrip_shared_stores_without_touching_local_trees() {
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(root.path().join("keep"), "local data").unwrap();
    fs::create_dir(root.path().join("other")).unwrap();
    assert!(
        success(config.path(), root.path(), &["projects", "list"])
            .starts_with("No projects registered.")
    );
    assert!(!config.path().join("drift").exists());
    success(config.path(), root.path(), &["projects", "add", "Shop"]);
    success(
        config.path(),
        root.path(),
        &["projects", "add", "Shop", "~/other"],
    );
    let store = Store::new(config.path().join("drift"));
    store.mark_opened("shop").unwrap();
    store
        .save_host(
            Some("shop"),
            None,
            Host {
                name: "prod".into(),
                hostname: "example.test".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let before = store.registry().unwrap().find("shop").unwrap().clone();
    let settings_path = config.path().join("drift/projects/shop.toml");
    let settings = fs::read(&settings_path).unwrap();
    assert_eq!(
        store.registry().unwrap().find("shop-2").unwrap().path,
        root.path().join("other")
    );
    success(
        config.path(),
        root.path(),
        &["projects", "edit", "shop", "--name=New Shop"],
    );
    success(config.path(), root.path(), &["projects", "archive", "shop"]);
    let listed = success(config.path(), root.path(), &["projects", "list"]);
    assert!(listed.contains("shop\tNew Shop\t"));
    assert!(listed.contains("\tarchived\n"));
    let project = store.registry().unwrap().find("shop").unwrap().clone();
    assert!(project.created_at == before.created_at && project.opened_at == before.opened_at);
    assert_eq!(fs::read(&settings_path).unwrap(), settings);
    success(config.path(), root.path(), &["projects", "archive", "shop"]);
    success(
        config.path(),
        root.path(),
        &["projects", "edit", "shop-2", "--path", "gone"],
    );
    assert!(success(config.path(), root.path(), &["projects", "list"]).contains("\tmissing\n"));
    success(config.path(), root.path(), &["projects", "remove", "shop"]);
    assert!(!settings_path.exists());
    assert!(store.registry().unwrap().find("shop").is_none());
    assert_eq!(fs::read(root.path().join("keep")).unwrap(), b"local data");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    assert!(!config.path().join("drift/drift.log").exists());
}
#[test]
fn syntax_store_and_missing_project_errors_return_nonzero_without_writes() {
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    success(config.path(), root.path(), &["projects", "add", "Shop"]);
    let registry = config.path().join("drift/projects.toml");
    let before = fs::read(&registry).unwrap();
    for args in [
        vec!["projects", "unknown"],
        vec!["projects", "add"],
        vec!["projects", "edit", "shop", "--path"],
        vec!["projects", "remove", "absent"],
        vec!["projects", "edit", "shop", "--name="],
        vec!["projects", "add", "Duplicate"],
        vec!["open", "absent"],
        vec!["--unexpected"],
        vec!["dash", "extra"],
    ] {
        let output = invoke(config.path(), root.path(), &args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("drift-gui: "));
        assert_eq!(fs::read(&registry).unwrap(), before);
    }
    success(
        config.path(),
        root.path(),
        &["projects", "edit", "shop", "--path", "gone"],
    );
    let before = fs::read(&registry).unwrap();
    let output = invoke(config.path(), root.path(), &["open", "shop"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(&registry).unwrap(), before);
    fs::write(&registry, "project = [").unwrap();
    let output = invoke(config.path(), root.path(), &["projects", "list"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(&registry).unwrap(), b"project = [");
}
#[test]
fn another_process_lock_blocks_every_mutation_but_allows_listing() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    success(config.path(), root.path(), &["projects", "add", "Shop"]);
    let registry = config.path().join("drift/projects.toml");
    let before = fs::read(&registry).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(config.path().join("drift/write.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    for args in [
        vec!["projects", "add", "Other", other.path().to_str().unwrap()],
        vec!["projects", "edit", "shop", "--name", "Changed"],
        vec!["projects", "archive", "shop"],
        vec!["projects", "remove", "shop"],
    ] {
        let output = invoke(config.path(), root.path(), &args);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("another drift process"));
        assert_eq!(fs::read(&registry).unwrap(), before);
    }
    assert!(success(config.path(), root.path(), &["projects", "list"]).contains("Shop"));
    FileExt::unlock(&lock).unwrap();
    success(config.path(), root.path(), &["projects", "archive", "shop"]);
    assert!(
        Store::new(config.path().join("drift"))
            .registry()
            .unwrap()
            .find("shop")
            .unwrap()
            .archived
    );
}
