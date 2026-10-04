#[path = "../../drift-core/tests/support/mod.rs"]
mod support;

use drift_app::{
    browser::{BrowserService, Location, OperationId},
    comparison::{ComparisonService, LoadRequest, ScopeOptions, remote_markable_paths},
    remote::RemoteService,
};
use drift_core::{
    config::{Host, RuntimeConfig},
    error::Error,
    local::ProjectRoot,
    pathmap::Mapping,
    remote::{self, ConnectOptions, ConnectionState},
    staging::staging_name,
};
use std::{collections::BTreeSet, fs, path::Path, process::Command, sync::Arc};
use tokio_util::sync::CancellationToken;

fn id() -> OperationId {
    OperationId {
        project: 1,
        operation: 2,
    }
}
fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn mapping(local: &str, remote: &str) -> Mapping {
    Mapping {
        local: local.into(),
        remote: remote.into(),
    }
}

async fn exercise(mut host: Host, options: ConnectOptions, files: &Path) {
    let repository = tempfile::tempdir().unwrap();
    let linked = tempfile::tempdir().unwrap();
    fs::create_dir_all(repository.path().join("child/src")).unwrap();
    fs::write(
        repository.path().join(".gitignore"),
        "child/src/*.log\n!child/src/keep.log\nchild/src/remote-only.txt\nchild/src/cache/\nchild/src/dir-only/\nchild/src/folder-only/\nchild/fallback/*.tmp\nchild/unmapped.log\n",
    )
    .unwrap();
    fs::write(repository.path().join("child/src/tracked.log"), "tracked").unwrap();
    git(repository.path(), &["init", "--quiet"]);
    git(repository.path(), &["add", ".gitignore"]);
    git(
        repository.path(),
        &["add", "--force", "child/src/tracked.log"],
    );
    git(
        repository.path(),
        &[
            "-c",
            "user.name=Visibility Test",
            "-c",
            "user.email=visibility@example.test",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    let worktree = linked.path().join("worktree");
    git(
        repository.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "visibility",
            worktree.to_str().unwrap(),
        ],
    );
    let local = worktree.join("child");
    // The browser is below the worktree root; classification must still use
    // parent rules and the linked worktree's real index.
    let location = Location {
        root: Arc::new(ProjectRoot::open(&local).unwrap()),
        directory: local.join("src"),
        slug: None,
        config: RuntimeConfig {
            hosts: vec![],
            mappings: vec![mapping("fallback", "deployed")],
            ui: Default::default(),
        },
    };
    let scope = files.join("scope");
    for directory in [
        "deployed/cache",
        "deployed/folder-only",
        "deployed/node_modules",
        "dependencies",
        "metadata",
        "node_modules",
        ".git",
        ".svn",
        ".hg",
        ".idea",
        ".vscode",
    ] {
        fs::create_dir_all(scope.join(directory)).unwrap();
    }
    let staging = staging_name("download").unwrap();
    for name in [
        "deployed/drop.log",
        "deployed/keep.log",
        "deployed/tracked.log",
        "deployed/remote-only.txt",
        "deployed/visible.txt",
        "deployed/project.tmp",
        "deployed/dir-only",
        "deployed/cache/nested",
        "deployed/node_modules/package",
        "dependencies/package",
        "metadata/config",
        "node_modules/package",
        "unmapped.log",
        "staged-alias",
    ] {
        fs::write(scope.join(name), "remote").unwrap();
    }
    fs::write(scope.join("deployed").join(&staging), "incomplete").unwrap();
    fs::write(files.join("outside.txt"), "outside").unwrap();
    host.root_path = if host.protocol == "ftp" {
        "/scope".into()
    } else {
        scope.to_str().unwrap().into()
    };
    host.mappings = vec![
        mapping("src", "deployed"),
        mapping("node_modules", "dependencies"),
        mapping(".git", "metadata"),
        mapping(&staging, "staged-alias"),
    ];

    // Read actual unfiltered protocol entries: RemoteService::list already
    // removes hard-excluded names, so it cannot supply those test cases.
    let client = remote::connect(host.clone(), options.clone(), CancellationToken::new())
        .await
        .unwrap();
    let root = client.canonicalize(&host.root_path).await.unwrap();
    let mut entries = client.read_dir(&root).await.unwrap();
    for suffix in [
        "deployed",
        "deployed/cache",
        "deployed/node_modules",
        "dependencies",
        "metadata",
        "node_modules",
    ] {
        entries.extend(client.read_dir(&format!("{root}/{suffix}")).await.unwrap());
    }
    let parent = Path::new(&root).parent().unwrap().to_str().unwrap();
    let outside = client
        .read_dir(parent)
        .await
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == "outside.txt")
        .unwrap();
    let outside_path = outside.path.clone();
    entries.push(outside);
    client.shutdown().await;

    let browser = BrowserService::new().unwrap();
    let service = RemoteService::with_options(browser.clone(), options);
    let session = service
        .connect(host.clone(), id())
        .task
        .await
        .unwrap()
        .unwrap()
        .session;
    let classified = service.classify_entries(
        location.clone(),
        host.clone(),
        session.clone(),
        entries.clone(),
        id(),
    );
    assert_eq!(classified.id, id());
    let visibility = classified.task.await.unwrap().unwrap();
    let paths = |suffixes: &[&str]| -> BTreeSet<String> {
        suffixes
            .iter()
            .map(|suffix| format!("{root}/{suffix}"))
            .collect()
    };
    assert_eq!(
        visibility.ignored,
        paths(&[
            "deployed/drop.log",
            "deployed/remote-only.txt",
            "deployed/cache",
            "deployed/cache/nested",
            "deployed/folder-only",
        ])
    );
    let mut excluded = paths(&[
        "deployed/node_modules",
        "deployed/node_modules/package",
        "dependencies",
        "dependencies/package",
        "metadata",
        "metadata/config",
        "node_modules",
        "node_modules/package",
        ".git",
        ".svn",
        ".hg",
        ".idea",
        ".vscode",
        "staged-alias",
    ]);
    excluded.insert(format!("{root}/deployed/{staging}"));
    excluded.insert(outside_path);
    assert_eq!(visibility.excluded, excluded);
    assert!(visibility.ignored.is_disjoint(&visibility.excluded));
    assert!(!local.join("src/remote-only.txt").exists());
    assert_eq!(
        remote_markable_paths(
            &location,
            &host,
            &root,
            vec![format!("{root}/unmapped.log")],
            &Default::default(),
        )
        .unwrap(),
        BTreeSet::new()
    );

    let mut fallback_host = host.clone();
    fallback_host.mappings.clear();
    let fallback = service
        .classify_entries(
            location.clone(),
            fallback_host,
            session.clone(),
            entries.clone(),
            id(),
        )
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fallback.ignored, paths(&["deployed/project.tmp"]));
    let mut root_relative = location.clone();
    root_relative.config.mappings.clear();
    let mut root_host = host.clone();
    root_host.mappings.clear();
    let root_visibility = service
        .classify_entries(
            root_relative,
            root_host,
            session.clone(),
            entries.clone(),
            id(),
        )
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(root_visibility.ignored, paths(&["unmapped.log"]));
    // Project mappings are not validated/used when a host overrides them.
    let mut invalid_project = location.clone();
    invalid_project.config.mappings = vec![mapping("../invalid", "deployed")];
    let overridden = service
        .classify_entries(
            invalid_project,
            host.clone(),
            session.clone(),
            entries.clone(),
            id(),
        )
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(overridden.ignored, visibility.ignored);

    let cancelled = service.classify_entries(
        location.clone(),
        host.clone(),
        session.clone(),
        entries.iter().cycle().take(20_000).cloned().collect(),
        id(),
    );
    cancelled.cancel.cancel();
    assert!(
        matches!(cancelled.task.await.unwrap(), Err(Error::Invalid(message)) if message.contains("cancelled"))
    );
    assert_eq!(session.state(), ConnectionState::Connected);
    assert_eq!(
        service
            .preview(
                session.clone(),
                format!("{root}/deployed/visible.txt"),
                id(),
            )
            .task
            .await
            .unwrap()
            .unwrap(),
        "remote"
    );

    let mut invalid_host = host.clone();
    invalid_host.mappings = vec![mapping("../invalid", "deployed")];
    assert!(matches!(
        service
            .classify_entries(
                location.clone(),
                invalid_host,
                session.clone(),
                entries.clone(),
                id(),
            )
            .task
            .await
            .unwrap(),
        Err(Error::Mapping(_))
    ));

    let config_path = repository.path().join(".git/config");
    let config = fs::read(&config_path).unwrap();
    fs::write(&config_path, "[broken\n").unwrap();
    let git_failure = service
        .classify_entries(
            location.clone(),
            host.clone(),
            session.clone(),
            entries.clone(),
            id(),
        )
        .task
        .await
        .unwrap();
    assert!(
        matches!(git_failure, Err(Error::Invalid(message)) if message.contains("find Git worktree"))
    );
    // With broken Git metadata, excluded and unmapped entries still succeed:
    // neither category may be submitted to the Git batch.
    let unclassified = entries
        .iter()
        .filter(|entry| entry.name == "unmapped.log" || excluded.contains(&entry.path))
        .cloned()
        .collect();
    let skipped = service
        .classify_entries(
            location.clone(),
            host.clone(),
            session.clone(),
            unclassified,
            id(),
        )
        .task
        .await
        .unwrap()
        .unwrap();
    assert!(skipped.ignored.is_empty());
    assert_eq!(skipped.excluded, excluded);
    fs::write(config_path, config).unwrap();
    assert_eq!(session.state(), ConnectionState::Connected);

    let comparison = ComparisonService::new(browser)
        .load(
            LoadRequest {
                location,
                host,
                connection: session.clone(),
                local: vec![],
                remote: vec![format!("{root}/unmapped.log")],
                scope: ScopeOptions::default(),
            },
            id(),
        )
        .operation
        .task
        .await
        .unwrap()
        .unwrap();
    session.shutdown().await;
    assert_eq!(comparison.entries.len(), 1);
    assert!(comparison.entries[0].error.is_some());
    assert!(comparison.entries[0].result.is_none()); // no deletion suggestion
}

#[tokio::test]
async fn sftp_entries_use_local_git_policy_without_changing_the_connection() {
    let server = support::Server::new(false);
    exercise(
        server.host(),
        server.options(),
        &server.dir.path().join("files"),
    )
    .await;
}

#[tokio::test]
async fn ftp_entries_use_local_git_policy_without_changing_the_connection() {
    let server = support::ftp::Server::new(4);
    exercise(
        server.host(),
        server.options(),
        &server.dir.path().join("files"),
    )
    .await;
    server.wait_idle().await;
}
