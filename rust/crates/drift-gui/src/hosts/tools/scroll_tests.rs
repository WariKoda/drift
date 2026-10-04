use super::*;
use crate::sftp_test_support::ftp::Server;
use drift_app::browser::BrowserService;
use drift_core::tlstrust::Manager;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, ScrollDelta, TestAppContext, WindowBounds, WindowOptions, point, px,
    size,
};
use std::{cell::Cell, fs, rc::Rc, sync::Arc, time::Duration};

struct InsetTools {
    tools: Entity<HostTools>,
    inset: f32,
    hidden: bool,
}
impl Render for InsetTools {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().p(px(self.inset)).child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(if self.hidden {
                    div().into_any_element()
                } else {
                    self.tools.clone().into_any_element()
                }),
        )
    }
}

fn fixture(
    store: &Store,
    server: &Server,
    host: Host,
    cx: &mut TestAppContext,
) -> (
    AnyWindowHandle,
    Entity<InsetTools>,
    Entity<HostTools>,
    Arc<Manager>,
) {
    let manager = Arc::new(Manager::with_roots(
        store.clone(),
        rustls::RootCertStore::empty(),
    ));
    let (handle, root) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
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
                cx.new(|cx| InsetTools {
                    tools: cx.new(|cx| {
                        HostTools::new(
                            store.clone(),
                            RemoteService::with_options(
                                BrowserService::new().unwrap(),
                                server.options(),
                            )
                            .with_trust(manager.clone()),
                            None,
                            host,
                            Mode::Test,
                            w,
                            cx,
                        )
                    }),
                    inset: 0.,
                    hidden: false,
                })
            },
        )
        .unwrap()
    });
    let tools = root.read_with(cx, |root, _| root.tools.clone());
    (handle, root, tools, manager)
}

fn frame(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
}
fn press(handle: AnyWindowHandle, key: &str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, cx| w.press(key, cx))
        .unwrap();
    frame(handle, cx);
}
async fn idle(handle: AnyWindowHandle, tools: &Entity<HostTools>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        tools.read(cx).cancel.is_none()
    })
    .await;
    frame(handle, cx);
    frame(handle, cx);
}
fn resize(
    handle: AnyWindowHandle,
    root: &Entity<InsetTools>,
    width: f32,
    height: f32,
    inset: f32,
    cx: &mut TestAppContext,
) {
    root.update(cx, |root, cx| {
        root.inset = inset;
        cx.notify();
    });
    cx.simulate_window_resize(handle, size(px(width), px(height)));
    frame(handle, cx);
    frame(handle, cx);
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.viewport_size(), size(px(width), px(height)));
        let viewport = w.find("host-tools-scroll").bounds();
        assert!(viewport.size.height > px(32.), "a native button must fit");
        assert!(viewport.top() > px(inset));
        assert!(viewport.bottom() < px(height - inset));
        assert!(viewport.left() > px(inset));
        assert!(viewport.right() < px(width - inset));
    })
    .unwrap();
}
fn visible(handle: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, _| {
        let viewport = w.find("host-tools-scroll").bounds();
        let target = w.find(id);
        let bounds = target.bounds();
        assert!(
            target.visible(),
            "{id} clipped: {bounds:?}, viewport {viewport:?}"
        );
        assert!(bounds.size.height > px(0.) && bounds.size.width > px(0.));
        let tolerance = px(1.);
        assert!(
            bounds.top() >= viewport.top() - tolerance
                && bounds.bottom() <= viewport.bottom() + tolerance,
            "{id} outside vertical viewport: {bounds:?}, viewport {viewport:?}"
        );
        assert!(
            bounds.left() >= viewport.left() - tolerance
                && bounds.right() <= viewport.right() + tolerance,
            "{id} outside horizontal viewport: {bounds:?}, viewport {viewport:?}"
        );
    })
    .unwrap();
}
fn tab_to(handle: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) {
    for _ in 0..8 {
        press(handle, "tab", cx);
        if cx
            .update_window(handle, |_, w, _| w.find(id).focused() == Some(true))
            .unwrap()
        {
            visible(handle, id, cx);
            return;
        }
    }
    panic!("Native Tab did not reach {id}");
}

async fn ready_reset(
    handle: AnyWindowHandle,
    tools: &Entity<HostTools>,
    store: &Store,
    cx: &mut TestAppContext,
) -> (Challenge, String) {
    idle(handle, tools, cx).await;
    let challenge = tools.read_with(cx, |tools, cx| {
        tools
            .certificate
            .as_ref()
            .unwrap()
            .read(cx)
            .challenge
            .clone()
    });
    // Use the existing certificate route; its geometry is not owned by HostTools.
    press(handle, "right", cx);
    press(handle, "enter", cx);
    idle(handle, tools, cx).await;
    let success = tools.read_with(cx, |tools, _| {
        assert!(tools.certificate.is_none());
        assert!(tools.status.contains("successful"), "{}", tools.status);
        tools.status.clone()
    });
    store
        .save_trusted_certificate(None, challenge.trust().unwrap())
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        tools.update(cx, |tools, cx| tools.inspect(w, cx));
    })
    .unwrap();
    idle(handle, tools, cx).await;
    tools.read_with(cx, |tools, _| {
        let snapshot = tools.reset.as_ref().unwrap();
        assert_eq!(snapshot.endpoint, challenge.endpoint);
        assert!(snapshot.session.is_some());
        assert!(snapshot.persistent.is_some());
    });
    (challenge, success)
}

#[gpui_kit::test]
async fn tools_short_narrow_inset_tab_reveals_controls_and_native_actions(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = Server::tls("valid");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let host = server.host();
    store.save_host(None, None, host.clone()).unwrap();
    let (handle, root, tools, _) = fixture(&store, &server, host.clone(), cx);
    let (challenge, _) = ready_reset(handle, &tools, &store, cx).await;
    let operation = tools.read_with(cx, |tools, _| tools.operation);
    let pass = server.commands("PASS");
    let trust = store.trusted_certificates().unwrap();
    let targets = [
        ("host-trust-reset-confirm", "Reset certificate trust"),
        ("host-trust-reload", "Reload trust"),
        ("host-test-again", "Test connection"),
        ("host-tools-close", "Return to hosts"),
    ];
    for (width, height, inset) in [(420., 200., 0.), (300., 128., 12.)] {
        resize(handle, &root, width, height, inset, cx);
        cx.update_window(handle, |_, w, cx| tools.read(cx).focus.clone().focus(w, cx))
            .unwrap();
        for (id, label) in targets {
            press(handle, "tab", cx);
            visible(handle, id, cx);
            cx.update_window(handle, |_, w, _| {
                assert_eq!(w.find(id).focused(), Some(true));
                assert_eq!(w.find(id).label(), Some(label));
            })
            .unwrap();
        }
        for (id, _) in targets[..3].iter().rev() {
            press(handle, "shift-tab", cx);
            visible(handle, id, cx);
            cx.update_window(handle, |_, w, _| {
                assert_eq!(w.find(*id).focused(), Some(true))
            })
            .unwrap();
        }
        if inset > 0. {
            cx.update_window(handle, |_, w, _| {
                assert!(
                    w.find("host-tools-reset-actions").bounds().size.height
                        > w.find("host-tools-scroll").bounds().size.height,
                    "reveal must measure the native control, not the oversized action row"
                );
            })
            .unwrap();
        }
    }
    tools.read_with(cx, |tools, _| {
        assert!(tools.host == host);
        assert_eq!(tools.operation, operation);
        assert_eq!(tools.status, "Review certificate trust before resetting");
        assert_eq!(tools.reset.as_ref().unwrap().endpoint, challenge.endpoint);
    });
    assert_eq!(server.commands("PASS"), pass);
    tab_to(handle, "host-trust-reload", cx);
    press(handle, "enter", cx);
    idle(handle, &tools, cx).await;
    assert_eq!(store.trusted_certificates().unwrap(), trust);
    assert_eq!(
        server.commands("PASS"),
        pass,
        "Reload Enter must not reset or connect"
    );
    tab_to(handle, "host-test-again", cx);
    press(handle, "space", cx);
    idle(handle, &tools, cx).await;
    tools.read_with(cx, |tools, _| {
        assert_eq!(tools.operation, operation + 2);
        assert!(tools.status.contains("successful"));
        assert!(tools.reset.is_none());
    });
    assert!(server.commands("PASS") > pass);
    assert_eq!(store.trusted_certificates().unwrap(), trust);
    cx.update_window(handle, |_, w, cx| {
        tools.update(cx, |tools, cx| tools.inspect(w, cx));
    })
    .unwrap();
    idle(handle, &tools, cx).await;
    let pass = server.commands("PASS");
    let closed = Rc::new(Cell::new(false));
    let _subscription = cx.update(|cx| {
        let closed = closed.clone();
        cx.subscribe(&tools, move |_, _: &Closed, _| closed.set(true))
    });
    tab_to(handle, "host-tools-close", cx);
    press(handle, "enter", cx);
    assert!(
        closed.get(),
        "native Return Enter must not be container confirmation"
    );
    assert_eq!(server.commands("PASS"), pass);
    assert_eq!(store.trusted_certificates().unwrap(), trust);
    assert!(tools.read_with(cx, |tools, _| tools.reset.is_some()));
    assert!(store.global().unwrap().hosts == vec![host]);
}

#[gpui_kit::test]
async fn tools_long_details_resize_shrink_and_wheel_preserve_focus_and_state(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = Server::tls("valid");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let mut host = server.host();
    host.name = "A long initiating draft host name ".repeat(12);
    host.root_path = format!("/{}", ["long-remote-deployment-directory"; 12].join("/"));
    fs::create_dir_all(
        server
            .dir
            .path()
            .join("files")
            .join(host.root_path.trim_start_matches('/')),
    )
    .unwrap();
    store.save_host(None, None, host.clone()).unwrap();
    let (handle, root, tools, _) = fixture(&store, &server, host.clone(), cx);
    let (challenge, success) = ready_reset(handle, &tools, &store, cx).await;
    let trust = store.trusted_certificates().unwrap();
    // Display a real long backend result alongside the real inspected trust details.
    tools.update(cx, |tools, cx| {
        tools.status = success.clone();
        cx.notify();
    });
    resize(handle, &root, 480., 260., 18., cx);
    let (operation, scroll) = tools.read_with(cx, |tools, _| {
        (tools.operation, tools.reveal.scroll_handle().clone())
    });
    let pass = server.commands("PASS");
    cx.update_window(handle, |_, w, _| {
        let viewport = w.find("host-tools-scroll").bounds();
        for id in [
            "host-tools-host",
            "host-tools-status",
            "host-tools-endpoint",
            "host-tools-persistent",
            "host-tools-session",
            "host-tools-guidance",
        ] {
            let bounds = w.find(id).bounds();
            assert!(
                bounds.left() >= viewport.left() - px(1.)
                    && bounds.right() <= viewport.right() + px(1.)
            );
        }
        assert!(w.find("host-tools-host").bounds().size.height > px(32.));
        assert!(w.find("host-tools-status").bounds().size.height > px(32.));
        assert!(w.find("host-tools-persistent").bounds().size.height > px(32.));
    })
    .unwrap();
    tab_to(handle, "host-tools-close", cx);
    let focused = cx
        .update_window(handle, |_, w, cx| w.focused(cx).unwrap())
        .unwrap();
    let before = scroll.offset();
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "host-tools-scroll",
            ScrollDelta::Pixels(point(px(0.), px(160.))),
            cx,
        );
    })
    .unwrap();
    frame(handle, cx);
    let after = scroll.offset();
    assert!(after.y > before.y, "wheel must move the actual viewport");
    for _ in 0..4 {
        frame(handle, cx);
    }
    assert_eq!(
        scroll.offset(),
        after,
        "ordinary redraw must not snap to unchanged focus"
    );
    cx.update_window(handle, |_, w, _| assert!(focused.is_focused(w)))
        .unwrap();
    resize(handle, &root, 300., 140., 12., cx);
    visible(handle, "host-tools-close", cx);
    cx.update_window(handle, |_, w, _| assert!(focused.is_focused(w)))
        .unwrap();
    resize(handle, &root, 700., 360., 24., cx);
    visible(handle, "host-tools-close", cx);
    tools.read_with(cx, |tools, _| {
        assert_eq!(tools.status, success);
        assert_eq!(tools.operation, operation);
        assert_eq!(tools.reset.as_ref().unwrap().endpoint, challenge.endpoint);
    });
    // Shorten content at a scrolled offset, without replacing the reveal's lifetime.
    tools.update(cx, |tools, cx| {
        tools.status = "Review certificate trust before resetting".into();
        cx.notify();
    });
    frame(handle, cx);
    frame(handle, cx);
    assert!(scroll.offset().y >= -scroll.max_offset().y);
    visible(handle, "host-tools-close", cx);
    cx.update_window(handle, |_, w, _| assert!(focused.is_focused(w)))
        .unwrap();
    press(handle, "shift-tab", cx);
    visible(handle, "host-test-again", cx);
    tools.read_with(cx, |tools, _| {
        assert!(tools.host == host);
        assert_eq!(tools.operation, operation);
        assert_eq!(tools.reset.as_ref().unwrap().endpoint, challenge.endpoint);
        assert_eq!(tools.status, "Review certificate trust before resetting");
        assert!(tools.cancel.is_none());
    });
    assert_eq!(server.commands("PASS"), pass);
    assert!(store.global().unwrap().hosts == vec![host]);
    assert_eq!(store.trusted_certificates().unwrap(), trust);
}

#[gpui_kit::test]
async fn tools_bounded_reset_keeps_native_space_container_enter_and_busy_guards(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for target in ["native", "container", "shortcut"] {
        let server = Server::tls("valid");
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        let host = server.host();
        store.save_host(None, None, host.clone()).unwrap();
        let (handle, root, tools, manager) = fixture(&store, &server, host.clone(), cx);
        let (challenge, _) = ready_reset(handle, &tools, &store, cx).await;
        resize(handle, &root, 300., 140., 12., cx);
        if target == "native" {
            tab_to(handle, "host-trust-reset-confirm", cx);
        } else if target == "shortcut" {
            tab_to(handle, "host-tools-close", cx);
        } else {
            cx.update_window(handle, |_, w, cx| tools.read(cx).focus.clone().focus(w, cx))
                .unwrap();
        }
        let pass = server.commands("PASS");
        let operation = tools.read_with(cx, |tools, _| tools.operation);
        let closed = Rc::new(Cell::new(false));
        let _subscription = cx.update(|cx| {
            let closed = closed.clone();
            cx.subscribe(&tools, move |_, _: &Closed, _| closed.set(true))
        });
        cx.update_window(handle, |_, w, cx| {
            w.press(
                match target {
                    "native" => "space",
                    "shortcut" => "y",
                    _ => "enter",
                },
                cx,
            );
            assert_eq!(tools.read(cx).operation, operation + 1);
            assert!(tools.read(cx).writing);
            let cancel = tools.read(cx).cancel.clone().unwrap();
            for key in ["y", "r", "enter", "escape"] {
                w.press(key, cx);
                assert_eq!(tools.read(cx).operation, operation + 1);
                assert!(tools.read(cx).writing);
                assert!(!cancel.is_cancelled());
            }
            w.render_frame(cx);
            assert_eq!(
                w.find("host-tools-close").label(),
                Some("Cancel and return")
            );
            assert!(tools.read(cx).writing && tools.read(cx).cancel.is_some());
            assert!(tools.read(cx).reset.is_some());
        })
        .unwrap();
        idle(handle, &tools, cx).await;
        assert!(!closed.get());
        assert_eq!(server.commands("PASS"), pass, "reset must not reconnect");
        assert!(store.trusted_certificates().unwrap().is_empty());
        let snapshot = manager.inspect(challenge.endpoint.clone()).unwrap();
        assert!(snapshot.session.is_none() && snapshot.persistent.is_none());
        tools.read_with(cx, |tools, _| {
            assert!(tools.host == host);
            assert!(tools.reset.is_none());
            assert_eq!(
                tools.status,
                "Certificate trust reset. Future connections require verification again."
            );
        });
        cx.update_window(handle, |_, w, cx| {
            tools.update(cx, |tools, cx| tools.inspect(w, cx))
        })
        .unwrap();
        idle(handle, &tools, cx).await;
        cx.update_window(handle, |_, w, cx| {
            tools.read(cx).focus.clone().focus(w, cx);
            w.press("tab", cx);
            let snapshot = tools.read(cx).reset.as_ref().unwrap();
            assert!(snapshot.persistent.is_none() && snapshot.session.is_none());
            assert_eq!(w.find("host-trust-reload").focused(), Some(true));
        })
        .unwrap();
        frame(handle, cx);
        visible(handle, "host-trust-reload", cx);
        assert!(store.global().unwrap().hosts == vec![host]);
    }
}

#[gpui_kit::test]
async fn tools_busy_cancel_and_current_certificate_route_do_not_retry_or_apply_stale_work(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = Server::tls("valid");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let host = server.host();
    store.save_host(None, None, host.clone()).unwrap();
    let (handle, root, tools, manager) = fixture(&store, &server, host.clone(), cx);
    let (challenge, _) = ready_reset(handle, &tools, &store, cx).await;
    resize(handle, &root, 300., 140., 12., cx);
    let scroll = tools.read_with(cx, |tools, _| tools.reveal.scroll_handle().clone());
    let trust = store.trusted_certificates().unwrap();
    let list = server.commands("LIST");
    server.flag("hold-list", "");
    tab_to(handle, "host-test-again", cx);
    press(handle, "enter", cx);
    cx.wait_for(handle, Duration::from_secs(30), |_, _| {
        server.commands("LIST") > list
    })
    .await;
    let (operation, cancel) = tools.read_with(cx, |tools, _| {
        assert!(!tools.writing);
        assert_eq!(tools.status, "Testing connection…");
        (tools.operation, tools.cancel.clone().unwrap())
    });
    let closed = Rc::new(Cell::new(false));
    let _subscription = cx.update(|cx| {
        let closed = closed.clone();
        cx.subscribe(&tools, move |_, _: &Closed, _| closed.set(true))
    });
    tab_to(handle, "host-tools-close", cx);
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(
            w.find("host-tools-close").label(),
            Some("Cancel and return")
        );
        assert!(tools.read(cx).cancel.is_some() && !tools.read(cx).writing);
    })
    .unwrap();
    press(handle, "space", cx);
    assert!(closed.get());
    assert!(cancel.is_cancelled());
    assert_eq!(
        tools.read_with(cx, |tools, _| tools.operation),
        operation + 1
    );
    // A newer real inspection must not be replaced by the cancelled test's queued result.
    cx.update_window(handle, |_, w, cx| {
        tools.update(cx, |tools, cx| tools.inspect(w, cx))
    })
    .unwrap();
    fs::remove_file(server.dir.path().join("hold-list")).unwrap();
    idle(handle, &tools, cx).await;
    tools.read_with(cx, |tools, _| {
        assert_eq!(tools.operation, operation + 2);
        assert_eq!(tools.status, "Review certificate trust before resetting");
        assert_eq!(tools.reset.as_ref().unwrap().endpoint, challenge.endpoint);
    });
    cx.wait_for(handle, Duration::from_secs(30), |_, _| {
        server.commands("OPEN") == server.commands("CLOSED")
    })
    .await;
    server.flag("rotate-cert", "");
    let pass = server.commands("PASS");
    tab_to(handle, "host-test-again", cx);
    press(handle, "enter", cx);
    idle(handle, &tools, cx).await;
    let prompt = tools.read_with(cx, |tools, cx| {
        assert_eq!(
            tools.status,
            "Certificate approval required for connection test"
        );
        let prompt = tools.certificate.as_ref().unwrap();
        assert_eq!(prompt.read(cx).challenge.endpoint, challenge.endpoint);
        assert_ne!(prompt.read(cx).challenge.fingerprint, challenge.fingerprint);
        prompt.clone()
    });
    cx.update_window(handle, |_, w, cx| {
        assert!(
            w.try_find("host-tools-scroll").is_none(),
            "certificate keeps its separate route"
        );
        assert!(prompt.read(cx).focus_handle(cx).is_focused(w));
    })
    .unwrap();
    let auth = server.commands("AUTH");
    press(handle, "escape", cx);
    frame(handle, cx);
    cx.update_window(handle, |_, w, cx| {
        assert!(tools.read(cx).focus.is_focused(w))
    })
    .unwrap();
    prompt.update(cx, |_, cx| cx.emit(Decision::Session));
    cx.run_until_parked();
    frame(handle, cx);
    tools.read_with(cx, |tools, _| {
        assert!(tools.certificate.is_none() && tools.subscription.is_none());
        assert!(tools.cancel.is_none());
        assert_eq!(tools.status, "FTPS certificate rejected");
        assert!(tools.host == host);
    });
    assert_eq!(server.commands("AUTH"), auth, "stale prompt must not retry");
    assert_eq!(
        server.commands("PASS"),
        pass,
        "unapproved certificate must not authenticate"
    );
    assert_eq!(store.trusted_certificates().unwrap(), trust);
    assert!(
        manager
            .inspect(challenge.endpoint)
            .unwrap()
            .session
            .is_some()
    );
    tab_to(handle, "host-tools-close", cx);
    visible(handle, "host-tools-close", cx);
    assert_eq!(
        scroll.bounds(),
        tools.read_with(cx, |tools, _| tools.reveal.scroll_handle().bounds())
    );
    assert!(store.global().unwrap().hosts == vec![host]);
}

#[gpui_kit::test]
async fn tools_mouse_test_never_restores_removed_reload_focus(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = Server::tls("valid");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let host = server.host();
    store.save_host(None, None, host.clone()).unwrap();
    let (handle, root, tools, _) = fixture(&store, &server, host.clone(), cx);
    ready_reset(handle, &tools, &store, cx).await;
    resize(handle, &root, 420., 700., 12., cx);
    let trust = store.trusted_certificates().unwrap();
    tab_to(handle, "host-trust-reload", cx);
    visible(handle, "host-test-again", cx);
    let removed = cx
        .update_window(handle, |_, w, cx| w.focused(cx).unwrap())
        .unwrap();
    cx.update_window(handle, |_, w, cx| w.click("host-test-again", cx))
        .unwrap();
    idle(handle, &tools, cx).await;
    cx.update_window(handle, |_, w, cx| {
        assert!(tools.read(cx).reset.is_none());
        assert!(tools.read(cx).status.contains("successful"));
        assert!(!removed.is_focused(w));
        assert!(tools.read(cx).focus.is_focused(w));
    })
    .unwrap();
    tab_to(handle, "host-tools-close", cx);
    visible(handle, "host-tools-close", cx);
    assert_eq!(store.trusted_certificates().unwrap(), trust);
    assert!(store.global().unwrap().hosts == vec![host]);
}

#[gpui_kit::test]
async fn tools_reload_restores_only_live_enabled_control_after_new_snapshot(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = Server::tls("valid");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let host = server.host();
    store.save_host(None, None, host.clone()).unwrap();
    let (handle, root, tools, manager) = fixture(&store, &server, host, cx);
    ready_reset(handle, &tools, &store, cx).await;
    resize(handle, &root, 420., 260., 12., cx);
    let trust = store.trusted_certificates().unwrap();
    let pass = server.commands("PASS");
    tab_to(handle, "host-trust-reload", cx);
    for _ in 0..2 {
        press(handle, "enter", cx);
        idle(handle, &tools, cx).await;
        assert_eq!(
            cx.update_window(handle, |_, w, _| w.find("host-trust-reload").focused())
                .unwrap(),
            Some(true)
        );
        visible(handle, "host-trust-reload", cx);
        assert_eq!(store.trusted_certificates().unwrap(), trust);
        assert_eq!(server.commands("PASS"), pass);
    }
    tab_to(handle, "host-trust-reset-confirm", cx);
    let old_focus = cx
        .update_window(handle, |_, w, cx| w.focused(cx).unwrap())
        .unwrap();
    let snapshot = tools.read_with(cx, |tools, _| tools.reset.clone().unwrap());
    manager.reset(&snapshot).unwrap();
    press(handle, "r", cx);
    idle(handle, &tools, cx).await;
    cx.update_window(handle, |_, w, cx| {
        let snapshot = tools.read(cx).reset.as_ref().unwrap();
        assert!(snapshot.session.is_none() && snapshot.persistent.is_none());
        assert!(!old_focus.is_focused(w));
        assert!(tools.read(cx).focus.is_focused(w));
    })
    .unwrap();
    press(handle, "tab", cx);
    assert_eq!(
        cx.update_window(handle, |_, w, _| w.find("host-trust-reload").focused())
            .unwrap(),
        Some(true)
    );
    visible(handle, "host-trust-reload", cx);
    assert_eq!(server.commands("PASS"), pass);
}

#[gpui_kit::test]
async fn tools_completed_reload_enter_before_deferred_focus_never_resets_trust(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = Server::tls("valid");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let host = server.host();
    store.save_host(None, None, host.clone()).unwrap();
    let (handle, root, tools, _) = fixture(&store, &server, host, cx);
    ready_reset(handle, &tools, &store, cx).await;
    let trust = store.trusted_certificates().unwrap();
    let pass = server.commands("PASS");
    tab_to(handle, "host-trust-reload", cx);
    cx.update_window(handle, |_, w, cx| {
        w.press("enter", cx);
        root.update(cx, |root, cx| {
            root.hidden = true;
            cx.notify();
        });
    })
    .unwrap();
    // The real backend settles while the restoration scene is not mounted.
    for _ in 0..3000 {
        cx.run_until_parked();
        if tools.read_with(cx, |tools, _| tools.cancel.is_none()) {
            break;
        }
        cx.update(|cx| cx.background_executor().timer(Duration::from_millis(10)))
            .await;
    }
    let operation = tools.read_with(cx, |tools, _| {
        assert!(tools.cancel.is_none());
        assert!(tools.restore_focus.is_some() && tools.temporary_focus);
        tools.operation
    });
    cx.update_window(handle, |_, w, cx| {
        assert!(tools.read(cx).focus.is_focused(w));
        root.update(cx, |root, cx| {
            root.hidden = false;
            cx.notify();
        });
        w.press("enter", cx);
        assert_eq!(tools.read(cx).operation, operation);
        assert!(tools.read(cx).cancel.is_none() && !tools.read(cx).writing);
    })
    .unwrap();
    frame(handle, cx);
    assert_eq!(store.trusted_certificates().unwrap(), trust);
    assert_eq!(server.commands("PASS"), pass);
    cx.update_window(handle, |_, w, cx| {
        assert!(tools.read(cx).focus.is_focused(w) && tools.read(cx).temporary_focus);
    })
    .unwrap();
    press(handle, "enter", cx);
    assert_eq!(tools.read_with(cx, |tools, _| tools.operation), operation);
    press(handle, "r", cx);
    idle(handle, &tools, cx).await;
    let operation = tools.read_with(cx, |tools, _| {
        assert!(tools.temporary_focus);
        tools.operation
    });
    press(handle, "enter", cx);
    assert_eq!(tools.read_with(cx, |tools, _| tools.operation), operation);
    tab_to(handle, "host-trust-reload", cx);
    press(handle, "enter", cx);
    idle(handle, &tools, cx).await;
    assert_eq!(store.trusted_certificates().unwrap(), trust);
    assert_eq!(server.commands("PASS"), pass);
}
