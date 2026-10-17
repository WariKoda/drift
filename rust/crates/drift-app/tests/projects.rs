use drift_app::{
    browser::{BrowserService, OperationId},
    projects::{ProjectCommand, StartOptions},
};
use drift_core::{project::now, store::Store};
use std::fs;
const ID: OperationId = OperationId {
    project: 1,
    operation: 1,
};

#[tokio::test]
async fn startup_restores_only_the_last_active_usable_project_and_honours_overrides() {
    let config = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let service = BrowserService::new().unwrap();
    let options = StartOptions {
        directory: cwd.path().into(),
        dashboard: false,
        no_dashboard: false,
        explicit_directory: false,
    };
    let start = service
        .resolve_start(store.clone(), options.clone(), ID)
        .task
        .await
        .unwrap()
        .unwrap();
    assert!(!start.dashboard && start.directory == cwd.path());
    let project = store.register("Shop", root.path().into()).unwrap();
    assert!(
        service
            .resolve_start(store.clone(), options.clone(), ID)
            .task
            .await
            .unwrap()
            .unwrap()
            .dashboard
    );
    store.mark_opened(&project.slug).unwrap();
    let start = service
        .resolve_start(store.clone(), options.clone(), ID)
        .task
        .await
        .unwrap()
        .unwrap();
    assert!(!start.dashboard && start.directory == root.path());
    let mut explicit = options.clone();
    explicit.explicit_directory = true;
    assert_eq!(
        service
            .resolve_start(store.clone(), explicit, ID)
            .task
            .await
            .unwrap()
            .unwrap()
            .directory,
        cwd.path()
    );
    let mut forced = options.clone();
    forced.dashboard = true;
    assert!(
        service
            .resolve_start(store.clone(), forced.clone(), ID)
            .task
            .await
            .unwrap()
            .unwrap()
            .dashboard
    );
    forced.no_dashboard = true;
    let start = service
        .resolve_start(store.clone(), forced, ID)
        .task
        .await
        .unwrap()
        .unwrap();
    assert!(!start.dashboard && start.directory == cwd.path());
    let nested = root.path().join("nested");
    fs::create_dir(&nested).unwrap();
    let mut inside = options.clone();
    inside.directory = nested.clone();
    assert_eq!(
        service
            .resolve_start(store.clone(), inside, ID)
            .task
            .await
            .unwrap()
            .unwrap()
            .directory,
        nested
    );
    fs::write(cwd.path().join(".git"), "gitdir: elsewhere").unwrap();
    assert_eq!(
        service
            .resolve_start(store.clone(), options.clone(), ID)
            .task
            .await
            .unwrap()
            .unwrap()
            .directory,
        cwd.path()
    );
    fs::remove_file(cwd.path().join(".git")).unwrap();
    let current = store.registry().unwrap().find("shop").unwrap().clone();
    let archived = store
        .archive_project(&current)
        .unwrap()
        .find("shop")
        .unwrap()
        .clone();
    assert!(
        service
            .resolve_start(store.clone(), options.clone(), ID)
            .task
            .await
            .unwrap()
            .unwrap()
            .dashboard
    );
    let mut missing = archived.clone();
    missing.archived = false;
    missing.path = root.path().join("missing");
    missing.opened_at = Some(now());
    store.save_project(Some(&archived), missing).unwrap();
    assert!(
        service
            .resolve_start(store, options, ID)
            .task
            .await
            .unwrap()
            .unwrap()
            .dashboard
    );
}

#[tokio::test]
async fn management_preserves_typed_identity_and_rejects_stale_forms() {
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let service = BrowserService::new().unwrap();
    let operation = service.manage_projects(
        store.clone(),
        ProjectCommand::Create {
            name: "Shop".into(),
            path: root.path().to_str().unwrap().into(),
        },
        ID,
    );
    assert_eq!(operation.id, ID);
    let response = operation.task.await.unwrap().unwrap();
    let project = response.changed.unwrap();
    let response = service
        .manage_projects(
            store.clone(),
            ProjectCommand::Edit {
                expected: Box::new(project.clone()),
                name: "Edited".into(),
                path: root.path().to_str().unwrap().into(),
            },
            ID,
        )
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.registry.find("shop").unwrap().name, "Edited");
    assert!(
        service
            .manage_projects(
                store.clone(),
                ProjectCommand::Remove {
                    expected: Box::new(project)
                },
                ID
            )
            .task
            .await
            .unwrap()
            .is_err()
    );
    let current = response.registry.find("shop").unwrap().clone();
    let response = service
        .manage_projects(
            store,
            ProjectCommand::Remove {
                expected: Box::new(current),
            },
            ID,
        )
        .task
        .await
        .unwrap()
        .unwrap();
    assert!(response.registry.projects.is_empty());
}

#[tokio::test]
async fn registering_a_nested_unregistered_repository_uses_its_git_root() {
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("src/nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(root.path().join(".git"), "gitdir: elsewhere").unwrap();
    let store = Store::new(config.path().into());
    let service = BrowserService::new().unwrap();
    let path = service
        .register(store.clone(), "Repo".into(), nested, ID)
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(path, root.path());
    assert_eq!(
        store.registry().unwrap().find("repo").unwrap().path,
        root.path()
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    assert!(!root.path().join(".drift").exists());
}
