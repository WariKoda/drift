use drift_app::{
    browser::{BrowserService, OperationId},
    hosts::{HostCommand, HostResponse},
    logging::{Logger, Options},
};
use drift_core::{config::Host, error::Error, store::Store};
use std::{fs, io::Write, os::unix::fs::PermissionsExt, time::Duration};

fn host(name: &str) -> Host {
    Host {
        name: name.into(),
        hostname: "deploy.example".into(),
        user: "deploy".into(),
        root_path: format!("/srv/{name}"),
        ..Host::default()
    }
}
fn id() -> OperationId {
    OperationId {
        project: 7,
        operation: 12,
    }
}

#[tokio::test]
async fn offers_are_typed_readonly_project_only_and_use_destination_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store.save_host(None, None, host("shared")).unwrap();
    store
        .save_host(
            None,
            None,
            Host {
                hostname: "elsewhere".into(),
                ..host("unmatched")
            },
        )
        .unwrap();
    store.save_host(Some("dest"), None, host("before")).unwrap();
    fs::write(
        dir.path().join("projects/dest.toml"),
        "[defaults]\nuser='deploy'\nport=22\n[[hosts]]\nname='before'\nhostname='deploy.example'\n",
    )
    .unwrap();
    let before_global = fs::read(dir.path().join("config.toml")).unwrap();
    let before_dest = fs::read(dir.path().join("projects/dest.toml")).unwrap();
    let desired = Host {
        user: String::new(),
        ..host("unsaved")
    };
    let service = BrowserService::new().unwrap();
    let operation = service.manage_hosts(
        store.clone(),
        Some("dest".into()),
        HostCommand::OfferLink {
            desired: Box::new(desired.clone()),
        },
        id(),
    );
    assert_eq!(operation.id, id());
    let HostResponse::LinkOffer(offer) = operation.task.await.unwrap().unwrap() else {
        panic!("wrong response")
    };
    assert_eq!(offer.targets.len(), 1);
    assert_eq!(offer.targets[0].host.name, "shared");
    assert_eq!(
        fs::read(dir.path().join("config.toml")).unwrap(),
        before_global
    );
    assert_eq!(
        fs::read(dir.path().join("projects/dest.toml")).unwrap(),
        before_dest
    );
    assert!(
        service
            .manage_hosts(
                store.clone(),
                None,
                HostCommand::OfferLink {
                    desired: Box::new(desired.clone())
                },
                id()
            )
            .task
            .await
            .unwrap()
            .is_err()
    );
    let linked = Host {
        name: "linked".into(),
        server: "shared".into(),
        ..Host::default()
    };
    assert!(
        service
            .manage_hosts(
                store.clone(),
                Some("dest".into()),
                HostCommand::OfferLink {
                    desired: Box::new(linked)
                },
                id()
            )
            .task
            .await
            .unwrap()
            .is_err()
    );
    let HostResponse::LinkSaved(saved) = service
        .manage_hosts(
            store.clone(),
            Some("dest".into()),
            HostCommand::SaveLink {
                expected: None,
                desired: Box::new(desired),
                target: Box::new(offer.targets[0].clone()),
            },
            id(),
        )
        .task
        .await
        .unwrap()
        .unwrap()
    else {
        panic!("wrong response")
    };
    assert!(saved.warning.is_none() && saved.destination_error.is_none());
    assert_eq!(store.project("dest").unwrap().hosts[1].server, "shared");
    assert_eq!(
        fs::read(dir.path().join("config.toml")).unwrap(),
        before_global
    );
}

#[tokio::test]
async fn host_management_logs_safe_error_categories_and_generic_partial_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("store"));
    let mut source = host("source-private-name");
    source.auth.password = "credential-must-not-be-logged".into();
    store.save_host(Some("source"), None, source).unwrap();
    let target = store.link_targets("dest").unwrap().targets.remove(0);
    let log = dir.path().join("diagnostics.log");
    let logger = Logger::open(Options {
        path: log.clone(),
        debug: true,
    })
    .unwrap();
    let service = BrowserService::new().unwrap().with_logger(logger.clone());
    let projects = store.dir().join("projects");
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o500)).unwrap();
    let result = service
        .manage_hosts(
            store.clone(),
            Some("dest".into()),
            HostCommand::SaveLink {
                expected: None,
                desired: Box::new(host("destination-private-name")),
                target: Box::new(target),
            },
            id(),
        )
        .task
        .await
        .unwrap();
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o700)).unwrap();
    let HostResponse::LinkSaved(saved) = result.unwrap() else {
        panic!("wrong response")
    };
    assert_eq!(saved.server.name, "source-private-name");
    assert!(saved.warning.is_some());
    assert!(matches!(saved.destination_error, Some(Error::Io(_))));
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    // No automatic retry. The retained server can be used by an explicit Save.
    let draft = host("destination-private-name");
    let link = Host {
        name: draft.name,
        root_path: draft.root_path,
        server: saved.server.name,
        ..Host::default()
    };
    let HostResponse::Saved = service
        .manage_hosts(
            store.clone(),
            Some("dest".into()),
            HostCommand::Save {
                expected: None,
                desired: Box::new(link),
            },
            id(),
        )
        .task
        .await
        .unwrap()
        .unwrap()
    else {
        panic!("wrong response")
    };
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    // A TOML decoder error may carry source lines with credentials. Log only its category.
    fs::write(
        store.dir().join("config.toml"),
        "password = 'decode-private-secret'\ninvalid = [\n",
    )
    .unwrap();
    let result = service
        .manage_hosts(
            store,
            Some("dest".into()),
            HostCommand::OfferLink {
                desired: Box::new(host("unsaved")),
            },
            id(),
        )
        .task
        .await
        .unwrap();
    assert!(matches!(result, Err(Error::Decode(_))));
    tokio::task::spawn_blocking(move || service.wait_for_shutdown())
        .await
        .unwrap();
    logger.finish().unwrap();
    let text = fs::read_to_string(log).unwrap();
    assert!(text.contains("host link save partially completed"));
    assert!(text.contains("host link destination save failed"));
    assert!(text.contains("error_kind=\"io\""));
    assert!(text.contains("error_kind=\"config_decode\""));
    for secret in [
        "credential-must-not-be-logged",
        "decode-private-secret",
        "source-private-name",
        "destination-private-name",
        "password",
        "invalid =",
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
}

#[tokio::test]
async fn cancellation_after_mutation_admission_preserves_the_committed_result() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store.save_host(Some("source"), None, host("prod")).unwrap();
    let target = store.link_targets("dest").unwrap().targets.remove(0);
    let fifo = dir.path().join("config.toml");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let service = BrowserService::new().unwrap();
    let operation = service.manage_hosts(
        store.clone(),
        Some("dest".into()),
        HostCommand::SaveLink {
            expected: None,
            desired: Box::new(host("destination")),
            target: Box::new(target),
        },
        id(),
    );
    // Opening the FIFO writer succeeds only after the admitted mutation has
    // acquired its lock and started reading global configuration.
    let mut writer = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::task::spawn_blocking(move || fs::OpenOptions::new().write(true).open(fifo)),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    operation.cancel.cancel();
    writer.write_all(b"[defaults]\n").unwrap();
    drop(writer);
    let HostResponse::LinkSaved(saved) =
        tokio::time::timeout(Duration::from_secs(5), operation.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    else {
        panic!("wrong response")
    };
    assert_eq!(saved.server.name, "prod");
    assert!(saved.warning.is_none() && saved.destination_error.is_none());
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    assert_eq!(store.project("source").unwrap().hosts[0].server, "prod");
    assert_eq!(store.project("dest").unwrap().hosts[0].server, "prod");
}

#[tokio::test]
async fn cancellation_after_admission_preserves_partial_commit_information() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store.save_host(Some("source"), None, host("prod")).unwrap();
    let target = store.link_targets("dest").unwrap().targets.remove(0);
    let fifo = dir.path().join("config.toml");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let service = BrowserService::new().unwrap();
    let projects = store.dir().join("projects");
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o500)).unwrap();
    let operation = service.manage_hosts(
        store.clone(),
        Some("dest".into()),
        HostCommand::SaveLink {
            expected: None,
            desired: Box::new(host("destination")),
            target: Box::new(target),
        },
        id(),
    );
    let mut writer = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::task::spawn_blocking(move || fs::OpenOptions::new().write(true).open(fifo)),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    operation.cancel.cancel();
    writer.write_all(b"[defaults]\n").unwrap();
    drop(writer);
    let result = tokio::time::timeout(Duration::from_secs(5), operation.task)
        .await
        .unwrap()
        .unwrap();
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o700)).unwrap();
    let HostResponse::LinkSaved(saved) = result.unwrap() else {
        panic!("wrong response")
    };
    assert_eq!(saved.server.name, "prod");
    assert!(saved.warning.is_some());
    assert!(matches!(saved.destination_error, Some(Error::Io(_))));
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    assert!(!dir.path().join("projects/dest.toml").exists());
}
