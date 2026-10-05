use super::*;
use drift_core::config::Auth;
use gpui_kit::test::{TestAppContextExt, TestWindowExt as NativeWindowTestExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, ElementId, EntityInputHandler as _, InputEvent as _, KeyDownEvent,
    KeyUpEvent, Keystroke, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, ScrollDelta,
    TestAppContext, WindowBounds, WindowOptions, point, size,
};
use std::{fs, ops::Range, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

const BACK: &str = "host-link-offer-back";
const OWN: &str = "host-link-offer-own";
const USE: &str = "host-link-offer-use";

fn host(name: &str) -> Host {
    Host {
        name: name.into(),
        hostname: "deploy.example".into(),
        user: "deploy".into(),
        protocol: "sftp".into(),
        root_path: "/srv/日本 app ".into(),
        auth: Auth {
            kind: "password".into(),
            password: " 日本🦀e\u{301} exact secret ".into(),
            ..Default::default()
        },
        keep_alive_interval: Some(120),
        mappings: vec![Mapping {
            local: "src/日本 app".into(),
            remote: "deploy/日本 app".into(),
        }],
        ..Default::default()
    }
}

fn server(name: &str) -> Host {
    Host {
        hostname: "DEPLOY.EXAMPLE".into(),
        port: 22,
        root_path: "/unrelated/server/root".into(),
        auth: Auth {
            kind: "keyfile".into(),
            key_file: "~/.ssh/server-only-key".into(),
            passphrase: "server-only-passphrase\tseparate\nline".into(),
            ..Default::default()
        },
        keep_alive_interval: Some(0),
        mappings: vec![],
        ..host(name)
    }
}

fn link(desired: &Host, name: &str) -> Host {
    Host {
        name: desired.name.clone(),
        server: name.into(),
        root_path: desired.root_path.clone(),
        mappings: desired.mappings.clone(),
        ..Default::default()
    }
}

// Compare bytes without printing credentials on a failing assertion.
fn stored_bytes(store: &Store) -> Vec<(PathBuf, Vec<u8>)> {
    let mut paths = vec![store.dir().join("config.toml")];
    if store.dir().join("projects").is_dir() {
        paths.extend(
            fs::read_dir(store.dir().join("projects"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.is_file()),
        );
    }
    paths.sort();
    paths
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
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

fn tab_to(handle: AnyWindowHandle, id: impl Into<ElementId>, cx: &mut TestAppContext) {
    let id = id.into();
    for _ in 0..160 {
        press(handle, "tab", cx);
        if cx
            .update_window(handle, |_, w, _| w.find(id.clone()).focused() == Some(true))
            .unwrap()
        {
            return;
        }
    }
    panic!("Native Tab did not reach {id:?}");
}

async fn idle(handle: AnyWindowHandle, manager: &Entity<HostManager>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        manager.read(cx).cancel.is_none()
    })
    .await;
    frame(handle, cx);
}

async fn ready(handle: AnyWindowHandle, manager: &Entity<HostManager>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        let m = manager.read(cx);
        m.cancel.is_none()
            && m.offer
                .as_ref()
                .is_some_and(|offer| offer.phase == Phase::Ready)
    })
    .await;
    frame(handle, cx);
}

async fn fixture(
    store: &Store,
    slug: Option<&str>,
    desired: Host,
    expected: Option<Host>,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<HostManager>) {
    cx.executor().allow_parking();
    let (handle, manager) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1100.), px(760.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    let service = BrowserService::new().unwrap();
                    HostManager::new(
                        store.clone(),
                        service.clone(),
                        RemoteService::new(service),
                        slug.map(str::to_owned),
                        w,
                        cx,
                    )
                })
            },
        )
        .unwrap()
    });
    cx.simulate_window_resize(handle, size(px(1100.), px(760.)));
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(w.viewport_size(), size(px(1100.), px(760.)));
        // Only fixture establishment uses the model; Save and every decision use native input.
        manager.update(cx, |m, cx| {
            m.form = Some(HostForm::new(desired, expected, w, cx).unwrap());
            m.status.clear();
            cx.notify();
        });
    })
    .unwrap();
    frame(handle, cx);
    (handle, manager)
}

async fn offer(handle: AnyWindowHandle, manager: &Entity<HostManager>, cx: &mut TestAppContext) {
    press(handle, "ctrl-s", cx);
    ready(handle, manager, cx).await;
}

type InputSnapshot = (
    Entity<InputState>,
    String,
    Range<usize>,
    Option<Range<usize>>,
    Point<Pixels>,
    bool,
);

fn inputs(manager: &Entity<HostManager>, w: &mut Window, cx: &mut App) -> Vec<InputSnapshot> {
    let fields = {
        let form = manager.read(cx).form.as_ref().unwrap();
        form.fields
            .iter()
            .chain(
                form.mappings
                    .iter()
                    .flat_map(|(local, remote)| [local, remote]),
            )
            .cloned()
            .collect::<Vec<_>>()
    };
    fields
        .into_iter()
        .map(|field| {
            let mark = field.update(cx, |state, cx| state.marked_text_range(w, cx));
            let state = field.read(cx);
            (
                field.clone(),
                state.value().to_string(),
                state.selected_range(),
                mark,
                state.scroll_offset(),
                state.presentation().is_masked(),
            )
        })
        .collect()
}

fn raw_key(w: &mut Window, cx: &mut App, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
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

fn raw_down(w: &mut Window, cx: &mut App, position: Point<Pixels>) {
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
}

fn raw_up(w: &mut Window, cx: &mut App, position: Point<Pixels>) {
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
}

fn visible_in_offer(handle: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, _| {
        let viewport = w.find("host-link-offer").bounds();
        let button = w.find(id);
        let bounds = button.bounds();
        assert!(button.visible(), "offer button {id} is clipped");
        assert!(bounds.top() >= viewport.top() - px(1.));
        assert!(bounds.bottom() <= viewport.bottom() + px(1.));
        assert!(bounds.left() >= viewport.left() - px(1.));
        assert!(bounds.right() <= viewport.right() + px(1.));
        assert!(viewport.bottom() <= w.viewport_size().height + px(1.));
    })
    .unwrap();
}

#[gpui_kit::test]
async fn offer_discovery_is_readonly_and_presents_only_first_sorted_target(
    cx: &mut TestAppContext,
) {
    for globals in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        for slug in ["z-source", "a-source"] {
            for name in ["z-host", "a-host"] {
                store.save_host(Some(slug), None, server(name)).unwrap();
            }
        }
        if globals {
            for name in ["z-server", "a-server"] {
                store.save_host(None, None, server(name)).unwrap();
            }
        }
        let desired = host("new destination");
        let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
        let before = stored_bytes(&store);
        offer(handle, &manager, cx).await;
        manager.read_with(cx, |m, cx| {
            let offer = m.offer.as_ref().unwrap();
            let target = offer.target.as_ref().unwrap();
            assert_eq!(target.project.as_deref(), (!globals).then_some("a-source"));
            assert_eq!(
                target.host.name,
                if globals { "a-server" } else { "a-host" }
            );
            assert_eq!(offer.alternatives, if globals { 5 } else { 3 });
            assert!(
                offer
                    .label
                    .contains(if globals { "Global server" } else { "a-source" })
            );
            assert!(target.host.auth != desired.auth);
            assert_eq!(target.host.keep_alive_interval, Some(0));
            assert!(m.form.as_ref().unwrap().draft(cx).build(false).unwrap() == desired);
        });
        let operation = manager.read_with(cx, |m, _| m.operation);
        // The offer placeholder focus has no default Enter/Space approval.
        for key in [
            "enter", "space", "ctrl-s", "cmd-s", "e", "d", "t", "l", "f5",
        ] {
            press(handle, key, cx);
            assert!(manager.read_with(cx, |m, _| m.operation == operation && m.cancel.is_none()));
        }
        cx.update_window(handle, |_, w, _| {
            assert!(w.try_find("host-save").is_none());
            assert!(w.try_find("host-choose-link").is_none());
            assert_eq!(w.find(BACK).label(), Some("Back to form"));
            assert_eq!(w.find(OWN).label(), Some("Save own connection (n)"));
            assert_eq!(w.find(USE).label(), Some("Use server link (y)"));
        })
        .unwrap();
        assert!(stored_bytes(&store) == before);
        assert!(store.project("dest").unwrap().hosts.is_empty());
        press(handle, "escape", cx);
        frame(handle, cx);
        assert!(manager.read_with(cx, |m, _| m.form.is_some() && m.offer.is_none()));
        assert!(stored_bytes(&store) == before);
    }
    // Plain text children have no text snapshot in GPUI Kit 0.7. Keep this disclosure
    // assertion separate from native focus/layout/activation evidence.
    let renderer = include_str!("../offer.rs");
    assert!(renderer.contains(
        "Linking uses that server's authentication and keep-alive settings, not this form's."
    ));
    assert!(renderer.contains("Already committed changes are not undone"));
}

#[gpui_kit::test]
async fn offer_n_declines_without_changing_original_authentication_metadata(
    cx: &mut TestAppContext,
) {
    for kind in ["password", "keyfile", "agent"] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        store.save_host(None, None, server("shared")).unwrap();
        let mut desired = host("own");
        desired.auth = match kind {
            "password" => desired.auth,
            "keyfile" => Auth {
                kind: kind.into(),
                key_file: " ~/.ssh/日本 key ".into(),
                passphrase: " 日本🦀 passphrase ".into(),
                ..Default::default()
            },
            _ => Auth {
                kind: kind.into(),
                ..Default::default()
            },
        };
        let global = fs::read(dir.path().join("config.toml")).unwrap();
        let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
        offer(handle, &manager, cx).await;
        assert!(store.project("dest").unwrap().hosts.is_empty());
        press(handle, "n", cx);
        idle(handle, &manager, cx).await;
        assert!(manager.read_with(cx, |m, _| m.form.is_none() && m.offer.is_none()));
        assert!(store.project("dest").unwrap().hosts == vec![desired]);
        assert!(fs::read(dir.path().join("config.toml")).unwrap() == global);
    }
}

#[gpui_kit::test]
async fn offer_native_enter_space_activate_the_focused_button_label_only(cx: &mut TestAppContext) {
    for key in ["enter", "space"] {
        for button in [BACK, OWN, USE] {
            let dir = tempfile::tempdir().unwrap();
            let store = Store::new(dir.path().into());
            store.save_host(None, None, server("shared")).unwrap();
            let desired = host("destination");
            let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
            offer(handle, &manager, cx).await;
            let before = stored_bytes(&store);
            tab_to(handle, button, cx);
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(w.find(button).focused(), Some(true));
                w.within("host-link-offer").press(key, cx);
            })
            .unwrap();
            idle(handle, &manager, cx).await;
            assert!(manager.read_with(cx, |m, _| m.offer.is_none()));
            match button {
                BACK => {
                    assert!(manager.read_with(cx, |m, _| m.form.is_some()));
                    assert!(stored_bytes(&store) == before);
                }
                OWN => assert!(store.project("dest").unwrap().hosts == vec![desired]),
                _ => {
                    assert!(store.project("dest").unwrap().hosts == vec![link(&desired, "shared")])
                }
            }
        }
    }
}

#[gpui_kit::test]
async fn offer_y_global_link_strips_connection_but_keeps_name_root_and_mappings(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let shared = server("shared");
    store.save_host(None, None, shared.clone()).unwrap();
    let original = host("before");
    store
        .save_host(Some("dest"), None, original.clone())
        .unwrap();
    let desired = Host {
        name: "after 日本".into(),
        ..original.clone()
    };
    let global = fs::read(dir.path().join("config.toml")).unwrap();
    let (handle, manager) =
        fixture(&store, Some("dest"), desired.clone(), Some(original), cx).await;
    offer(handle, &manager, cx).await;
    press(handle, "y", cx);
    idle(handle, &manager, cx).await;
    assert!(store.project("dest").unwrap().hosts == vec![link(&desired, "shared")]);
    assert!(fs::read(dir.path().join("config.toml")).unwrap() == global);
    let runtime = store.runtime(Some("dest")).unwrap().hosts.remove(0);
    assert!(runtime.auth == shared.auth);
    assert_eq!(runtime.keep_alive_interval, Some(0));
    assert_eq!(runtime.root_path, desired.root_path);
    assert_eq!(runtime.mappings, desired.mappings);
    assert!(
        manager.read_with(cx, |m, _| m.cursor.as_deref() == Some("after 日本")
            && m.form.is_none())
    );
}

#[gpui_kit::test]
async fn offer_y_other_project_promotes_and_links_both_project_scopes_once(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store
        .save_host(
            None,
            None,
            Host {
                hostname: "elsewhere".into(),
                ..server("prod")
            },
        )
        .unwrap();
    let source = Host {
        root_path: "/source/root".into(),
        mappings: vec![Mapping {
            local: "source".into(),
            remote: "source-app".into(),
        }],
        ..server("prod")
    };
    store
        .save_host(Some("source"), None, source.clone())
        .unwrap();
    let desired = host("destination");
    let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
    let before = stored_bytes(&store);
    offer(handle, &manager, cx).await;
    assert!(stored_bytes(&store) == before);
    press(handle, "y", cx);
    idle(handle, &manager, cx).await;
    let globals = store.global().unwrap().hosts;
    assert_eq!(globals.len(), 2);
    let promoted = globals
        .iter()
        .find(|host| host.name == "source-prod")
        .unwrap();
    assert!(promoted.auth == source.auth);
    assert!(promoted.mappings.is_empty());
    assert_eq!(promoted.keep_alive_interval, Some(0));
    assert!(store.project("source").unwrap().hosts == vec![link(&source, "source-prod")]);
    assert!(store.project("dest").unwrap().hosts == vec![link(&desired, "source-prod")]);
    let operation = manager.read_with(cx, |m, _| m.operation);
    for key in ["y", "n", "enter"] {
        press(handle, key, cx);
    }
    assert_eq!(manager.read_with(cx, |m, _| m.operation), operation);
    assert_eq!(store.global().unwrap().hosts.len(), 2);
}

#[gpui_kit::test]
async fn offer_bypasses_global_linked_and_no_match_saves_without_second_approval(
    cx: &mut TestAppContext,
) {
    for mode in ["global", "linked", "no-match"] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        store.save_host(None, None, server("shared")).unwrap();
        let desired = match mode {
            "linked" => link(&host("destination"), "shared"),
            "no-match" => Host {
                hostname: "no-match.example".into(),
                ..host("destination")
            },
            _ => host("destination"),
        };
        let slug = (mode != "global").then_some("dest");
        let (handle, manager) = fixture(&store, slug, desired.clone(), None, cx).await;
        let operation = manager.read_with(cx, |m, _| m.operation);
        tab_to(handle, "host-save", cx);
        press(handle, "enter", cx);
        idle(handle, &manager, cx).await;
        manager.read_with(cx, |m, _| {
            assert!(m.form.is_none() && m.offer.is_none());
            assert_eq!(
                m.operation,
                operation + if mode == "no-match" { 3 } else { 2 }
            );
        });
        let hosts = match slug {
            Some(slug) => store.project(slug).unwrap().hosts,
            None => store.global().unwrap().hosts,
        };
        assert_eq!(hosts.iter().filter(|h| h.name == desired.name).count(), 1);
        assert!(hosts.into_iter().find(|h| h.name == desired.name).unwrap() == desired);
    }
}

#[gpui_kit::test]
async fn offer_matches_effective_defaults_and_exact_user_protocol_with_ascii_hostname_case(
    cx: &mut TestAppContext,
) {
    for (
        draft_protocol,
        target_protocol,
        port,
        target_port,
        target_user,
        target_hostname,
        matches,
    ) in [
        ("", "sftp", 0, 22, "deploy", "DEPLOY.EXAMPLE", true),
        ("ftp", "ftp", 0, 21, "deploy", "DEPLOY.EXAMPLE", true),
        ("ftps", "ftps", 0, 21, "deploy", "deploy.example", true),
        ("sftp", "sftp", 2222, 0, "", "DEPLOY.EXAMPLE", true),
        ("ftp", "ftps", 0, 21, "deploy", "deploy.example", false),
        ("sftp", "sftp", 0, 23, "deploy", "deploy.example", false),
        ("sftp", "sftp", 0, 22, "Deploy", "deploy.example", false),
        ("sftp", "sftp", 0, 22, "deploy", "deploy.example.", false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        fs::write(
            dir.path().join("config.toml"),
            "[defaults]\nport = 2222\nuser = 'deploy'\n",
        )
        .unwrap();
        store
            .save_host(
                None,
                None,
                Host {
                    port: target_port,
                    user: target_user.into(),
                    protocol: target_protocol.into(),
                    hostname: target_hostname.into(),
                    ..server("shared")
                },
            )
            .unwrap();
        let desired = Host {
            port,
            protocol: draft_protocol.into(),
            auth: Auth {
                kind: "password".into(),
                password: " own 日本 secret ".into(),
                ..Default::default()
            },
            ..host("destination")
        };
        let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
        press(handle, "ctrl-s", cx);
        idle(handle, &manager, cx).await;
        if matches {
            assert!(manager.read_with(cx, |m, _| {
                m.offer.as_ref().is_some_and(|o| o.phase == Phase::Ready)
            }));
            assert!(store.project("dest").unwrap().hosts.is_empty());
            press(handle, "escape", cx);
        } else {
            assert!(manager.read_with(cx, |m, _| m.offer.is_none() && m.form.is_none()));
            assert!(store.project("dest").unwrap().hosts == vec![desired]);
        }
    }
    for protocol in ["sftp", "ftp"] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        fs::create_dir(dir.path().join("projects")).unwrap();
        for slug in ["dest", "source"] {
            fs::write(
                dir.path().join(format!("projects/{slug}.toml")),
                "[defaults]\nport = 2222\nuser = 'deploy'\n",
            )
            .unwrap();
        }
        store
            .save_host(
                Some("source"),
                None,
                Host {
                    port: 0,
                    user: String::new(),
                    protocol: protocol.into(),
                    ..server("shared")
                },
            )
            .unwrap();
        let desired = Host {
            user: String::new(),
            protocol: protocol.into(),
            ..host("destination")
        };
        let (handle, manager) = fixture(&store, Some("dest"), desired, None, cx).await;
        offer(handle, &manager, cx).await;
        manager.read_with(cx, |m, _| {
            let target = m.offer.as_ref().unwrap().target.as_ref().unwrap();
            assert_eq!(target.project.as_deref(), Some("source"));
            assert_eq!(target.host.port, 2222);
            assert_eq!(target.host.user, "deploy");
        });
        assert!(store.project("dest").unwrap().hosts.is_empty());
        press(handle, "escape", cx);
    }
}

#[gpui_kit::test]
async fn offer_escape_checking_cancels_and_versions_out_the_old_response(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store.save_host(None, None, server("shared")).unwrap();
    let desired = host("destination");
    let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
    let before = stored_bytes(&store);
    let (cancel, old_operation) = cx
        .update_window(handle, |_, w, cx| {
            w.press("ctrl-s", cx);
            let m = manager.read(cx);
            assert!(m.offer.as_ref().unwrap().phase == Phase::Checking);
            let cancel = m.cancel.as_ref().unwrap().clone();
            let operation = m.operation;
            for key in ["enter", "space", "y", "n", "ctrl-s"] {
                w.press(key, cx);
                let m = manager.read(cx);
                assert_eq!(m.operation, operation);
                assert!(m.offer.as_ref().unwrap().phase == Phase::Checking);
            }
            w.press("escape", cx);
            let m = manager.read(cx);
            assert!(m.offer.is_none() && m.cancel.is_none() && !m.writing);
            assert!(m.operation > operation);
            (cancel, operation)
        })
        .unwrap();
    assert!(cancel.is_cancelled());
    // A new real request also gives the detached old completion a chance to arrive.
    frame(handle, cx);
    offer(handle, &manager, cx).await;
    let serial = manager.read_with(cx, |m, _| m.offer.as_ref().unwrap().serial);
    assert!(serial > old_operation);
    cx.executor().timer(Duration::from_millis(100)).await;
    frame(handle, cx);
    manager.read_with(cx, |m, cx| {
        assert_eq!(m.offer.as_ref().unwrap().serial, serial);
        assert!(m.offer.as_ref().unwrap().phase == Phase::Ready);
        assert!(m.form.as_ref().unwrap().draft(cx).build(false).unwrap() == desired);
    });
    assert!(stored_bytes(&store) == before);
    press(handle, "escape", cx);
}

#[gpui_kit::test]
async fn offer_escape_during_mutating_save_does_not_cancel_or_claim_rollback(
    cx: &mut TestAppContext,
) {
    for approval in ["y", "n"] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        store.save_host(None, None, server("shared")).unwrap();
        let desired = host("destination");
        let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
        offer(handle, &manager, cx).await;
        cx.update_window(handle, |_, w, cx| {
            w.press(approval, cx);
            let m = manager.read(cx);
            let operation = m.operation;
            let cancel = m.cancel.as_ref().unwrap().clone();
            assert!(m.offer.as_ref().unwrap().phase == Phase::Committing);
            w.render_frame(cx);
            w.click(BACK, cx);
            assert!(manager.read(cx).offer.is_some());
            w.press("escape", cx);
            raw_key(w, cx, "ctrl-s");
            raw_key(w, cx, "y");
            raw_key(w, cx, "n");
            let m = manager.read(cx);
            assert_eq!(m.operation, operation);
            assert!(m.offer.as_ref().unwrap().phase == Phase::Committing);
            assert!(m.writing && m.cancel.is_some());
            assert!(!cancel.is_cancelled());
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        let expected = if approval == "y" {
            link(&desired, "shared")
        } else {
            desired
        };
        assert!(store.project("dest").unwrap().hosts == vec![expected]);
    }
}

#[gpui_kit::test]
async fn offer_no_frame_native_parent_capture_and_callback_guards_block_old_scenes(
    cx: &mut TestAppContext,
) {
    for old_button in [
        ElementId::from("host-save"),
        ElementId::from("host-cancel"),
        ElementId::from("host-test-draft"),
        ElementId::from("direct"),
        ElementId::from("host-choose-link"),
        ElementId::from(("link", 0usize)),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        store.save_host(None, None, server("shared")).unwrap();
        let desired = host("destination");
        let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
        tab_to(handle, old_button.clone(), cx);
        cx.update_window(handle, |_, w, cx| {
            let position = w.find(old_button.clone()).bounds().center();
            // Leave a native press armed in the old frame across the scene change.
            raw_down(w, cx, position);
            raw_key(w, cx, "ctrl-s");
            let operation = manager.read(cx).operation;
            assert!(manager.read(cx).offer.is_some());
            raw_up(w, cx, position);
            raw_down(w, cx, position);
            raw_up(w, cx, position);
            for key in ["ctrl-s", "cmd-s", "enter", "space", "y", "n"] {
                raw_key(w, cx, key);
            }
            let m = manager.read(cx);
            assert_eq!(m.operation, operation);
            assert!(m.offer.as_ref().unwrap().phase == Phase::Checking);
            assert!(m.links.is_none() && m.tools.is_none() && m.form.is_some());
            assert!(m.form.as_ref().unwrap().server.is_empty());
        })
        .unwrap();
        ready(handle, &manager, cx).await;
        tab_to(handle, BACK, cx);
        cx.update_window(handle, |_, w, cx| {
            let positions = [w.find(OWN).bounds().center(), w.find(USE).bounds().center()];
            raw_key(w, cx, "enter");
            assert!(manager.read(cx).offer.is_none());
            let operation = manager.read(cx).operation;
            for position in positions {
                raw_down(w, cx, position);
                raw_up(w, cx, position);
            }
            for key in ["y", "n", "space"] {
                raw_key(w, cx, key);
            }
            let m = manager.read(cx);
            assert_eq!(m.operation, operation);
            assert!(m.form.is_some() && m.offer.is_none() && m.cancel.is_none());
            assert!(m.form.as_ref().unwrap().draft(cx).build(false).unwrap() == desired);
        })
        .unwrap();
        frame(handle, cx);
        assert!(store.project("dest").unwrap().hosts.is_empty());
    }
}

#[gpui_kit::test]
async fn offer_stale_target_defaults_and_destination_conflicts_never_retry_or_promote(
    cx: &mut TestAppContext,
) {
    for failure in [
        "global-host",
        "global-defaults",
        "source-host",
        "source-defaults",
        "dest-version",
        "dest-name",
        "dest-defaults",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        let from_global = failure.starts_with("global");
        store
            .save_host(
                if from_global { None } else { Some("source") },
                None,
                server("shared"),
            )
            .unwrap();
        let original = host("before");
        store
            .save_host(Some("dest"), None, original.clone())
            .unwrap();
        let desired = Host {
            name: "after".into(),
            ..original.clone()
        };
        let (handle, manager) = fixture(
            &store,
            Some("dest"),
            desired.clone(),
            Some(original.clone()),
            cx,
        )
        .await;
        offer(handle, &manager, cx).await;
        match failure {
            "global-host" | "source-host" => {
                let scope = if from_global { None } else { Some("source") };
                let expected = server("shared");
                store
                    .save_host(
                        scope,
                        Some(&expected),
                        Host {
                            auth: Auth {
                                kind: "password".into(),
                                password: "changed server secret".into(),
                                ..Default::default()
                            },
                            ..expected.clone()
                        },
                    )
                    .unwrap();
            }
            "global-defaults" | "source-defaults" | "dest-defaults" => {
                let path = dir.path().join(match failure {
                    "global-defaults" => "config.toml",
                    "source-defaults" => "projects/source.toml",
                    _ => "projects/dest.toml",
                });
                let bytes = fs::read_to_string(&path).unwrap();
                assert_eq!(bytes.matches("[defaults]").count(), 1);
                let changed = bytes.replacen(
                    "[defaults]",
                    "[defaults]\nport = 2222\nuser = 'changed-account'",
                    1,
                );
                fs::write(path, changed).unwrap();
            }
            "dest-version" => {
                store
                    .save_host(
                        Some("dest"),
                        Some(&original),
                        Host {
                            root_path: "/concurrently-edited".into(),
                            ..original.clone()
                        },
                    )
                    .unwrap();
            }
            _ => {
                store.save_host(Some("dest"), None, host("after")).unwrap();
            }
        }
        let before = stored_bytes(&store);
        let state = cx
            .update_window(handle, |_, w, cx| inputs(&manager, w, cx))
            .unwrap();
        press(handle, "y", cx);
        idle(handle, &manager, cx).await;
        let operation = manager.read_with(cx, |m, _| {
            assert!(m.offer.as_ref().unwrap().phase == Phase::Failed);
            assert!(m.form.is_some() && !m.status.is_empty());
            assert!(!m.status.contains("changed server secret"));
            m.operation
        });
        for key in ["enter", "y", "ctrl-s", "cmd-s"] {
            press(handle, key, cx);
        }
        cx.executor().timer(Duration::from_millis(50)).await;
        frame(handle, cx);
        cx.update_window(handle, |_, w, cx| {
            assert!(inputs(&manager, w, cx) == state);
            assert_eq!(manager.read(cx).operation, operation);
        })
        .unwrap();
        assert!(stored_bytes(&store) == before);
        if !from_global {
            assert!(store.global().unwrap().hosts.is_empty());
        }
        if failure == "source-host" {
            // A failed link is not authorization to fall back to an own save.
            press(handle, "n", cx);
            idle(handle, &manager, cx).await;
            assert!(store.project("dest").unwrap().hosts == vec![desired]);
            assert!(store.global().unwrap().hosts.is_empty());
        } else {
            press(handle, "escape", cx);
            assert!(manager.read_with(cx, |m, cx| {
                m.form.as_ref().unwrap().draft(cx).build(false).unwrap() == desired
            }));
            assert!(stored_bytes(&store) == before);
        }
    }
}

#[gpui_kit::test]
async fn offer_rejected_approval_and_back_preserve_password_hidden_marks_and_input_entities(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store.save_host(None, None, server("shared")).unwrap();
    let desired = host("destination");
    let (handle, manager) = fixture(&store, Some("dest"), desired, None, cx).await;
    tab_to(handle, "password", cx);
    cx.update_window(handle, |_, w, cx| {
        w.press("ctrl-a", cx);
        w.input(" 日本🦀e\u{301} replacement ", cx);
    })
    .unwrap();
    offer(handle, &manager, cx).await;
    let operation = manager.read_with(cx, |m, _| m.operation);
    let state = cx
        .update_window(handle, |_, w, cx| {
            let hidden = manager.read(cx).form.as_ref().unwrap().fields[PASSPHRASE].clone();
            hidden.update(cx, |s, cx| {
                s.set_value("A🦀Z", w, cx);
                s.replace_and_mark_text_in_range(Some(1..3), "に🦀", Some(1..3), w, cx);
            });
            assert!(w.try_find("passphrase").is_none());
            inputs(&manager, w, cx)
        })
        .unwrap();
    for key in ["y", "n"] {
        press(handle, key, cx);
        cx.update_window(handle, |_, w, cx| {
            assert!(inputs(&manager, w, cx) == state);
            let m = manager.read(cx);
            assert_eq!(m.operation, operation);
            assert!(m.cancel.is_none() && m.offer.is_some());
            assert!(m.status.contains("composition"));
            assert!(!m.status.contains("replacement"));
        })
        .unwrap();
    }
    press(handle, "escape", cx);
    frame(handle, cx);
    frame(handle, cx);
    cx.update_window(handle, |_, w, cx| {
        assert!(inputs(&manager, w, cx) == state);
        let m = manager.read(cx);
        let form = m.form.as_ref().unwrap();
        assert!(form.fields[PASSWORD].focus_handle(cx).is_focused(w));
        assert!(form.fields[PASSWORD].read(cx).value() == " 日本🦀e\u{301} replacement ");
        assert!(form.fields[PASSWORD].read(cx).presentation().is_masked());
        assert!(
            !w.find("password")
                .value()
                .unwrap_or_default()
                .contains("replacement")
        );
        assert_eq!(form.mappings.len(), 1);
        assert_eq!(form.fields[ROOT].read(cx).value(), "/srv/日本 app ");
    })
    .unwrap();
    assert!(store.project("dest").unwrap().hosts.is_empty());
}

#[gpui_kit::test]
async fn offer_short_viewport_reveals_native_buttons_on_focus_resize_without_wheel_snap(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store
        .save_host(Some("source"), None, server("shared"))
        .unwrap();
    let desired = Host {
        mappings: (0..8)
            .map(|i| Mapping {
                local: format!("src/{i}"),
                remote: format!("deploy/{i}"),
            })
            .collect(),
        ..host("destination")
    };
    let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
    cx.simulate_window_resize(handle, size(px(860.), px(330.)));
    frame(handle, cx);
    tab_to(handle, ("map-remote", 7usize), cx);
    let (field, form_scroll, old_offset) = manager.read_with(cx, |m, _| {
        let form = m.form.as_ref().unwrap();
        (
            form.mappings[7].1.clone(),
            form.reveal.scroll_handle().clone(),
            form.reveal.scroll_handle().offset(),
        )
    });
    let state = cx
        .update_window(handle, |_, w, cx| inputs(&manager, w, cx))
        .unwrap();
    offer(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.viewport_size(), size(px(860.), px(330.)));
        assert!(w.find("host-link-offer").bounds().size.height <= px(330.));
    })
    .unwrap();
    for button in [BACK, OWN, USE] {
        tab_to(handle, button, cx);
        visible_in_offer(handle, button, cx);
    }
    cx.simulate_window_resize(handle, size(px(620.), px(180.)));
    frame(handle, cx);
    frame(handle, cx);
    visible_in_offer(handle, USE, cx);
    let scroll = manager.read_with(cx, |m, _| {
        m.offer.as_ref().unwrap().reveal.scroll_handle().clone()
    });
    let before = scroll.offset();
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "host-link-offer",
            ScrollDelta::Pixels(point(px(0.), px(180.))),
            cx,
        );
    })
    .unwrap();
    frame(handle, cx);
    let after = scroll.offset();
    assert!(after.y > before.y);
    for _ in 0..4 {
        frame(handle, cx);
    }
    assert_eq!(
        scroll.offset(),
        after,
        "unchanged offer focus snapped back after wheel scroll"
    );
    press(handle, "shift-tab", cx);
    visible_in_offer(handle, OWN, cx);
    cx.simulate_window_resize(handle, size(px(860.), px(330.)));
    frame(handle, cx);
    frame(handle, cx);
    tab_to(handle, BACK, cx);
    press(handle, "space", cx);
    frame(handle, cx);
    frame(handle, cx);
    cx.update_window(handle, |_, w, cx| {
        assert!(inputs(&manager, w, cx) == state);
        assert!(field.focus_handle(cx).is_focused(w));
        assert!(w.find(("map-remote", 7usize)).visible());
        assert_eq!(form_scroll.offset(), old_offset);
        assert!(
            manager
                .read(cx)
                .form
                .as_ref()
                .unwrap()
                .draft(cx)
                .build(false)
                .unwrap()
                == desired
        );
    })
    .unwrap();
    assert!(store.project("dest").unwrap().hosts.is_empty());
    assert!(store.global().unwrap().hosts.is_empty());
}

#[gpui_kit::test]
async fn offer_lookup_error_keeps_draft_and_requires_explicit_new_own_approval(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store.save_host(None, None, server("shared")).unwrap();
    let desired = host("destination");
    let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
    fs::create_dir(dir.path().join("projects")).unwrap();
    fs::write(dir.path().join("projects/unreadable.toml"), "[unterminated").unwrap();
    let before = stored_bytes(&store);
    let state = cx
        .update_window(handle, |_, w, cx| inputs(&manager, w, cx))
        .unwrap();
    press(handle, "ctrl-s", cx);
    idle(handle, &manager, cx).await;
    let operation = manager.read_with(cx, |m, _| {
        let offer = m.offer.as_ref().unwrap();
        assert!(offer.phase == Phase::Failed && offer.target.is_none());
        assert!(m.form.is_some() && !m.status.is_empty());
        m.operation
    });
    cx.update_window(handle, |_, w, _| assert!(w.try_find(USE).is_none()))
        .unwrap();
    for key in ["enter", "space", "y", "ctrl-s", "cmd-s"] {
        press(handle, key, cx);
    }
    cx.update_window(handle, |_, w, cx| {
        assert!(inputs(&manager, w, cx) == state);
        assert_eq!(manager.read(cx).operation, operation);
    })
    .unwrap();
    assert!(stored_bytes(&store) == before);
    assert!(store.project("dest").unwrap().hosts.is_empty());
    tab_to(handle, OWN, cx);
    press(handle, "space", cx);
    idle(handle, &manager, cx).await;
    assert!(store.project("dest").unwrap().hosts == vec![desired]);
    assert_eq!(
        fs::read(dir.path().join("projects/unreadable.toml")).unwrap(),
        b"[unterminated"
    );
    assert_eq!(store.global().unwrap().hosts.len(), 1);
}

struct ReadOnlyProjects(PathBuf);
impl Drop for ReadOnlyProjects {
    fn drop(&mut self) {
        fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

#[gpui_kit::test]
async fn offer_partial_promotion_io_error_retains_committed_server_and_draft_without_auto_retry(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let source = server("shared");
    store
        .save_host(Some("source"), None, source.clone())
        .unwrap();
    let desired = host("destination");
    let (handle, manager) = fixture(&store, Some("dest"), desired.clone(), None, cx).await;
    offer(handle, &manager, cx).await;
    let state = cx
        .update_window(handle, |_, w, cx| inputs(&manager, w, cx))
        .unwrap();
    let projects = dir.path().join("projects");
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o500)).unwrap();
    let permissions = ReadOnlyProjects(projects.clone());
    // An unreadable destination would fail validation before promotion. Directory
    // permissions instead permit all reads and fail only the subsequent atomic writes.
    match fs::File::create(projects.join(".permission-probe")) {
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied),
        Ok(file) => {
            drop(file);
            fs::remove_file(projects.join(".permission-probe")).unwrap();
            // CAP_DAC_OVERRIDE/root cannot exercise a real permission-denied fixture.
            drop(permissions);
            return;
        }
    }
    press(handle, "y", cx);
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        assert!(inputs(&manager, w, cx) == state);
        let m = manager.read(cx);
        assert!(m.offer.is_none() && m.form.is_some());
        assert_eq!(m.form.as_ref().unwrap().server, "shared");
        assert!(m.status.contains("committed") && m.status.contains("not saved"));
        assert!(m.status.contains("source project keeps its own copy"));
        assert!(!m.status.contains("server-only-passphrase"));
    })
    .unwrap();
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    assert!(store.project("source").unwrap().hosts == vec![source]);
    assert!(store.project("dest").unwrap().hosts.is_empty());
    let operation = manager.read_with(cx, |m, _| m.operation);
    drop(permissions);
    cx.executor().timer(Duration::from_millis(100)).await;
    for _ in 0..4 {
        frame(handle, cx);
    }
    assert_eq!(manager.read_with(cx, |m, _| m.operation), operation);
    assert!(store.project("dest").unwrap().hosts.is_empty());
    // A later explicit Save uses the already committed link, not promotion again.
    tab_to(handle, "host-save", cx);
    press(handle, "enter", cx);
    idle(handle, &manager, cx).await;
    assert!(store.project("dest").unwrap().hosts == vec![link(&desired, "shared")]);
    assert_eq!(store.global().unwrap().hosts.len(), 1);
}
