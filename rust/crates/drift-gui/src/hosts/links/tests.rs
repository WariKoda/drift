use super::super::{HOSTNAME, HostManager, NAME, ROOT};
use super::*;
use drift_core::{config::Auth, pathmap::Mapping};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, point, size};
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

async fn idle(handle: AnyWindowHandle, manager: &Entity<HostManager>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        let manager = manager.read(cx);
        manager.cancel.is_none()
            && manager
                .links
                .as_ref()
                .is_none_or(|picker| picker.read(cx).cancel.is_none())
    })
    .await;
}
fn open(store: &Store, cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<HostManager>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1400.), px(1200.)),
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
                        Some("dest".into()),
                        w,
                        cx,
                    )
                })
            },
        )
        .unwrap()
    })
}

#[gpui_kit::test]
async fn keyboard_safety_promotion_tabs_activate_back_and_reload_without_promoting(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let host = Host {
        name: "stage".into(),
        hostname: "example.test".into(),
        root_path: "/srv".into(),
        ..Default::default()
    };
    store.save_host(Some("source"), None, host.clone()).unwrap();
    let source = fs::read(config.path().join("projects/source.toml")).unwrap();
    let (handle, manager) = open(&store, cx);
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.press("down", cx);
        w.press("l", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    let picker = manager.read_with(cx, |m, _| m.links.as_ref().unwrap().clone());
    for key in ["enter", "space"] {
        cx.update_window(handle, |_, w, cx| {
            w.press("down", cx);
            w.press("enter", cx);
            assert!(picker.read(cx).focus.is_focused(w));
            super::super::tests::tab_to(w, "host-link-close", cx);
            w.press(key, cx);
            assert!(picker.read(cx).selected.is_none());
            assert!(picker.read(cx).list_focus.is_focused(w));
            assert!(picker.read(cx).cancel.is_none());
            assert!(manager.read(cx).links.is_some());
            w.press("enter", cx);
            super::super::tests::tab_to(w, "host-link-reload", cx);
            let operation = picker.read(cx).operation;
            w.press(key, cx);
            assert_eq!(picker.read(cx).operation, operation + 1);
            assert!(!picker.read(cx).writing);
            assert!(picker.read(cx).cancel.is_some());
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        cx.update_window(handle, |_, w, cx| {
            assert!(picker.read(cx).selected.is_none());
            assert!(picker.read(cx).list_focus.is_focused(w));
            assert!(manager.read(cx).links.is_some());
        })
        .unwrap();
        assert!(store.global().unwrap().hosts.is_empty());
        assert_eq!(
            fs::read(config.path().join("projects/source.toml")).unwrap(),
            source
        );
        assert!(store.project("source").unwrap().hosts == vec![host.clone()]);
        assert!(!config.path().join("projects/dest.toml").exists());
    }
    // Y remains an explicit confirmation even with Back focused.
    cx.update_window(handle, |_, w, cx| {
        w.press("enter", cx);
        super::super::tests::tab_to(w, "host-link-close", cx);
        w.press("y", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    assert!(manager.read_with(cx, |m, _| m.links.is_none() && m.form.is_some()));
    assert_eq!(store.project("source").unwrap().hosts[0].server, "stage");
    assert_eq!(store.global().unwrap().hosts.len(), 1);
}

#[gpui_kit::test]
async fn promotion_filter_confirmation_and_save_preserve_form_scope(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let project = store
        .register("Source Shop", project_root.path().into())
        .unwrap();
    let host = Host {
        name: "stage".into(),
        hostname: "stage.example".into(),
        root_path: "/source".into(),
        auth: Auth {
            kind: "keyfile".into(),
            key_file: "~/.ssh/key".into(),
            ..Auth::default()
        },
        keep_alive_interval: Some(0),
        mappings: vec![Mapping {
            local: "src".into(),
            remote: "app".into(),
        }],
        ..Host::default()
    };
    store
        .save_host(Some(&project.slug), None, host.clone())
        .unwrap();
    let source_path = config
        .path()
        .join(format!("projects/{}.toml", project.slug));
    let source = fs::read_to_string(&source_path).unwrap();
    assert!(source.contains("[defaults]"));
    fs::write(
        source_path,
        source.replace("[defaults]", "[defaults]\nport=2200\nuser='web'"),
    )
    .unwrap();
    store
        .save_host(
            None,
            None,
            Host {
                name: "stage".into(),
                hostname: "global.example".into(),
                root_path: "/global".into(),
                ..Host::default()
            },
        )
        .unwrap();
    let before = fs::read(config.path().join("config.toml")).unwrap();
    let (handle, manager) = open(&store, cx);
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.click("host-new", cx);
        for (field, value) in [
            ("host-name", "my-deployment"),
            ("hostname", "typed.example"),
            ("root-path", "/dest"),
        ] {
            w.click(field, cx);
            w.input(value, cx);
        }
        w.click("map-add", cx);
        w.click(("map-local", 0usize), cx);
        w.input("assets", cx);
        w.click(("map-remote", 0usize), cx);
        w.input("public", cx);
        w.click("host-choose-link", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    let picker = manager.read_with(cx, |m, _| m.links.as_ref().unwrap().clone());
    let focus = picker.read_with(cx, |p, _| p.previous_focus.clone().unwrap());
    cx.update_window(handle, |_, w, cx| {
        w.click("host-link-filter", cx);
        w.input("Source Shop", cx);
        w.press("down", cx);
        w.press("enter", cx);
    })
    .unwrap();
    assert_eq!(fs::read(config.path().join("config.toml")).unwrap(), before);
    assert!(store.project(&project.slug).unwrap().hosts[0] == host);
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        assert!(picker.read(cx).list_focus.is_focused(w));
        assert_eq!(
            picker.read(cx).cursor.as_ref().unwrap().0.as_deref(),
            Some(project.slug.as_str())
        );
        w.press("enter", cx);
        w.press("enter", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        let m = manager.read(cx);
        let form = m.form.as_ref().unwrap();
        assert!(m.links.is_none());
        assert!(focus.is_focused(w));
        assert_eq!(form.fields[NAME].read(cx).value(), "my-deployment");
        assert_eq!(form.fields[HOSTNAME].read(cx).value(), "typed.example");
        assert_eq!(form.fields[ROOT].read(cx).value(), "/dest");
        assert_eq!(form.server, format!("{}-stage", project.slug));
        assert!(!config.path().join("projects/dest.toml").exists());
        w.click("host-save", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    let dest = store.project("dest").unwrap();
    let link = &dest.hosts[0];
    assert_eq!(link.name, "my-deployment");
    assert_eq!(link.root_path, "/dest");
    assert!(link.hostname.is_empty() && link.user.is_empty() && link.auth == Auth::default());
    assert_eq!(
        link.mappings,
        vec![Mapping {
            local: "assets".into(),
            remote: "public".into()
        }]
    );
    let runtime = store.runtime(Some("dest")).unwrap();
    assert_eq!(runtime.hosts[0].port, 2200);
    assert_eq!(runtime.hosts[0].user, "web");
    assert_eq!(
        store.project(&project.slug).unwrap().hosts[0].mappings,
        host.mappings
    );
    assert!(fs::read_dir(project_root.path()).unwrap().next().is_none());
    // Add-link from the list makes an unsaved form using the selected server root.
    cx.update_window(handle, |_, w, cx| w.click("host-add-link", cx))
        .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| w.click(("host-link-target", 1usize), cx))
        .unwrap();
    idle(handle, &manager, cx).await;
    manager.read_with(cx, |m, cx| {
        let form = m.form.as_ref().unwrap();
        assert_eq!(form.server, "stage");
        assert_eq!(form.fields[ROOT].read(cx).value(), "/global");
    });
    assert_eq!(store.project("dest").unwrap().hosts.len(), 1);
}

#[gpui_kit::test]
async fn link_conflicts_and_partial_writes_are_visible_without_losing_confirmation(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for mode in ["source", "global", "partial"] {
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        let host = Host {
            name: "stage".into(),
            hostname: "stage.example".into(),
            root_path: "/source".into(),
            ..Host::default()
        };
        store
            .save_host(
                if mode == "global" {
                    None
                } else {
                    Some("source")
                },
                None,
                host.clone(),
            )
            .unwrap();
        let (handle, manager) = open(&store, cx);
        idle(handle, &manager, cx).await;
        cx.update_window(handle, |_, w, cx| w.click("host-add-link", cx))
            .unwrap();
        idle(handle, &manager, cx).await;
        if mode != "global" {
            cx.update_window(handle, |_, w, cx| w.click(("host-link-target", 0usize), cx))
                .unwrap();
        }
        if mode == "partial" {
            fs::set_permissions(
                config.path().join("projects"),
                fs::Permissions::from_mode(0o500),
            )
            .unwrap();
        } else {
            let mut edited = host.clone();
            edited.hostname = "changed.example".into();
            store
                .save_host(
                    if mode == "global" {
                        None
                    } else {
                        Some("source")
                    },
                    Some(&host),
                    edited,
                )
                .unwrap();
        }
        cx.update_window(handle, |_, w, cx| {
            if mode == "global" {
                w.click(("host-link-target", 0usize), cx);
            } else {
                w.press("enter", cx);
            }
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        if mode == "partial" {
            fs::set_permissions(
                config.path().join("projects"),
                fs::Permissions::from_mode(0o700),
            )
            .unwrap();
            manager.read_with(cx, |m, _| {
                assert!(m.links.is_none());
                assert!(m.form.is_some());
                assert!(m.status.contains("keeps its own copy"), "{}", m.status);
            });
            assert!(store.project("source").unwrap().hosts[0] == host);
            assert_eq!(store.global().unwrap().hosts.len(), 1);
            assert!(!config.path().join("projects/dest.toml").exists());
        } else {
            let picker = manager.read_with(cx, |m, _| m.links.as_ref().unwrap().clone());
            picker.read_with(cx, |p, _| {
                assert!(p.selected.is_some());
                assert!(p.status.contains("changed"), "{}", p.status);
                assert!(!p.writing);
            });
            if mode == "source" {
                assert!(!config.path().join("config.toml").exists());
            }
            cx.update_window(handle, |_, w, cx| w.press("r", cx))
                .unwrap();
            idle(handle, &manager, cx).await;
            cx.update_window(handle, |_, w, cx| {
                w.click(("host-link-target", 0usize), cx);
                if mode == "source" {
                    w.press("enter", cx);
                }
            })
            .unwrap();
            idle(handle, &manager, cx).await;
            assert!(manager.read_with(cx, |m, _| m.links.is_none() && m.form.is_some()));
        }
    }
}

#[gpui_kit::test]
async fn keyboard_picker_cursor_filter_reload_and_promotion_preserve_identity(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let source = store.register("Source", root.path().into()).unwrap();
    for name in ["shared", "zzz"] {
        store
            .save_host(
                None,
                None,
                Host {
                    name: name.into(),
                    hostname: "global.example".into(),
                    root_path: "/global".into(),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    store
        .save_host(
            Some(&source.slug),
            None,
            Host {
                name: "stage".into(),
                hostname: "stage.example".into(),
                root_path: "/stage".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let before = fs::read(config.path().join("config.toml")).unwrap();
    let (handle, manager) = open(&store, cx);
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| w.click("host-add-link", cx))
        .unwrap();
    idle(handle, &manager, cx).await;
    let picker = manager.read_with(cx, |m, _| m.links.as_ref().unwrap().clone());
    cx.update_window(handle, |_, w, cx| {
        w.press("down", cx);
        for (key, name) in [
            ("home", "shared"),
            ("end", "stage"),
            ("k", "zzz"),
            ("j", "stage"),
            ("g", "shared"),
            ("shift-g", "stage"),
        ] {
            w.press(key, cx);
            assert_eq!(picker.read(cx).cursor.as_ref().unwrap().1, name);
        }
        w.press("/", cx);
        w.input("jkgery", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(picker.read(cx).query.read(cx).value().as_str(), "jkgery");
        assert!(picker.read(cx).selected.is_none());
        w.press("enter", cx);
        w.press("enter", cx);
        assert!(picker.read(cx).selected.is_none());
        assert!(picker.read(cx).cancel.is_none());
        w.press("ctrl-f", cx);
        w.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        w.input("Source", cx);
        w.press("down", cx);
        w.press("enter", cx);
        assert_eq!(
            picker
                .read(cx)
                .selected
                .as_ref()
                .unwrap()
                .project
                .as_deref(),
            Some(source.slug.as_str())
        );
        w.press("escape", cx);
        assert!(picker.read(cx).list_focus.is_focused(w));
        w.press("r", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    assert_eq!(fs::read(config.path().join("config.toml")).unwrap(), before);
    cx.update_window(handle, |_, w, cx| {
        assert!(picker.read(cx).list_focus.is_focused(w));
        assert_eq!(
            picker.read(cx).cursor.as_ref().unwrap().0.as_deref(),
            Some(source.slug.as_str())
        );
        w.press("enter", cx);
        w.press("y", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    assert!(manager.read_with(cx, |m, _| m.links.is_none() && m.form.is_some()));
    assert_eq!(
        store.project(&source.slug).unwrap().hosts[0].server,
        "stage"
    );
    assert!(!config.path().join("projects/dest.toml").exists());
}
