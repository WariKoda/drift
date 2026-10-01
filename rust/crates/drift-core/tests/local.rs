use drift_core::{
    error::Error,
    local::{PREVIEW_LIMIT, ProjectRoot},
};
use std::{
    fs,
    io::Cursor,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};
#[test]
fn capability_survives_root_rename_and_rejects_escapes_and_special_files() {
    let parent = tempfile::tempdir().unwrap();
    let base = parent.path().join("project");
    fs::create_dir(&base).unwrap();
    fs::write(base.join("inside"), "text").unwrap();
    fs::write(parent.path().join("outside"), "secret").unwrap();
    symlink("../outside", base.join("escape")).unwrap();
    symlink("inside", base.join("link")).unwrap();
    let root = ProjectRoot::open(&base).unwrap();
    assert!(root.preview(Path::new("escape")).is_err());
    assert!(root.preview(Path::new("../outside")).is_err());
    assert!(root.preview(&parent.path().join("outside")).is_err());
    assert_eq!(root.preview(Path::new("link")).unwrap(), "text");
    let output = std::process::Command::new("mkfifo")
        .arg(base.join("pipe"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(root.preview(Path::new("pipe")).is_err());
    fs::rename(&base, parent.path().join("moved")).unwrap();
    fs::create_dir(&base).unwrap();
    fs::write(base.join("inside"), "replacement").unwrap();
    assert_eq!(root.preview(Path::new("inside")).unwrap(), "text");
}
#[test]
fn preview_rejects_binary_and_oversized_files_and_walker_excludes_staging() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("binary"), b"hello\0world").unwrap();
    fs::write(
        dir.path().join("large"),
        vec![b'a'; (PREVIEW_LIMIT + 1) as usize],
    )
    .unwrap();
    fs::write(
        dir.path()
            .join(".a.drift-tmp-0123456789abcdef0123456789abcdef"),
        b"interrupted",
    )
    .unwrap();
    fs::create_dir(dir.path().join("node_modules")).unwrap();
    let root = ProjectRoot::open(dir.path()).unwrap();
    assert!(matches!(
        root.preview(Path::new("binary")),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        root.preview(Path::new("large")),
        Err(Error::Invalid(_))
    ));
    assert_eq!(root.entries(Path::new(".")).unwrap().len(), 2);
}
#[test]
fn atomic_write_preserves_previous_file_on_completion_error_and_keeps_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    fs::write(&path, "previous").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let root = ProjectRoot::open(dir.path()).unwrap();
    let missing = dir.path().join("missing-completion");
    let result = root.write_atomic(&path, Cursor::new(b"partial"), |_| {
        fs::metadata(&missing)?;
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "previous");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    root.write_atomic(&path, Cursor::new(b"complete"), |_| Ok(()))
        .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "complete");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    symlink("file", dir.path().join("link")).unwrap();
    assert!(
        root.write_atomic(Path::new("link"), Cursor::new(b"bad"), |_| Ok(()))
            .is_err()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "complete");
}

#[test]
fn rejected_atomic_destinations_still_finish_the_owned_source() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("source"), b"source").unwrap();
    fs::write(dir.path().join("parent-file"), b"parent").unwrap();
    symlink("source", dir.path().join("target-link")).unwrap();
    let root = ProjectRoot::open(dir.path()).unwrap();
    for target in ["../escape", "target-link", "parent-file/target"] {
        let source = fs::File::open(dir.path().join("source")).unwrap();
        let mut finished = false;
        let result = root.write_atomic(Path::new(target), source, |source| {
            nix::unistd::close(source).unwrap();
            finished = true;
            Ok(())
        });
        assert!(result.is_err());
        assert!(finished, "source was abandoned for {target}");
    }
}
