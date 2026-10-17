use super::*;
use std::{fs, process::Command};

#[tokio::test]
async fn rejected_git_index_reports_classification_failure_even_when_stdin_breaks() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new("git")
        .arg("-C")
        .arg(root.path())
        .args(["init", "--quiet", "--template=", "--initial-branch=main"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("Git is required for ignore classification tests");
    assert!(output.status.success());
    fs::write(root.path().join("kept.txt"), "kept").unwrap();
    fs::write(root.path().join(".git/index"), b"corrupt index").unwrap();
    let project = ProjectRoot::open(root.path()).unwrap();
    let entry = project
        .entries(Path::new("."))
        .unwrap()
        .into_iter()
        .find(|entry| entry.path == Path::new("kept.txt"))
        .unwrap();
    // More than a pipe buffer of real-path requests forces an early Git exit
    // to interrupt writing instead of depending on process scheduling.
    let entries = vec![entry; 131_072];
    let error = classify_ignored(&CancellationToken::new(), root.path(), &entries)
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::Invalid(message) if message.starts_with("Git ignore classification:"))
    );
}
