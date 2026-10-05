use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, point, size};
use std::{fs, time::Duration};

async fn idle(handle: AnyWindowHandle, manager: &Entity<HostManager>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        manager.read(cx).cancel.is_none()
    })
    .await;
}

pub(super) fn tab_to(window: &mut Window, target: &'static str, cx: &mut App) {
    for _ in 0..96 {
        window.press("tab", cx);
        if window.find(target).focused() == Some(true) {
            return;
        }
    }
    panic!("Tab traversal did not reach {target}");
}

fn keyboard_window(
    store: &Store,
    slug: Option<&str>,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<HostManager>) {
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
                        slug.map(str::to_owned),
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
async fn keyboard_safety_host_confirmation_tabs_activate_cancel_and_back(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let host = Host {
        name: "keep".into(),
        hostname: "example.test".into(),
        ..Default::default()
    };
    store.save_host(None, None, host.clone()).unwrap();
    let (handle, manager) = keyboard_window(&store, None, cx);
    let closed = std::rc::Rc::new(std::cell::Cell::new(0));
    let _subscription = cx.update(|cx| {
        let closed = closed.clone();
        cx.subscribe(&manager, move |_, event: &HostEvent, _| {
            if matches!(event, HostEvent::Close) {
                closed.set(closed.get() + 1);
            }
        })
    });
    idle(handle, &manager, cx).await;
    for key in ["enter", "space"] {
        cx.update_window(handle, |_, w, cx| {
            w.press("down", cx);
            w.press("d", cx);
            assert!(manager.read(cx).focus.is_focused(w));
            for _ in 0..32 {
                w.press("tab", cx);
                assert!(!manager.read(cx).focus.is_focused(w));
            }
            tab_to(w, "host-cancel-delete", cx);
            w.press(key, cx);
            assert!(manager.read(cx).delete.is_none());
            assert!(manager.read(cx).list_focus.is_focused(w));
            assert!(manager.read(cx).cancel.is_none());
            w.press("d", cx);
            tab_to(w, "hosts-close", cx);
            w.press(key, cx);
            assert!(manager.read(cx).delete.is_some());
            assert!(manager.read(cx).cancel.is_none());
            w.press("escape", cx);
        })
        .unwrap();
        assert!(store.global().unwrap().hosts == vec![host.clone()]);
    }
    assert_eq!(closed.get(), 2);
}

#[gpui_kit::test]
async fn keyboard_safety_global_empty_and_filtered_host_lists_allow_tab_traversal(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for mode in ["empty", "global", "filtered"] {
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        if mode != "empty" {
            store
                .save_host(
                    None,
                    None,
                    Host {
                        name: "keep".into(),
                        hostname: "example.test".into(),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        let (handle, manager) = keyboard_window(&store, None, cx);
        idle(handle, &manager, cx).await;
        cx.update_window(handle, |_, w, cx| {
            if mode == "filtered" {
                w.input("no-match", cx);
            }
            w.press("down", cx);
            assert!(manager.read(cx).list_focus.is_focused(w));
            let operation = manager.read(cx).operation;
            w.press("tab", cx);
            assert!(
                !manager.read(cx).list_focus.is_focused(w),
                "{mode}: Tab trapped in list"
            );
            assert_eq!(manager.read(cx).operation, operation);
            if mode == "global" {
                assert_eq!(w.find(("host-edit", 0usize)).focused(), Some(true));
                w.press("enter", cx);
                assert!(manager.read(cx).form.is_some());
                w.press("escape", cx);
            }
            w.press("ctrl-f", cx);
            w.press("down", cx);
            w.press("shift-tab", cx);
            assert!(w.find("hosts-reload").focused() == Some(true));
            assert_eq!(manager.read(cx).operation, operation);
            assert!(manager.read(cx).global);
            assert!(manager.read(cx).cancel.is_none());
        })
        .unwrap();
    }
}

#[gpui_kit::test]
async fn keyboard_safety_host_edit_tabs_skip_inactive_list_and_preserve_draft(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let host = Host {
        name: "keep".into(),
        hostname: "example.test".into(),
        root_path: "/srv".into(),
        ..Default::default()
    };
    store
        .save_host(Some("project"), None, host.clone())
        .unwrap();
    let (handle, manager) = keyboard_window(&store, Some("project"), cx);
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.press("down", cx);
        w.press("e", cx);
        w.input("unsaved", cx);
        let operation = manager.read(cx).operation;
        for key in ["tab", "shift-tab"] {
            for _ in 0..64 {
                w.press(key, cx);
                assert!(
                    !manager.read(cx).list_focus.is_focused(w),
                    "inactive list is a Tab stop"
                );
                assert!(manager.read(cx).form.is_some());
                assert_eq!(manager.read(cx).operation, operation);
                assert!(!manager.read(cx).global);
            }
        }
        assert!(
            manager.read(cx).form.as_ref().unwrap().fields[NAME]
                .read(cx)
                .value()
                .ends_with("unsaved")
        );
        tab_to(w, "host-cancel", cx);
        w.press("enter", cx);
        assert!(manager.read(cx).form.is_none());
        assert!(manager.read(cx).list_focus.is_focused(w));
    })
    .unwrap();
    assert!(store.project("project").unwrap().hosts == vec![host]);
}

#[gpui_kit::test]
async fn real_host_forms_crud_links_conflicts_and_defaults(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    fs::write(
        dir.path().join("config.toml"),
        "[defaults]\nport = 2222\nuser = 'deploy'\n",
    )
    .unwrap();
    let server = Host {
        name: "shared".into(),
        hostname: "server.example".into(),
        root_path: "/srv".into(),
        ..Host::default()
    };
    store.save_host(None, None, server.clone()).unwrap();
    let (handle, manager) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1400.), px(1200.)),
                })),
                ..WindowOptions::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    HostManager::new(
                        store.clone(),
                        BrowserService::new().unwrap(),
                        RemoteService::new(BrowserService::new().unwrap()),
                        Some("project".into()),
                        window,
                        cx,
                    )
                })
            },
        )
        .unwrap()
    });
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, window, cx| {
        window.click("host-new", cx);
        for (field, value) in [
            ("host-name", "prod"),
            ("hostname", "ftp.example"),
            ("root-path", "/project"),
            ("keep-alive", "0"),
        ] {
            window.click(field, cx);
            window.input(value, cx);
        }
        window.click("protocol-ftps", cx);
        window.click("password", cx);
        window.input("secret", cx);
        window.click("map-add", cx);
        window.click(("map-local", 0usize), cx);
        window.input("src", cx);
        window.click(("map-remote", 0usize), cx);
        window.input("deploy", cx);
        assert!(
            manager.read(cx).form.as_ref().unwrap().fields[PASSWORD]
                .read(cx)
                .presentation()
                .is_masked()
        );
        window.press("ctrl-s", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    let original = store.project("project").unwrap().hosts.remove(0);
    assert_eq!(original.protocol, "ftps");
    assert_eq!(original.port, 0);
    assert_eq!(original.keep_alive_interval, Some(0));
    assert_eq!(original.auth.password, "secret");
    assert_eq!(original.mappings[0].local, "src");
    assert_eq!(store.runtime(Some("project")).unwrap().hosts[0].port, 21);

    cx.update_window(handle, |_, window, cx| {
        window.click(("host-edit", 0usize), cx);
        window.click("hostname", cx);
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        window.input("edited.example", cx);
        window.click("host-name", cx);
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        window.input("unsaved-rename", cx);
    })
    .unwrap();
    let foreign = Host {
        user: "other process".into(),
        ..original.clone()
    };
    store
        .save_host(Some("project"), Some(&original), foreign.clone())
        .unwrap();
    cx.update_window(handle, |_, window, cx| window.press("ctrl-s", cx))
        .unwrap();
    idle(handle, &manager, cx).await;
    manager.read_with(cx, |manager, cx| {
        assert!(manager.status.contains("changed in another drift process"));
        assert_eq!(
            manager.form.as_ref().unwrap().fields[HOSTNAME]
                .read(cx)
                .value()
                .as_str(),
            "edited.example"
        );
    });
    assert_eq!(
        store.project("project").unwrap().hosts[0].hostname,
        original.hostname
    );
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        assert_eq!(manager.read(cx).cursor.as_deref(), Some("prod"));
        assert!(manager.read(cx).list_focus.is_focused(window));
        window.press("f5", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, window, cx| {
        window.click(("host-edit", 0usize), cx);
        window.click("hostname", cx);
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        window.input("edited.example", cx);
        window.press("ctrl-s", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    assert_eq!(
        store.project("project").unwrap().hosts[0].hostname,
        "edited.example"
    );
    cx.update_window(handle, |_, window, cx| {
        window.click(("host-duplicate", 0usize), cx);
        window.press("ctrl-s", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    let hosts = store.project("project").unwrap().hosts;
    assert_eq!(hosts.len(), 2);
    assert_eq!(hosts[1].name, "prod copy");
    assert_eq!(hosts[1].user, "other process");
    cx.update_window(handle, |_, window, cx| {
        window.click(("host-delete", 1usize), cx);
        window.press("enter", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    assert_eq!(store.project("project").unwrap().hosts.len(), 1);

    cx.update_window(handle, |_, window, cx| window.click("scope-global", cx))
        .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, window, cx| {
        window.click(("host-edit", 0usize), cx);
        let form = manager.read(cx).form.as_ref().unwrap();
        assert!(form.fields[PORT].read(cx).value().is_empty());
        assert!(form.fields[USER].read(cx).value().is_empty());
        window.press("ctrl-s", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    assert!(store.global().unwrap().hosts[0] == server); // resolved defaults never enter the form

    cx.update_window(handle, |_, window, cx| window.click("scope-project", cx))
        .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, window, cx| {
        window.click("host-new", cx);
        window.click("host-name", cx);
        window.input("linked", cx);
        window.click(("link", 0usize), cx);
        window.click("root-path", cx);
        window.input("/linked", cx);
        window.press("ctrl-s", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    let hosts = store.project("project").unwrap().hosts;
    assert_eq!(hosts[1].server, "shared");
    assert!(hosts[1].hostname.is_empty());
    assert!(hosts[1].auth == Default::default());
    assert_eq!(store.runtime(Some("project")).unwrap().hosts[1].port, 2222);
    cx.update_window(handle, |_, window, cx| window.click("scope-global", cx))
        .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, window, cx| {
        window.click(("host-delete", 0usize), cx);
        window.press("enter", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    manager.read_with(cx, |manager, _| {
        assert!(manager.status.contains("linked by project"));
        assert!(manager.delete.is_some());
    });
    assert_eq!(store.global().unwrap().hosts.len(), 1);
}

#[gpui_kit::test]
async fn testing_an_unsaved_form_preserves_inputs_focus_and_cancels_real_io(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = crate::sftp_test_support::ftp::Server::new(4);
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    fs::write(
        dir.path().join("config.toml"),
        format!("[defaults]\nport = {}\nuser = 'testuser'\n", server.port),
    )
    .unwrap();
    let original = fs::read(dir.path().join("config.toml")).unwrap();
    let (handle, manager) = cx.update(|cx| {
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
                        RemoteService::with_options(service, server.options()),
                        None,
                        w,
                        cx,
                    )
                })
            },
        )
        .unwrap()
    });
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.click("host-new", cx);
        for (field, value) in [
            ("host-name", "unsaved"),
            ("hostname", "127.0.0.1"),
            ("root-path", "/"),
        ] {
            w.click(field, cx);
            w.input(value, cx);
        }
        w.click("protocol-ftp", cx);
        w.click("password", cx);
        w.input("test-password", cx);
        w.click("host-test-draft", cx);
    })
    .unwrap();
    let tools = manager.read_with(cx, |manager, _| manager.tools.as_ref().unwrap().clone());
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        tools.read(cx).cancel.is_none()
    })
    .await;
    assert!(tools.read_with(cx, |tools, _| tools.status.contains("successful")));
    let previous_focus = tools.read_with(cx, |tools, _| tools.previous_focus.clone().unwrap());
    let before = server.commands("LIST");
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let form = manager.read(cx).form.as_ref().unwrap();
        assert_eq!(form.fields[NAME].read(cx).value(), "unsaved");
        assert_eq!(form.fields[PASSWORD].read(cx).value(), "test-password");
        assert!(form.fields[PORT].read(cx).value().is_empty());
        assert!(form.fields[USER].read(cx).value().is_empty());
        assert!(manager.read(cx).tools.is_none());
        // Returning from the test restores the initiating control's focus.
        assert!(previous_focus.is_focused(w));
        server.flag("hold-list", "");
        w.click("host-test-draft", cx);
    })
    .unwrap();
    let tools = manager.read_with(cx, |manager, _| manager.tools.as_ref().unwrap().clone());
    cx.wait_for(handle, Duration::from_secs(30), |_, _| {
        server.commands("LIST") > before
    })
    .await;
    let cancel = tools.read_with(cx, |tools, _| tools.cancel.as_ref().unwrap().clone());
    cx.update_window(handle, |_, w, cx| w.click("host-tools-close", cx))
        .unwrap();
    assert!(cancel.is_cancelled());
    assert!(manager.read_with(cx, |manager, _| manager.tools.is_none()
        && manager.form.is_some()));
    fs::remove_file(server.dir.path().join("hold-list")).unwrap();
    assert_eq!(fs::read(dir.path().join("config.toml")).unwrap(), original);
    assert!(store.global().unwrap().hosts.is_empty());
    assert!(!dir.path().join("projects.toml").exists());
}

#[gpui_kit::test]
async fn keyboard_hosts_crud_scope_links_and_real_connection_test(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = crate::sftp_test_support::Server::new(false);
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    for name in ["alpha", "zulu"] {
        let mut host = server.host();
        host.name = name.into();
        store.save_host(Some("project"), None, host).unwrap();
    }
    let mut global = server.host();
    global.name = "shared".into();
    store.save_host(None, None, global).unwrap();
    let (handle, manager) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1400.), px(1000.)),
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
                        RemoteService::with_options(service, server.options()),
                        Some("project".into()),
                        w,
                        cx,
                    )
                })
            },
        )
        .unwrap()
    });
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.press("down", cx);
        for (key, name) in [
            ("end", "zulu"),
            ("g", "alpha"),
            ("j", "zulu"),
            ("k", "alpha"),
            ("shift-g", "zulu"),
        ] {
            w.press(key, cx);
            assert_eq!(manager.read(cx).selected_host(cx).unwrap().name, name);
        }
        w.press("c", cx);
        assert!(manager.read(cx).form.as_ref().unwrap().expected.is_none());
        assert_eq!(
            manager.read(cx).form.as_ref().unwrap().fields[NAME]
                .read(cx)
                .value()
                .as_str(),
            "zulu copy"
        );
        w.press("ctrl-f", cx);
        assert!(
            manager.read(cx).form.as_ref().unwrap().fields[NAME]
                .focus_handle(cx)
                .is_focused(w)
        );
        w.press("ctrl-s", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        assert!(manager.read(cx).offer.is_some());
        w.press("n", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    assert_eq!(store.project("project").unwrap().hosts.len(), 3);
    cx.update_window(handle, |_, w, cx| {
        assert!(manager.read(cx).list_focus.is_focused(w));
        assert_eq!(
            manager.read(cx).selected_host(cx).unwrap().name,
            "zulu copy"
        );
        w.press("e", cx);
        w.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        w.input("edited", cx);
        w.press("cmd-s", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        assert!(manager.read(cx).offer.is_some());
        w.press("n", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(manager.read(cx).selected_host(cx).unwrap().name, "edited");
        w.press("d", cx);
        assert_eq!(manager.read(cx).delete.as_ref().unwrap().name, "edited");
        w.press("escape", cx);
        assert!(manager.read(cx).list_focus.is_focused(w));
        w.press("delete", cx);
        w.press("y", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    assert_eq!(store.project("project").unwrap().hosts.len(), 2);
    cx.update_window(handle, |_, w, cx| {
        w.press("tab", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        assert!(manager.read(cx).global);
        assert_eq!(manager.read(cx).selected_host(cx).unwrap().name, "shared");
        w.press("shift-tab", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.press("l", cx);
        assert!(manager.read(cx).links.is_some());
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(manager.read(cx).links.is_none());
        assert!(manager.read(cx).list_focus.is_focused(w));
        w.press("home", cx);
        w.press("r", cx); // SFTP has no FTPS trust to reset.
        assert!(manager.read(cx).tools.is_none());
        w.press("t", cx);
        assert!(manager.read(cx).tools.is_some());
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        manager
            .read(cx)
            .tools
            .as_ref()
            .is_some_and(|tools| tools.read(cx).cancel.is_none())
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert!(
            manager
                .read(cx)
                .tools
                .as_ref()
                .unwrap()
                .read(cx)
                .status
                .contains("Connection successful")
        );
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(manager.read(cx).list_focus.is_focused(w));
        w.press("/", cx);
        w.input("ncedtrljkgGy", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(
            manager.read(cx).query.read(cx).value().as_str(),
            "ncedtrljkgGy"
        );
        assert!(manager.read(cx).form.is_none());
        assert!(manager.read(cx).delete.is_none());
        assert!(manager.read(cx).visible_hosts(cx).is_empty());
        w.press("enter", cx);
        w.press("home", cx);
        w.press("delete", cx);
        assert!(manager.read(cx).delete.is_none());
        w.press("n", cx);
        w.input("new", cx);
        w.click("hostname", cx);
        w.input("example.test", cx);
        w.press("ctrl-s", cx);
        assert!(manager.read(cx).form.is_some());
        assert!(manager.read(cx).status.contains("Root path is required"));
        w.click("root-path", cx);
        w.input("/srv", cx);
        w.press("ctrl-s", cx);
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    assert!(
        store
            .project("project")
            .unwrap()
            .hosts
            .iter()
            .any(|h| h.name == "new")
    );
}
