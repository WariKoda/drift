use super::*;
use crate::{actions::bind_keys, sftp_test_support as support};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
};
use std::{fs, time::Duration};

struct Fixture {
    handle: AnyWindowHandle,
    shell: Entity<Shell>,
    server: support::Server,
    local: tempfile::TempDir,
    _config: tempfile::TempDir,
}
impl Fixture {
    async fn new(trusted: bool, cx: &mut TestAppContext) -> Self {
        cx.executor().allow_parking();
        let server = support::Server::certified(Default::default());
        fs::write(
            server.dir.path().join("files/remote.txt"),
            "certified remote content",
        )
        .unwrap();
        let local = tempfile::tempdir().unwrap();
        fs::write(local.path().join("local.txt"), "unchanged local content").unwrap();
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        store.save_host(None, None, server.host()).unwrap();
        if trusted {
            Self::trust(&server);
        }
        let (handle, shell) = cx.update(|cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1400.), px(900.)),
                    })),
                    ..Default::default()
                },
                cx,
                |w, cx| {
                    cx.new(|cx| {
                        let service = BrowserService::new().unwrap();
                        Shell::new(
                            w,
                            cx,
                            store.clone(),
                            service.clone(),
                            RemoteService::with_options(service, server.options()),
                            local.path().to_path_buf(),
                        )
                    })
                },
            )
            .unwrap()
        });
        cx.simulate_window_resize(handle, size(px(1400.), px(900.)));
        cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
            shell.read(cx).browser.read(cx).location().is_some()
                && !shell.read(cx).browser.read(cx).is_loading()
        })
        .await;
        Self {
            handle,
            shell,
            server,
            local,
            _config: config,
        }
    }
    fn trust(server: &support::Server) {
        let path = server.options().known_hosts;
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            format!(
                "@cert-authority [127.0.0.1]:{} {}",
                server.port,
                fs::read_to_string(server.dir.path().join("ca-key.pub")).unwrap()
            ),
        )
        .unwrap();
    }
    async fn connect(&self, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, w, cx| {
            w.click("remote", cx);
            w.click(("connect-host", 0usize), cx);
        })
        .unwrap();
        cx.wait_for(self.handle, Duration::from_secs(30), |_, cx| {
            !self.shell.read(cx).remote.read(cx).is_loading()
        })
        .await;
    }
}
#[gpui_kit::test]
async fn certified_ssh_browser_preview_and_help_do_not_transfer_or_open_tls_approval(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(true, cx).await;
    let trust = fs::read(f.server.options().known_hosts).unwrap();
    f.connect(cx).await;
    assert!(
        f.shell
            .read_with(cx, |s, cx| s.remote.read(cx).has_session()
                && s.certificate.is_none())
    );
    let session = f
        .shell
        .read_with(cx, |s, cx| s.remote.read(cx).session_id());
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote-filter", cx);
        w.press("enter", cx);
        w.press("home", cx);
        w.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).remote_preview && !f.shell.read(cx).preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("certified remote content")
        );
        w.press("f1", cx);
    })
    .unwrap();
    assert!(
        f.shell
            .read_with(cx, |s, _| s.help.is_some() && s.certificate.is_none())
    );
    cx.update_window(f.handle, |_, w, cx| w.press("escape", cx))
        .unwrap();
    assert!(
        f.shell
            .read_with(cx, |s, cx| s.remote.read(cx).has_session()
                && s.remote.read(cx).session_id() == session)
    );
    assert_eq!(
        fs::read_to_string(f.local.path().join("local.txt")).unwrap(),
        "unchanged local content"
    );
    assert_eq!(
        fs::read_to_string(f.server.dir.path().join("files/remote.txt")).unwrap(),
        "certified remote content"
    );
    assert!(fs::read(f.server.options().known_hosts).unwrap() == trust);
    assert!(!f._config.path().join("trusted-certificates.toml").exists());
}
#[gpui_kit::test]
async fn untrusted_ssh_certificate_rejects_without_tofu_tls_prompt_or_automatic_retry(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(false, cx).await;
    let browser = f.shell.read_with(cx, |s, cx| {
        (
            s.browser.entity_id(),
            s.browser.read(cx).id(),
            s.browser.read(cx).selected().map(str::to_owned),
        )
    });
    f.connect(cx).await;
    assert!(
        f.shell
            .read_with(cx, |s, cx| !s.remote.read(cx).has_session()
                && s.certificate.is_none()
                && !s.remote.read(cx).is_loading())
    );
    assert!(!f.server.options().known_hosts.exists());
    assert!(f.shell.read_with(cx, |s, _| !s.status.is_empty()));
    let connection = f
        .shell
        .read_with(cx, |s, cx| s.remote.read(cx).connection());
    Fixture::trust(&f.server);
    cx.executor().timer(Duration::from_millis(100)).await;
    assert!(
        f.shell
            .read_with(cx, |s, cx| !s.remote.read(cx).has_session()
                && s.remote.read(cx).connection() == connection)
    );
    cx.update_window(f.handle, |_, w, cx| w.click(("connect-host", 0usize), cx))
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    assert!(
        f.shell
            .read_with(cx, |s, cx| s.remote.read(cx).has_session()
                && s.certificate.is_none())
    );
    assert!(f.shell.read_with(cx, |s, cx| (
        s.browser.entity_id(),
        s.browser.read(cx).id(),
        s.browser.read(cx).selected().map(str::to_owned)
    ) == browser));
    assert_eq!(
        fs::read_to_string(f.local.path().join("local.txt")).unwrap(),
        "unchanged local content"
    );
    assert!(!f._config.path().join("trusted-certificates.toml").exists());
}
