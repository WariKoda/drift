use super::*;

#[tokio::test]
async fn another_peers_terminal_read_failure_reclaims_our_healthy_destination_stage() {
    let server = fixture(&["restricted-rename"]);
    let source_server = fixture(&["arm-oversized-read"]);
    let path = target(&server, true);
    let client = connect(&server).await;
    let source_client = connect(&source_server).await;
    let source = source_client
        .open(
            source_server
                .dir
                .path()
                .join("files/source")
                .to_str()
                .unwrap(),
        )
        .await
        .unwrap();
    let error = tokio::time::timeout(Duration::from_secs(10), client.upload(&path, source))
        .await
        .unwrap()
        .unwrap_err();
    assert!(matches!(error, Error::Connection(_)), "{error}");
    contents(&path, OLD, Some(MODE));
    no_stages(&server.dir.path().join("files"));
    let log = operations(&server);
    assert_eq!(count(&log, "posix"), 0);
    assert_eq!(count(&log, "standard"), 0);
    assert_eq!(count(&log, "remove"), 1);
    stage_used(&log, &path);
    never_delete_target(&log, &path);
    single_channel(&server);
    single_channel(&source_server);
    client.shutdown().await;
    source_client.shutdown().await;
}
