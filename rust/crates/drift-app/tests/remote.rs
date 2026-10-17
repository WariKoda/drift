#[path = "../../drift-core/tests/support/mod.rs"]
mod support;
use drift_app::{
    browser::{BrowserService, OperationId},
    remote::RemoteService,
};
use drift_core::remote::ConnectionState;
use std::{fs, time::Duration};

#[tokio::test]
async fn remote_sessions_preserve_roots_and_scope_and_close_on_connection_loss() {
    let mut server = support::Server::new(false);
    fs::write(server.dir.path().join("files/text.txt"), "remote text").unwrap();
    fs::write(server.dir.path().join("files/binary"), b"zero\0byte").unwrap();
    let service = RemoteService::with_options(BrowserService::new().unwrap(), server.options());
    let id = OperationId {
        project: 1,
        operation: 1,
    };
    let directory = service
        .connect(server.host(), id)
        .task
        .await
        .unwrap()
        .unwrap();
    let session = directory.session;
    assert!(!session.contains(&format!("{}/../outside", session.root)));
    assert!(!session.contains(&format!("{}-other/file", session.root)));
    assert!(
        service
            .list(session.clone(), "/outside".into(), id)
            .task
            .await
            .unwrap()
            .is_err()
    );
    let preview = service
        .preview(session.clone(), format!("{}/text.txt", session.root), id)
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(preview, "remote text");
    let cancelled = service.preview(session.clone(), format!("{}/text.txt", session.root), id);
    cancelled.cancel.cancel();
    assert!(cancelled.task.await.unwrap().is_err());
    assert_eq!(session.state(), ConnectionState::Connected);
    assert_eq!(
        service
            .preview(session.clone(), format!("{}/text.txt", session.root), id)
            .task
            .await
            .unwrap()
            .unwrap(),
        "remote text"
    );
    assert!(
        service
            .preview(session.clone(), format!("{}/binary", session.root), id)
            .task
            .await
            .unwrap()
            .is_err()
    );
    let observer = service.observe(session.clone(), id);
    server.stop();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), observer.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        ConnectionState::Failed(_)
    ));
    session.shutdown().await;
}
