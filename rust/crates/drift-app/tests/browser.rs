use drift_app::browser::{BrowserService, OperationId};
use drift_core::store::Store;
use std::{fs, path::PathBuf};
#[tokio::test]
async fn real_git_ignore_classification_and_finder_respect_visibility_and_hard_exclusions() {
    let dir = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(dir.path().join("ignored.txt"), "hidden by Git").unwrap();
    fs::write(dir.path().join("visible.txt"), "hello\nworld").unwrap();
    fs::create_dir(dir.path().join("node_modules")).unwrap();
    fs::write(dir.path().join("node_modules/package"), "excluded").unwrap();
    let service = BrowserService::new().unwrap();
    let operation = service.open(
        Store::new(config.path().into()),
        dir.path().into(),
        OperationId {
            project: 1,
            operation: 1,
        },
        false,
        false,
    );
    let directory = operation.task.await.unwrap().unwrap();
    assert_eq!(directory.entries.len(), 1);
    assert_eq!(
        directory.entries[0].path.file_name().unwrap(),
        "visible.txt"
    );
    let preview = service.preview(
        directory.location.clone(),
        PathBuf::from("visible.txt"),
        OperationId {
            project: 1,
            operation: 2,
        },
    );
    assert_eq!(preview.task.await.unwrap().unwrap(), "hello\nworld");
    let operation = service.find(
        directory.location,
        OperationId {
            project: 1,
            operation: 3,
        },
        true,
        true,
    );
    let entries = operation.task.await.unwrap().unwrap();
    assert_eq!(entries.len(), 3); // .gitignore, ignored.txt, visible.txt
    assert!(
        !entries
            .iter()
            .any(|e| e.path.to_string_lossy().contains("node_modules"))
    );
}

#[tokio::test]
async fn tracked_files_and_parent_worktree_rules_survive_subdirectory_browsing() {
    let repo = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let child = repo.path().join("child");
    fs::create_dir(&child).unwrap();
    fs::write(repo.path().join(".gitignore"), "*.txt\n!visible.txt\n").unwrap();
    for name in ["ignored.txt", "tracked.txt", "visible.txt"] {
        fs::write(child.join(name), "content").unwrap();
    }
    for args in [
        vec!["init", "--quiet"],
        vec!["add", "--force", "child/tracked.txt"],
    ] {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let service = BrowserService::new().unwrap();
    let aliases = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(repo.path(), aliases.path().join("repo")).unwrap();
    for directory in [child, aliases.path().join("repo/child")] {
        let operation = service.open(
            Store::new(config.path().into()),
            directory,
            OperationId {
                project: 1,
                operation: 1,
            },
            false,
            false,
        );
        let directory = operation.task.await.unwrap().unwrap();
        let names: Vec<_> = directory
            .entries
            .iter()
            .map(|e| e.path.file_name().unwrap())
            .collect();
        assert_eq!(names, ["tracked.txt", "visible.txt"]);
    }
}
