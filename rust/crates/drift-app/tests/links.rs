use drift_app::{
    browser::{BrowserService, OperationId},
    hosts::{HostCommand, HostResponse},
};
use drift_core::{config::Host, store::Store};

#[tokio::test]
async fn link_management_reports_typed_results_and_cancellation_without_saving_the_destination() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let host = Host {
        name: "stage".into(),
        hostname: "stage.example".into(),
        root_path: "/source".into(),
        ..Host::default()
    };
    store.save_host(Some("source"), None, host).unwrap();
    let service = BrowserService::new().unwrap();
    let id = OperationId {
        project: 7,
        operation: 12,
    };
    let catalogue = service.manage_hosts(
        store.clone(),
        Some("dest".into()),
        HostCommand::LinkTargets,
        id,
    );
    assert_eq!(catalogue.id, id);
    let HostResponse::LinkTargets(catalogue) = catalogue.task.await.unwrap().unwrap() else {
        panic!("wrong response")
    };
    let target = catalogue.targets[0].clone();
    let cancelled = service.manage_hosts(
        store.clone(),
        Some("dest".into()),
        HostCommand::SelectLink {
            expected: Box::new(target.clone()),
        },
        id,
    );
    cancelled.cancel.cancel();
    // Cancellation can stop admission. A commit already running must finish
    // and report its result, so a fast successful promotion is never repeated.
    let promotion = match cancelled.task.await.unwrap() {
        Ok(HostResponse::LinkSelected(promotion)) => promotion,
        Err(_) => {
            assert!(!dir.path().join("config.toml").exists());
            let HostResponse::LinkSelected(promotion) = service
                .manage_hosts(
                    store.clone(),
                    Some("dest".into()),
                    HostCommand::SelectLink {
                        expected: Box::new(target),
                    },
                    id,
                )
                .task
                .await
                .unwrap()
                .unwrap()
            else {
                panic!("wrong response")
            };
            promotion
        }
        Ok(_) => panic!("wrong response"),
    };
    assert_eq!(promotion.server.name, "stage");
    assert!(promotion.warning.is_none());
    assert_eq!(store.project("source").unwrap().hosts[0].server, "stage");
    assert!(!dir.path().join("projects/dest.toml").exists());
    assert!(
        service
            .manage_hosts(store, None, HostCommand::LinkTargets, id)
            .task
            .await
            .unwrap()
            .is_err()
    );
}
