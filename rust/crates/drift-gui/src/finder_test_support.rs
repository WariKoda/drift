use std::{fs, path::Path, process::Command};

pub fn fail_git_scan(root: &Path) {
    // Call after the initial browser opens: a pre-existing repository would
    // trigger project registration. The walk succeeds, then Git rejects its index.
    let commands: &[&[&str]] = &[
        &["init", "--quiet", "--template=", "--initial-branch=main"],
        &["config", "--local", "core.excludesFile", "/dev/null"],
    ];
    for args in commands {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(*args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("Git is required for Finder failure tests");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(root.join(".git/index"), b"corrupt index").unwrap();
}
