use super::*;
use crate::{actions::bind_keys, sftp_test_support as support};
use gpui_kit::base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, path::Path, time::Duration};

async fn open_browser(
    store: &Store,
    local: &Path,
    server: &support::Server,
    cx: &mut TestAppContext,
) -> (gpui_kit::AnyWindowHandle, Entity<Shell>) {
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
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
                    Shell::new(
                        w,
                        cx,
                        store.clone(),
                        service.clone(),
                        RemoteService::with_options(service, server.options()),
                        local.to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
            && !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.click("remote", cx);
        w.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    assert!(shell.read_with(cx, |s, cx| s.remote.read(cx).has_session()));
    (handle, shell)
}

async fn wait_idle(
    handle: gpui_kit::AnyWindowHandle,
    shell: &Entity<Shell>,
    cx: &mut TestAppContext,
) {
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        let s = shell.read(cx);
        !s.browser.read(cx).is_loading()
            && !s.remote.read(cx).is_loading()
            && !s.comparison.read(cx).is_loading()
            && !s.preview.read(cx).is_loading()
    })
    .await;
}

fn menu_item(window: &Window, label: &str) -> ElementSnapshot {
    let items: Vec<_> = snapshots(window)
        .into_iter()
        .filter(|item| {
            item.role() == Some(gpui_kit::Role::MenuItem)
                && item.label().is_some_and(|text| text.starts_with(label))
                && item.visible()
        })
        .collect();
    assert_eq!(
        items.len(),
        1,
        "expected one visible menu item starting with {label}"
    );
    items.into_iter().next().unwrap()
}

fn click_menu(window: &mut Window, label: &str, cx: &mut gpui_kit::App) {
    let item = menu_item(window, label);
    window
        .within("popup-menu")
        .click(item.path().last().unwrap().clone(), cx);
}

#[gpui_kit::test]
async fn context_compare_uses_only_its_path_preserves_marks_and_requires_sync_confirmation(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    fs::write(local.path().join("a-local"), "upload only this").unwrap();
    fs::write(local.path().join("b-mark"), "leave local mark").unwrap();
    fs::write(remote.join("b-mark"), "leave remote mark").unwrap();
    fs::write(remote.join("c-remote"), "download only this").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_browser(&store, local.path(), &server, cx).await;
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("space", cx);
        shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("end", cx);
        w.press("space", cx);
        w.within("local-files").click(1usize, cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    cx.update_window(handle, |_, w, cx| {
        let preview = shell.read(cx).preview.read(cx).id();
        w.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "leave local mark"
        );
        w.within("local-files").right_click(0usize, cx);
        assert_eq!(shell.read(cx).browser.read(cx).selected(), Some("a-local"));
        assert_eq!(shell.read(cx).preview.read(cx).id(), preview);
        assert_eq!(shell.read(cx).browser.read(cx).marked(), ["b-mark"]);
        assert_eq!(
            shell.read(cx).remote.read(cx).marked(),
            [remote.join("b-mark").to_string_lossy()]
        );
        click_menu(w, "Compare this ", cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    shell.read_with(cx, |s, cx| {
        let pane = s.comparison.read(cx);
        let session = pane.session.as_ref().unwrap();
        assert_eq!(session.request.local, [PathBuf::from("a-local")]);
        assert!(session.request.remote.is_empty());
        assert!(!session.request.scope.include_ignored);
        assert_eq!(session.entries.len(), 1);
        assert_eq!(session.entries[0].local, local.path().join("a-local"));
        assert!(pane.sync_result_for_test().is_none());
        assert_eq!(s.browser.read(cx).marked(), ["b-mark"]);
        assert_eq!(
            s.remote.read(cx).marked(),
            [remote.join("b-mark").to_string_lossy()]
        );
    });
    cx.update_window(handle, |_, w, cx| {
        assert!(w.try_find("popup-menu").is_none());
        assert!(shell.read(cx).comparison.focus_handle(cx).is_focused(w));
        w.press("ctrl-enter", cx);
        assert!(!shell.read(cx).comparison.read(cx).is_loading());
        w.click("sync-selected", cx);
    })
    .unwrap();
    assert!(!remote.join("a-local").exists());
    assert!(!local.path().join("c-remote").exists());
    cx.update_window(handle, |_, w, cx| w.click("sync-confirm", cx))
        .unwrap();
    wait_idle(handle, &shell, cx).await;
    assert_eq!(
        fs::read(remote.join("a-local")).unwrap(),
        b"upload only this"
    );
    cx.update_window(handle, |_, w, cx| {
        w.click("comparison-back", cx);
        w.click("remote", cx);
        // The uploaded path is now row zero; b-mark remains the marked preview source.
        w.within("remote-files").click(1usize, cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    cx.update_window(handle, |_, w, cx| {
        let preview = shell.read(cx).preview.read(cx).id();
        w.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "leave remote mark"
        );
        w.within("remote-files").right_click(2usize, cx);
        assert_eq!(
            shell.read(cx).remote.read(cx).selected(),
            Some(remote.join("c-remote").to_string_lossy().as_ref())
        );
        assert_eq!(shell.read(cx).preview.read(cx).id(), preview);
        click_menu(w, "Compare this ", cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    shell.read_with(cx, |s, cx| {
        let pane = s.comparison.read(cx);
        let session = pane.session.as_ref().unwrap();
        assert!(session.request.local.is_empty());
        assert_eq!(
            session.request.remote,
            [remote.join("c-remote").to_string_lossy()]
        );
        assert_eq!(session.entries.len(), 1);
        assert_eq!(session.entries[0].local, local.path().join("c-remote"));
        assert!(pane.sync_result_for_test().is_none());
        assert_eq!(s.browser.read(cx).marked(), ["b-mark"]);
        assert_eq!(
            s.remote.read(cx).marked(),
            [remote.join("b-mark").to_string_lossy()]
        );
    });
    cx.update_window(handle, |_, w, cx| {
        w.press("ctrl-enter", cx);
        assert!(!shell.read(cx).comparison.read(cx).is_loading());
        w.click("sync-selected", cx);
    })
    .unwrap();
    assert!(!local.path().join("c-remote").exists());
    cx.update_window(handle, |_, w, cx| w.click("sync-confirm", cx))
        .unwrap();
    wait_idle(handle, &shell, cx).await;
    assert_eq!(
        fs::read(local.path().join("c-remote")).unwrap(),
        b"download only this"
    );
    assert_eq!(
        fs::read(local.path().join("b-mark")).unwrap(),
        b"leave local mark"
    );
    assert_eq!(
        fs::read(remote.join("b-mark")).unwrap(),
        b"leave remote mark"
    );
}

#[gpui_kit::test]
async fn context_marked_scope_includes_both_panes_and_collapsed_descendants(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    fs::create_dir(local.path().join("folder")).unwrap();
    fs::write(local.path().join("folder/child"), "upload child").unwrap();
    fs::write(local.path().join("unselected"), "not selected").unwrap();
    fs::create_dir(remote.join("folder")).unwrap();
    fs::write(remote.join("folder/remote-child"), "download child").unwrap();
    fs::write(remote.join("unselected"), "leave remote").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_browser(&store, local.path(), &server, cx).await;
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("right", cx);
        shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("right", cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("right", cx);
        w.press("space", cx);
        w.press("left", cx);
        shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("right", cx);
        w.press("space", cx);
        w.press("left", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let id = shell.read(cx).browser.read(cx).id();
        w.within("local-files").right_click(0usize, cx);
        assert_eq!(shell.read(cx).browser.read(cx).id(), id);
        assert!(!shell.read(cx).browser.read(cx).is_loading());
        assert_eq!(
            shell
                .read(cx)
                .browser
                .read(cx)
                .location()
                .unwrap()
                .directory,
            local.path()
        );
        click_menu(w, "Compare this ", cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    shell.read_with(cx, |s, cx| {
        let session = s.comparison.read(cx).session.as_ref().unwrap();
        assert_eq!(session.request.local, [PathBuf::from("folder")]);
        assert!(session.request.remote.is_empty());
        assert_eq!(session.entries.len(), 2);
        assert_eq!(s.browser.read(cx).marked(), ["folder/child"]);
        assert_eq!(s.remote.read(cx).marked().len(), 1);
    });
    cx.update_window(handle, |_, w, cx| w.click("comparison-back", cx))
        .unwrap();
    for remote_menu in [false, true] {
        cx.update_window(handle, |_, w, cx| {
            if remote_menu {
                w.within("remote-files").right_click(1usize, cx);
            } else {
                w.within("local-files").right_click(1usize, cx);
            }
            click_menu(w, "Compare marked ", cx);
        })
        .unwrap();
        wait_idle(handle, &shell, cx).await;
        shell.read_with(cx, |s, cx| {
            let pane = s.comparison.read(cx);
            let session = pane.session.as_ref().unwrap();
            assert_eq!(session.request.local, [PathBuf::from("folder/child")]);
            assert_eq!(
                session.request.remote,
                [remote.join("folder/remote-child").to_string_lossy()]
            );
            assert_eq!(session.entries.len(), 2);
            assert_eq!(s.browser.read(cx).marked(), ["folder/child"]);
            assert_eq!(
                s.remote.read(cx).marked(),
                [remote.join("folder/remote-child").to_string_lossy()]
            );
            assert!(pane.sync_result_for_test().is_none());
        });
        assert!(!remote.join("folder/child").exists());
        assert!(!local.path().join("folder/remote-child").exists());
        cx.update_window(handle, |_, w, cx| w.click("comparison-back", cx))
            .unwrap();
    }
    for button in ["compare-local", "compare-remote"] {
        cx.update_window(handle, |_, w, cx| w.click(button, cx))
            .unwrap();
        wait_idle(handle, &shell, cx).await;
        shell.read_with(cx, |s, cx| {
            let session = s.comparison.read(cx).session.as_ref().unwrap();
            if button == "compare-local" {
                assert_eq!(session.request.local, [PathBuf::from("folder/child")]);
                assert!(session.request.remote.is_empty());
            } else {
                assert!(session.request.local.is_empty());
                assert_eq!(
                    session.request.remote,
                    [remote.join("folder/remote-child").to_string_lossy()]
                );
            }
            assert_eq!(session.entries.len(), 1);
        });
        cx.update_window(handle, |_, w, cx| w.click("comparison-back", cx))
            .unwrap();
    }
    cx.update_window(handle, |_, w, cx| {
        w.within("local-files").right_click(0usize, cx);
        click_menu(w, "Compare project", cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    shell.read_with(cx, |s, cx| {
        let session = s.comparison.read(cx).session.as_ref().unwrap();
        assert!(session.request.local.is_empty());
        assert!(session.request.remote.is_empty());
        assert_eq!(session.entries.len(), 3);
        assert_eq!(s.browser.read(cx).marked(), ["folder/child"]);
        assert_eq!(s.remote.read(cx).marked().len(), 1);
    });
    cx.update_window(handle, |_, w, cx| {
        w.click("comparison-back", cx);
        w.within("remote-files").right_click(0usize, cx);
        click_menu(w, "Clear all marks", cx);
        assert!(shell.read(cx).remote.read(cx).marked().is_empty());
        w.press("shift-f10", cx);
        click_menu(w, "Compare marked files", cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    shell.read_with(cx, |s, cx| {
        let session = s.comparison.read(cx).session.as_ref().unwrap();
        assert_eq!(session.request.local, [PathBuf::from("folder/child")]);
        assert!(session.request.remote.is_empty());
        assert_eq!(session.entries.len(), 1);
    });
    assert!(!remote.join("folder/child").exists());
}

#[gpui_kit::test]
async fn menu_escape_preserves_filter_range_marks_and_returns_focus(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    for base in [local.path(), remote.as_path()] {
        for name in ["keep-a", "keep-b", "keep-c", "other"] {
            fs::write(base.join(name), name).unwrap();
        }
    }
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_browser(&store, local.path(), &server, cx).await;
    for remote_menu in [false, true] {
        let filter = if remote_menu {
            "remote-filter"
        } else {
            "filter"
        };
        cx.update_window(handle, |_, w, cx| {
            let list = if remote_menu {
                "remote-files"
            } else {
                "local-files"
            };
            w.click(filter, cx);
            w.input("keep", cx);
            if remote_menu {
                shell.read(cx).remote.focus_handle(cx).focus(w, cx);
            } else {
                shell.read(cx).browser.focus_handle(cx).focus(w, cx);
            }
            w.press("home", cx);
            w.press("space", cx);
            w.press("v", cx);
            w.within(list).right_click(1usize, cx);
            assert_eq!(w.find("popup-menu").focused(), Some(true));
            w.press("escape", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert!(w.try_find("popup-menu").is_none());
            assert_eq!(w.find(filter).value(), Some("keep"));
            if remote_menu {
                assert!(shell.read(cx).remote.focus_handle(cx).is_focused(w));
                assert_eq!(
                    shell.read(cx).remote.read(cx).marked(),
                    [remote.join("keep-a").to_string_lossy()]
                );
            } else {
                assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
                assert_eq!(shell.read(cx).browser.read(cx).len(), 3);
                assert_eq!(shell.read(cx).browser.read(cx).marked(), ["keep-a"]);
            }
            // Finishing the original interval proves Escape did not cancel its anchor.
            w.press("v", cx);
            if remote_menu {
                assert_eq!(
                    shell.read(cx).remote.read(cx).marked(),
                    [
                        remote.join("keep-a").to_string_lossy(),
                        remote.join("keep-b").to_string_lossy()
                    ]
                );
            } else {
                assert_eq!(
                    shell.read(cx).browser.read(cx).marked(),
                    ["keep-a", "keep-b"]
                );
            }
        })
        .unwrap();
    }
}

#[gpui_kit::test]
async fn busy_panes_reject_comparison_even_for_queued_menu_events(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    fs::write(local.path().join("a"), "local").unwrap();
    fs::write(remote.join("a"), "remote").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_browser(&store, local.path(), &server, cx).await;
    cx.update_window(handle, |_, w, cx| {
        let browser = shell.read(cx).browser.clone();
        let remote_pane = shell.read(cx).remote.clone();
        remote_pane.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        browser.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("space", cx);
        remote_pane.update(cx, |pane, cx| pane.refresh(&Refresh, w, cx));
        assert!(remote_pane.read(cx).is_loading());
        w.within("local-files").right_click(0usize, cx);
        click_menu(w, "Compare this ", cx);
        click_menu(w, "Compare marked ", cx);
        assert!(!shell.read(cx).comparison.read(cx).visible());
        w.press("escape", cx);
        remote_pane.update(cx, |pane, cx| {
            cx.emit(RemoteEvent::Compare {
                project: pane.project(),
                connection: pane.connection(),
                path: remote.join("a").to_string_lossy().into_owned(),
            });
            cx.emit(RemoteEvent::Toolbar {
                project: pane.project(),
                connection: pane.connection(),
                event: ToolbarEvent::CompareMarked,
            });
        });
        shell.update(cx, |s, cx| {
            let id = s.browser.read(cx).id();
            s.browser_event(
                &BrowserEvent::Compare {
                    id,
                    connection: s.remote.read(cx).session_id().unwrap(),
                    path: "a".into(),
                },
                w,
                cx,
            );
            s.toolbar_event(&ToolbarEvent::CompareProject, w, cx);
            s.open_comparison(
                vec![],
                vec![remote.join("a").to_string_lossy().into_owned()],
                w,
                cx,
            );
            assert!(!s.comparison.read(cx).visible());
        });
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    cx.update_window(handle, |_, w, cx| {
        let browser = shell.read(cx).browser.clone();
        browser.update(cx, |pane, cx| pane.refresh(&Refresh, w, cx));
        assert!(browser.read(cx).is_loading());
        shell.update(cx, |s, cx| {
            s.open_comparison(vec!["a".into()], vec![], w, cx);
            assert!(!s.comparison.read(cx).visible());
        });
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    assert!(!shell.read_with(cx, |s, cx| s.comparison.read(cx).visible()));
    assert_eq!(
        shell.read_with(cx, |s, cx| s.browser.read(cx).marked()),
        ["a"]
    );
    assert_eq!(fs::read(local.path().join("a")).unwrap(), b"local");
    assert_eq!(fs::read(remote.join("a")).unwrap(), b"remote");
    cx.update_window(handle, |_, w, cx| {
        shell.update(cx, |s, cx| {
            s.remote.update(cx, |pane, cx| pane.disconnect(cx));
            let preview_id = s.preview.read(cx).id();
            s.open_comparison(vec!["a".into()], vec![], w, cx);
            assert!(!s.comparison.read(cx).visible());
            assert_eq!(s.preview.read(cx).id(), preview_id);
            s.browser.update(cx, |pane, cx| pane.invalidate_project(cx));
            s.open_comparison(vec!["a".into()], vec![], w, cx);
            assert!(!s.comparison.read(cx).visible());
        });
    })
    .unwrap();
}

#[gpui_kit::test]
async fn stale_path_operation_project_and_connection_events_are_rejected(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote = server.dir.path().join("files");
    for base in [local.path(), other.path(), remote.as_path()] {
        fs::write(base.join("a"), "a").unwrap();
        fs::write(base.join("b"), "b").unwrap();
    }
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_browser(&store, local.path(), &server, cx).await;
    let (old_id, old_connection, old_session) = shell.read_with(cx, |s, cx| {
        (
            s.browser.read(cx).id(),
            s.remote.read(cx).connection(),
            s.remote.read(cx).session_id().unwrap(),
        )
    });
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        let browser = shell.read(cx).browser.clone();
        browser.update(cx, |pane, cx| pane.refresh(&Refresh, w, cx));
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    cx.update_window(handle, |_, w, cx| {
        shell.update(cx, |s, cx| {
            assert_ne!(s.browser.read(cx).id(), old_id);
            assert_eq!(s.browser.read(cx).id().project, old_id.project);
            s.browser_event(
                &BrowserEvent::Compare {
                    id: old_id,
                    connection: old_session,
                    path: "a".into(),
                },
                w,
                cx,
            );
            s.browser_event(
                &BrowserEvent::Toolbar {
                    id: old_id,
                    connection: old_session,
                    event: ToolbarEvent::Hosts,
                },
                w,
                cx,
            );
            let id = s.browser.read(cx).id();
            s.browser_event(
                &BrowserEvent::Compare {
                    id,
                    connection: s.remote.read(cx).session_id().unwrap(),
                    path: "b".into(),
                },
                w,
                cx,
            );
            assert!(s.hosts.is_none());
            assert!(!s.comparison.read(cx).visible());
        });
        let pane = shell.read(cx).remote.clone();
        pane.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        pane.update(cx, |pane, cx| {
            cx.emit(RemoteEvent::Compare {
                project: pane.project(),
                connection: pane.connection(),
                path: remote.join("b").to_string_lossy().into_owned(),
            })
        });
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(!shell.read(cx).comparison.read(cx).visible());
        w.click("disconnect", cx);
        w.click(("connect-host", 0usize), cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    cx.update_window(handle, |_, w, cx| {
        let pane = shell.read(cx).remote.clone();
        pane.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        assert_ne!(pane.read(cx).connection(), old_connection);
        shell.update(cx, |s, cx| {
            let id = s.browser.read(cx).id();
            assert_eq!(s.browser.read(cx).selected(), Some("a"));
            s.browser_event(
                &BrowserEvent::Compare {
                    id,
                    connection: old_session,
                    path: "a".into(),
                },
                w,
                cx,
            );
            s.browser_event(
                &BrowserEvent::Toolbar {
                    id,
                    connection: old_session,
                    event: ToolbarEvent::Hosts,
                },
                w,
                cx,
            );
        });
        pane.update(cx, |_, cx| {
            cx.emit(RemoteEvent::Compare {
                project: old_id.project,
                connection: old_connection,
                path: remote.join("a").to_string_lossy().into_owned(),
            });
            cx.emit(RemoteEvent::Toolbar {
                project: old_id.project,
                connection: old_connection,
                event: ToolbarEvent::Hosts,
            });
        });
        w.render_frame(cx);
        assert!(!shell.read(cx).comparison.read(cx).visible());
        assert!(shell.read(cx).hosts.is_none());
    })
    .unwrap();
    let project_id = shell.read_with(cx, |s, cx| s.browser.read(cx).id());
    cx.update_window(handle, |_, w, cx| {
        assert!(!shell.read(cx).comparison.read(cx).visible());
        assert!(shell.read(cx).hosts.is_none());
        shell.update(cx, |s, cx| s.open_project(other.path().into(), w, cx));
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    cx.update_window(handle, |_, w, cx| w.click(("connect-host", 0usize), cx))
        .unwrap();
    wait_idle(handle, &shell, cx).await;
    cx.update_window(handle, |_, w, cx| {
        shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        shell.update(cx, |s, cx| {
            s.browser_event(
                &BrowserEvent::Compare {
                    id: project_id,
                    connection: old_session,
                    path: "a".into(),
                },
                w,
                cx,
            );
            s.browser_event(
                &BrowserEvent::Toolbar {
                    id: project_id,
                    connection: old_session,
                    event: ToolbarEvent::Hosts,
                },
                w,
                cx,
            );
        });
        let pane = shell.read(cx).remote.clone();
        pane.update(cx, |pane, cx| {
            // A current connection number must not authorize another project's event.
            cx.emit(RemoteEvent::Compare {
                project: project_id.project,
                connection: pane.connection(),
                path: remote.join("a").to_string_lossy().into_owned(),
            });
            cx.emit(RemoteEvent::Toolbar {
                project: project_id.project,
                connection: pane.connection(),
                event: ToolbarEvent::Hosts,
            });
        });
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        assert!(!shell.read(cx).comparison.read(cx).visible());
        assert!(shell.read(cx).hosts.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn hidden_pane_menus_close_without_taking_modal_focus(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("a"), "a").unwrap();
    fs::write(server.dir.path().join("files/a"), "remote a").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let (handle, shell) = open_browser(&store, local.path(), &server, cx).await;
    for remote_menu in [false, true] {
        for event in [
            ToolbarEvent::Projects,
            ToolbarEvent::Hosts,
            ToolbarEvent::CompareProject,
        ] {
            cx.update_window(handle, |_, w, cx| {
                let list = if remote_menu {
                    "remote-files"
                } else {
                    "local-files"
                };
                w.within(list).right_click(0usize, cx);
                shell.update(cx, |s, cx| s.toolbar_event(&event, w, cx));
                w.render_frame(cx);
                assert!(w.try_find("popup-menu").is_none());
                shell.read_with(cx, |s, cx| {
                    assert!(!s.browser.focus_handle(cx).is_focused(w));
                    assert!(!s.remote.focus_handle(cx).is_focused(w));
                });
            })
            .unwrap();
            wait_idle(handle, &shell, cx).await;
            cx.update_window(handle, |_, w, cx| match event {
                ToolbarEvent::Projects | ToolbarEvent::Hosts => w.press("escape", cx),
                ToolbarEvent::CompareProject => w.click("comparison-back", cx),
                _ => unreachable!(),
            })
            .unwrap();
            cx.update_window(handle, |_, w, cx| {
                assert!(shell.read(cx).browser_screen_active(cx));
                assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
                w.press("shift-f10", cx);
                assert!(w.find("popup-menu").visible());
                w.press("escape", cx);
            })
            .unwrap();
        }
    }
    let session_id = shell.read_with(cx, |s, cx| s.remote.read(cx).session_id());
    cx.update_window(handle, |_, w, cx| {
        w.within("local-files").right_click(0usize, cx);
        shell.read(cx).remote.focus_handle(cx).focus(w, cx);
        w.press("enter", cx);
    })
    .unwrap();
    wait_idle(handle, &shell, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(shell.read(cx).remote_preview);
        assert!(w.try_find("popup-menu").is_none());
        assert!(shell.read(cx).remote.focus_handle(cx).is_focused(w));
        w.press("shift-f10", cx);
        assert!(w.find("popup-menu").visible());
        shell
            .read(cx)
            .remote
            .clone()
            .update(cx, |_, cx| cx.emit(RemoteEvent::ShowLocalPreview));
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!shell.read(cx).show_remote);
        assert!(w.try_find("popup-menu").is_none());
        assert!(shell.read(cx).browser.focus_handle(cx).is_focused(w));
        w.click("remote", cx);
        assert!(w.try_find("popup-menu").is_none());
        assert_eq!(shell.read(cx).remote.read(cx).session_id(), session_id);
        w.within("local-files").right_click(0usize, cx);
        let modal_focus = shell.update(cx, |s, cx| {
            let focus = cx.focus_handle();
            focus.focus(w, cx);
            s.startup = Some(CancellationToken::new());
            cx.notify();
            focus
        });
        w.render_frame(cx);
        assert!(w.try_find("popup-menu").is_none());
        assert!(modal_focus.is_focused(w));
        shell.update(cx, |s, cx| {
            s.startup.take().unwrap().cancel();
            cx.notify();
        });
    })
    .unwrap();
}
