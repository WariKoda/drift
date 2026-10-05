use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, App, Bounds, InputEvent as _, KeyDownEvent, KeyUpEvent, Keystroke,
    MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext, WindowBounds, WindowOptions, point,
    px, size,
};
use std::time::Duration;

struct Fixture {
    handle: AnyWindowHandle,
    shell: Entity<Shell>,
    store: Store,
    slug: String,
    _local: tempfile::TempDir,
    _config: tempfile::TempDir,
}
impl Fixture {
    async fn new(cx: &mut TestAppContext) -> Self {
        cx.executor().allow_parking();
        let local = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        let slug = store
            .register("Offer project", local.path().to_path_buf())
            .unwrap()
            .slug;
        store
            .save_host(
                None,
                None,
                drift_core::config::Host {
                    name: "shared".into(),
                    hostname: "deploy.example".into(),
                    user: "deploy".into(),
                    root_path: "/server".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let (handle, shell) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::actions::bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1200.), px(800.)),
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
                            RemoteService::new(service),
                            local.path().to_path_buf(),
                        )
                    })
                },
            )
            .unwrap()
        });
        cx.simulate_window_resize(handle, size(px(1200.), px(800.)));
        cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
            let s = shell.read(cx);
            s.browser.read(cx).location().is_some() && !s.browser.read(cx).is_loading()
        })
        .await;
        cx.update_window(handle, |_, w, cx| w.click("hosts", cx))
            .unwrap();
        cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
            shell
                .read(cx)
                .hosts
                .as_ref()
                .is_some_and(|hosts| !hosts.read(cx).is_loading())
        })
        .await;
        Self {
            handle,
            shell,
            store,
            slug,
            _local: local,
            _config: config,
        }
    }
    fn frame(&self, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    async fn draft(&self, hostname: &str, cx: &mut TestAppContext) {
        cx.wait_for(self.handle, Duration::from_secs(30), |_, cx| {
            self.shell
                .read(cx)
                .hosts
                .as_ref()
                .is_some_and(|hosts| !hosts.read(cx).is_loading())
        })
        .await;
        cx.update_window(self.handle, |_, w, cx| w.click("host-new", cx))
            .unwrap();
        for (field, value) in [
            ("host-name", "destination"),
            ("hostname", hostname),
            ("user", "deploy"),
            ("root-path", "/own-root"),
        ] {
            cx.update_window(self.handle, |_, w, cx| {
                for _ in 0..96 {
                    w.render_frame(cx);
                    if w.find(field).focused() == Some(true) {
                        break;
                    }
                    w.press("tab", cx);
                }
                assert_eq!(w.find(field).focused(), Some(true));
                w.press(
                    if cfg!(target_os = "macos") {
                        "cmd-a"
                    } else {
                        "ctrl-a"
                    },
                    cx,
                );
                w.input(value, cx);
            })
            .unwrap();
        }
        self.frame(cx);
    }
}
fn key(w: &mut Window, cx: &mut App, name: &str) {
    let keystroke = Keystroke::parse(name).unwrap();
    w.dispatch_event(
        KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        }
        .to_platform_input(),
        cx,
    );
    w.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
}

#[gpui_kit::test]
async fn serverlink_offer_covering_help_rejects_old_pointer_and_letter_approval(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    f.draft("deploy.example", cx).await;
    cx.update_window(f.handle, |_, w, cx| w.press("ctrl-s", cx))
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |w, _| {
        w.try_find("host-link-offer-use").is_some()
    })
    .await;
    f.frame(cx);
    cx.update_window(f.handle, |_, w, cx| {
        let position = w.find("host-link-offer-use").bounds().center();
        key(w, cx, "f1");
        let focus = w.focused(cx);
        assert!(f.shell.read(cx).help.is_some());
        w.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        w.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        for value in ["y", "n", "ctrl-s", "enter", "space"] {
            key(w, cx, value);
        }
        assert!(w.focused(cx) == focus);
        assert!(f.shell.read(cx).help.is_some());
    })
    .unwrap();
    f.frame(cx);
    cx.executor().timer(Duration::from_millis(100)).await;
    assert!(f.store.project(&f.slug).unwrap().hosts.is_empty());
    cx.update_window(f.handle, |_, w, cx| w.press("escape", cx))
        .unwrap();
    f.frame(cx);
    cx.update_window(f.handle, |_, w, cx| w.press("y", cx))
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.store.project(&f.slug).unwrap().hosts.len() == 1
            && f.shell
                .read(cx)
                .hosts
                .as_ref()
                .is_some_and(|hosts| !hosts.read(cx).is_loading())
    })
    .await;
    let hosts = f.store.project(&f.slug).unwrap().hosts;
    assert!(hosts.len() == 1 && hosts[0].server == "shared" && hosts[0].root_path == "/own-root");
}

#[gpui_kit::test]
async fn serverlink_no_match_save_completion_keeps_help_focus_and_restores_current_host_view(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    f.draft("own.example", cx).await;
    let help_focus = cx
        .update_window(f.handle, |_, w, cx| {
            key(w, cx, "ctrl-s");
            key(w, cx, "f1");
            assert!(f.shell.read(cx).help.is_some());
            w.focused(cx).unwrap()
        })
        .unwrap();
    f.frame(cx);
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.store.project(&f.slug).unwrap().hosts.len() == 1
            && f.shell
                .read(cx)
                .hosts
                .as_ref()
                .is_some_and(|hosts| !hosts.read(cx).is_loading())
            && f.shell.read(cx).configuration > 0
            && f.shell.read(cx).config_cancel.is_none()
    })
    .await;
    f.frame(cx);
    cx.update_window(f.handle, |_, w, cx| {
        assert!(w.focused(cx) == Some(help_focus));
        assert!(f.shell.read(cx).help.is_some());
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(f.shell.read(cx).help.is_none());
        assert!(w.try_find("host-save").is_none());
        for _ in 0..48 {
            w.press("tab", cx);
            w.render_frame(cx);
            if w.find("host-new").focused() == Some(true) {
                return;
            }
        }
        panic!("current host controls must remain natively reachable after help");
    })
    .unwrap();
    let hosts = f.store.project(&f.slug).unwrap().hosts;
    assert!(hosts.len() == 1 && hosts[0].server.is_empty() && hosts[0].hostname == "own.example");
}
