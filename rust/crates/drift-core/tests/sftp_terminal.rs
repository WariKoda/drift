//! Terminal peer violations over the real Go daemon, not a fabricated client.
mod support;

use drift_core::{
    error::Error,
    remote::{self, ConnectionState},
};
use std::{fs, time::Duration};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

async fn malformed_data_is_terminal(policy: &str, preview: bool) {
    let server = support::Server::new(false);
    let root = server.dir.path().join("files");
    let path = root.join("source");
    let target = root.join("deployed");
    let original = vec![b'x'; 128];
    fs::write(&path, &original).unwrap();
    fs::write(&target, b"previous deployment").unwrap();
    for marker in ["record-transport-close", "record-close-status"] {
        fs::write(server.dir.path().join(marker), b"armed\n").unwrap();
    }
    let client = remote::connect(server.host(), server.options(), CancellationToken::new())
        .await
        .unwrap();
    let mut state = client.state();
    let upload_source = client.open(path.to_str().unwrap()).await.unwrap();
    let mut reader = if preview {
        None
    } else {
        Some(client.open(path.to_str().unwrap()).await.unwrap())
    };
    assert_eq!(*state.borrow(), ConnectionState::Connected);
    fs::write(
        server.dir.path().join(format!("arm-{policy}-read")),
        b"armed\n",
    )
    .unwrap();

    if let Some(reader) = reader.as_mut() {
        let mut bytes = [0xa5; 8];
        let error = tokio::time::timeout(Duration::from_secs(5), reader.read(&mut bytes))
            .await
            .expect("real READ hung")
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("empty or oversized SFTP DATA response"),
            "ReadFile must reject decoded DATA, not a transport/framing error: {error}"
        );
        assert_eq!(bytes, [0xa5; 8], "malformed DATA must not reach the caller");
    } else {
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            client.read_limited(path.to_str().unwrap(), 1024),
        )
        .await
        .expect("real preview hung")
        .unwrap_err();
        assert!(matches!(error, Error::Connection(_)), "{error}");
        assert!(
            error
                .to_string()
                .contains("empty or oversized SFTP DATA response")
        );
    }

    // Keep both the client and streaming reader alive. Neither Drop, Sync nor
    // explicit shutdown may be responsible for making the state terminal.
    tokio::time::timeout(Duration::from_secs(5), async {
        while *state.borrow_and_update() == ConnectionState::Connected {
            state.changed().await.unwrap();
        }
        loop {
            if fs::read_to_string(server.dir.path().join("ssh-connections"))
                .ok()
                .as_deref()
                == Some("closed\n")
                && fs::read_to_string(server.dir.path().join("sftp-resources"))
                    .ok()
                    .as_deref()
                    == Some("released\n")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("ReadFile failure must cancel the monitor and release SSH/SFTP resources");
    assert!(matches!(
        &*state.borrow(),
        ConnectionState::Closed | ConnectionState::Failed(_)
    ));
    assert_eq!(
        fs::read_to_string(server.dir.path().join("ssh-connections")).unwrap(),
        "closed\n"
    );
    assert_eq!(
        fs::read_to_string(server.dir.path().join("sftp-resources")).unwrap(),
        "released\n"
    );

    let log = fs::read_to_string(server.dir.path().join("sftp-operations")).unwrap();
    let faults: Vec<_> = log
        .lines()
        .filter(|line| line.starts_with("fault-data\t"))
        .collect();
    assert_eq!(
        faults.len(),
        1,
        "one actual DATA reply was corrupted: {log}"
    );
    let fields: Vec<_> = faults[0].split('\t').collect();
    assert_eq!(fields[1], policy);
    let requested: usize = fields[3].parse().unwrap();
    let native: usize = fields[4].parse().unwrap();
    let corrupted: usize = fields[5].parse().unwrap();
    assert!(
        native > 0 && native <= requested,
        "the daemon read actual file bytes"
    );
    assert_eq!(corrupted, if policy == "empty" { 0 } else { requested + 1 });
    if !preview {
        assert_eq!(requested, 8);
        assert_eq!(native, 8);
    }

    assert!(matches!(
        client.delete(target.to_str().unwrap()).await.unwrap_err(),
        Error::Connection(_)
    ));
    assert!(matches!(
        client.stat(path.to_str().unwrap()).await.unwrap_err(),
        Error::Connection(_)
    ));
    assert!(matches!(
        client.open(path.to_str().unwrap()).await.err().unwrap(),
        Error::Connection(_)
    ));
    assert!(matches!(
        client
            .read_limited(path.to_str().unwrap(), 1024)
            .await
            .unwrap_err(),
        Error::Connection(_)
    ));
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        client.upload(target.to_str().unwrap(), upload_source),
    )
    .await
    .expect("upload on terminal client hung")
    .unwrap_err();
    assert!(matches!(error, Error::Connection(_)), "{error}");
    assert_eq!(fs::read(&target).unwrap(), b"previous deployment");
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 2, "no staged upload");
    assert_eq!(
        fs::read_to_string(server.dir.path().join("sftp-operations")).unwrap(),
        log,
        "terminal client must not send new operations"
    );
    drop(reader);

    // The fixture did not kill SSH or disable CLOSE. A fresh production client
    // can read to real EOF and receive the native successful CLOSE ACK.
    fs::remove_file(server.dir.path().join(format!("arm-{policy}-read"))).unwrap();
    let healthy = remote::connect(server.host(), server.options(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        healthy
            .read_limited(path.to_str().unwrap(), 1024)
            .await
            .unwrap(),
        original
    );
    assert_eq!(*healthy.state().borrow(), ConnectionState::Connected);
    assert!(
        fs::read_to_string(server.dir.path().join("sftp-operations"))
            .unwrap()
            .contains("close-status\t0\n")
    );
    healthy.shutdown().await;
}

#[tokio::test]
async fn empty_data_from_normal_open_read_tears_down_without_reader_drop() {
    malformed_data_is_terminal("empty", false).await;
}

#[tokio::test]
async fn oversized_data_from_normal_open_read_tears_down_without_reader_drop() {
    malformed_data_is_terminal("oversized", false).await;
}

#[tokio::test]
async fn empty_data_from_preview_tears_down_and_blocks_mutation() {
    malformed_data_is_terminal("empty", true).await;
}

#[tokio::test]
async fn oversized_data_from_preview_tears_down_and_blocks_mutation() {
    malformed_data_is_terminal("oversized", true).await;
}

#[tokio::test]
async fn real_eof_is_not_an_empty_data_violation_and_close_still_succeeds() {
    for policy in ["empty", "oversized"] {
        let server = support::Server::new(false);
        let path = server.dir.path().join("files/empty");
        fs::write(&path, b"").unwrap();
        fs::write(
            server.dir.path().join(format!("arm-{policy}-read")),
            b"armed\n",
        )
        .unwrap();
        fs::write(server.dir.path().join("record-close-status"), b"armed\n").unwrap();
        let client = remote::connect(server.host(), server.options(), CancellationToken::new())
            .await
            .unwrap();
        let mut reader = client.open(path.to_str().unwrap()).await.unwrap();
        assert_eq!(reader.read(&mut [0; 8]).await.unwrap(), 0);
        reader.close().await.unwrap();
        assert!(
            client
                .read_limited(path.to_str().unwrap(), 8)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(*client.state().borrow(), ConnectionState::Connected);
        let log = fs::read_to_string(server.dir.path().join("sftp-operations")).unwrap();
        assert!(
            !log.contains("fault-data\t"),
            "native EOF STATUS was preserved"
        );
        assert_eq!(
            log.lines()
                .filter(|line| *line == "close-status\t0")
                .count(),
            2
        );
        client.shutdown().await;
    }
}
