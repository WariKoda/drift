use super::*;
use crate::pane_tabs::{PaneSide, PaneView};
use gpui_kit::component::ActiveTheme;

struct ConnectedPanes {
    f: Fixture,
    server: support::Server,
    local: tempfile::TempDir,
    _config: tempfile::TempDir,
}
impl ConnectedPanes {
    async fn new(cx: &mut TestAppContext) -> Self {
        let server = support::Server::new(false);
        let local = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        fs::create_dir(local.path().join("nested")).unwrap();
        fs::create_dir(server.dir.path().join("files/nested")).unwrap();
        for (name, left, right) in [
            ("a.txt", "LOCAL a\nUnicode Ω", "REMOTE a\nUnicode ä"),
            ("b.txt", "LOCAL b", "REMOTE b"),
        ] {
            fs::write(local.path().join("nested").join(name), left).unwrap();
            fs::write(server.dir.path().join("files/nested").join(name), right).unwrap();
        }
        let store = Store::new(config.path().into());
        store.register("Panes", local.path().into()).unwrap();
        store.save_host(Some("panes"), None, server.host()).unwrap();
        let f = Fixture::new(
            1400.,
            1100.,
            store,
            &local.path().join("nested"),
            Some(server.options()),
            cx,
        )
        .await;
        cx.update_window(f.handle, |_, w, cx| {
            w.click("remote-files-tab", cx);
            crate::test_support::connect_first(w, cx);
        })
        .unwrap();
        cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
            f.shell.read(cx).remote.read(cx).has_session()
                && !f.shell.read(cx).remote.read(cx).is_loading()
        })
        .await;
        cx.update_window(f.handle, |_, w, cx| {
            w.within("remote-files").click(0usize, cx);
            w.press("alt-enter", cx);
        })
        .unwrap();
        cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
            !f.shell.read(cx).remote.read(cx).is_loading()
        })
        .await;
        cx.update_window(f.handle, |_, w, cx| {
            w.click("local-files-tab", cx);
            w.press("home", cx);
            w.press("space", cx);
            w.click("remote-files-tab", cx);
            w.press("home", cx);
            w.press("space", cx);
            assert_eq!(f.shell.read(cx).browser.read(cx).marked(), ["nested/a.txt"]);
            assert_eq!(
                f.shell.read(cx).remote.read(cx).marked(),
                [server
                    .dir
                    .path()
                    .join("files/nested/a.txt")
                    .to_string_lossy()]
            );
        })
        .unwrap();
        Self {
            f,
            server,
            local,
            _config: config,
        }
    }
    fn request_previews(&self, w: &mut Window, cx: &mut gpui_kit::App) {
        let s = self.f.shell.read(cx);
        let project = s.browser.read(cx).id().project;
        let location = s.browser.read(cx).location().unwrap().clone();
        let session = s.remote.read(cx).comparison_target().unwrap().1;
        let left = s.local_preview.clone();
        let right = s.remote_preview.clone();
        left.update(cx, |pane, cx| {
            pane.show(location, "nested/a.txt".into(), project, w, cx)
        });
        right.update(cx, |pane, cx| {
            pane.show_remote(
                session,
                self.server
                    .dir
                    .path()
                    .join("files/nested/a.txt")
                    .to_string_lossy()
                    .into_owned(),
                project,
                w,
                cx,
            )
        });
        self.f.shell.update(cx, |s, cx| {
            s.reveal_preview(PaneSide::Local, w, cx);
            s.reveal_preview(PaneSide::Remote, w, cx);
        });
    }
    async fn previews(&self, cx: &mut TestAppContext) {
        cx.update_window(self.f.handle, |_, w, cx| {
            w.click("local-preview-tab", cx);
            w.click("remote-preview-tab", cx);
        })
        .unwrap();
        self.idle(cx).await;
    }
    async fn idle(&self, cx: &mut TestAppContext) {
        cx.wait_for(self.f.handle, Duration::from_secs(30), |_, cx| {
            !self.f.shell.read(cx).local_preview.read(cx).is_loading()
                && !self.f.shell.read(cx).remote_preview.read(cx).is_loading()
        })
        .await;
    }
    async fn shutdown(&self, cx: &mut TestAppContext) {
        let connection = self.f.shell.read_with(cx, |s, cx| {
            s.remote
                .read(cx)
                .comparison_target()
                .map(|(_, session)| session)
        });
        if let Some(connection) = connection {
            connection.shutdown().await;
        }
    }
}

fn copy(w: &mut Window, scope: &'static str, expected: &str, cx: &mut gpui_kit::App) {
    w.within(scope).click("copy-preview", cx);
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some(expected)
    );
}

#[gpui_kit::test]
async fn fixed_pane_tabs_default_to_files_and_reject_directories_without_a_cached_document(
    cx: &mut TestAppContext,
) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir(local.path().join("folder")).unwrap();
    let f = Fixture::new(
        1400.,
        1100.,
        Store::new(config.path().into()),
        local.path(),
        None,
        cx,
    )
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("local-files-tab", cx);
        w.press("home", cx);
        w.render_frame(cx);
        for (scope, tabs, files, preview, files_name, preview_name) in [
            (
                "local-workspace",
                "local-pane-tabs",
                "local-files-tab",
                "local-preview-tab",
                "Local files",
                "Local preview",
            ),
            (
                "remote-workspace",
                "remote-pane-tabs",
                "remote-files-tab",
                "remote-preview-tab",
                "Remote files",
                "Remote preview",
            ),
        ] {
            assert!(w.within(scope).find(tabs).visible());
            let files_button = w.within(tabs).find(files);
            assert_eq!(files_button.role(), Some(gpui_kit::Role::Button));
            assert_eq!(files_button.label(), Some(files_name));
            let selected_bounds = files_button.bounds().scale(w.scale_factor());
            assert!(
                w.painted_quads().iter().any(|q| q.bounds == selected_bounds
                    && q.background == cx.theme().tokens.secondary_active.into()),
                "native selected files button"
            );
            let preview_button = w.within(tabs).find(preview);
            assert_eq!(preview_button.role(), Some(gpui_kit::Role::Button));
            assert_eq!(preview_button.label(), Some(preview_name));
            assert_ne!(preview_button.selected(), Some(true));
            w.click(preview, cx);
            assert_ne!(
                w.find(preview).focused(),
                Some(true),
                "disabled preview tab must be inert"
            );
        }
        let s = f.shell.read(cx);
        assert_eq!(s.pane_view(PaneSide::Local), PaneView::Files);
        assert_eq!(s.pane_view(PaneSide::Remote), PaneView::Files);
        assert_ne!(
            s.preview_pane(PaneSide::Local),
            s.preview_pane(PaneSide::Remote)
        );
        assert!(w.within("local-workspace").find("browser-pane").visible());
        assert!(w.within("remote-workspace").find("remote-pane").visible());
        for id in ["remote", "local-browser", "local-preview", "preview-header"] {
            assert!(
                w.try_find(id).is_none(),
                "removed/empty preview control {id}"
            );
        }
        w.click("remote-files-tab", cx);
        assert!(f.shell.read(cx).remote.focus_handle(cx).is_focused(w));
        w.click("local-files-tab", cx);
        assert!(f.shell.read(cx).browser.focus_handle(cx).is_focused(w));
        assert_eq!(f.shell.read(cx).browser.read(cx).selected(), Some("folder"));
    })
    .unwrap();
}

#[gpui_kit::test]
async fn both_preview_tabs_reject_selected_real_directories_then_enable_for_regular_files(
    cx: &mut TestAppContext,
) {
    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("up", cx);
        w.click("remote-up", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).browser.read(cx).is_loading()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        for (files, preview) in [
            ("local-files-tab", "local-preview-tab"),
            ("remote-files-tab", "remote-preview-tab"),
        ] {
            w.click(files, cx);
            w.press("home", cx);
            w.click(preview, cx);
            assert_ne!(w.find(preview).focused(), Some(true));
        }
        let s = f.shell.read(cx);
        assert_eq!(
            (s.local_view, s.remote_view),
            (PaneView::Files, PaneView::Files)
        );
        assert!(!s.browser.read(cx).can_preview());
        assert!(!s.remote.read(cx).can_preview());
        assert!(
            !s.local_preview
                .read(cx)
                .can_show(s.browser.read(cx).id().project)
        );
        assert!(
            !s.remote_preview
                .read(cx)
                .can_show(s.browser.read(cx).id().project)
        );
        w.within("local-files").click(0usize, cx);
        w.press("alt-enter", cx);
        w.within("remote-files").click(0usize, cx);
        w.press("alt-enter", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).browser.read(cx).is_loading()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("local-files-tab", cx);
        w.press("home", cx);
        w.click("remote-files-tab", cx);
        w.press("home", cx);
    })
    .unwrap();
    p.previews(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        copy(w, "local-workspace", "LOCAL a\nUnicode Ω", cx);
        copy(w, "remote-workspace", "REMOTE a\nUnicode ä", cx);
    })
    .unwrap();
    p.shutdown(cx).await;
}

#[gpui_kit::test]
async fn independent_native_previews_keep_entities_documents_directories_marks_and_connection_on_tab_return(
    cx: &mut TestAppContext,
) {
    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    p.previews(cx).await;
    let original = cx
        .update_window(f.handle, |_, w, cx| {
            let s = f.shell.read(cx);
            let local = s.preview_pane(PaneSide::Local);
            let remote = s.preview_pane(PaneSide::Remote);
            assert_ne!(local, remote);
            local.update(cx, |p, cx| p.focus(w, cx));
            let local_editor = w.focused(cx).unwrap();
            remote.update(cx, |p, cx| p.focus(w, cx));
            let remote_editor = w.focused(cx).unwrap();
            assert_ne!(local_editor, remote_editor);
            let s = f.shell.read(cx);
            (
                local.clone(),
                remote.clone(),
                local.read(cx).id(),
                remote.read(cx).id(),
                local_editor,
                remote_editor,
                s.browser.read(cx).id(),
                s.browser.read(cx).location().unwrap().directory.clone(),
                s.remote.read(cx).session_id(),
                s.remote.read(cx).connection(),
                s.remote.read(cx).directory().to_owned(),
                s.browser.read(cx).marked(),
                s.remote.read(cx).marked(),
            )
        })
        .unwrap();
    // A tab change must reveal the retained editor, not silently reload a changed file.
    fs::write(p.local.path().join("nested/a.txt"), "LOCAL changed on disk").unwrap();
    fs::write(
        p.server.dir.path().join("files/nested/a.txt"),
        "REMOTE changed on disk",
    )
    .unwrap();
    for (left, right) in [(false, true), (true, false), (false, false), (true, true)] {
        cx.update_window(f.handle, |_, w, cx| {
            w.click(
                if left {
                    "local-preview-tab"
                } else {
                    "local-files-tab"
                },
                cx,
            );
            w.click(
                if right {
                    "remote-preview-tab"
                } else {
                    "remote-files-tab"
                },
                cx,
            );
            w.render_frame(cx);
            let s = f.shell.read(cx);
            assert_eq!(
                s.local_view,
                if left {
                    PaneView::Preview
                } else {
                    PaneView::Files
                }
            );
            assert_eq!(
                s.remote_view,
                if right {
                    PaneView::Preview
                } else {
                    PaneView::Files
                }
            );
            assert_eq!(s.local_preview, original.0);
            assert_eq!(s.remote_preview, original.1);
            assert_eq!(s.local_preview.read(cx).id(), original.2);
            assert_eq!(s.remote_preview.read(cx).id(), original.3);
            assert!(!s.local_preview.read(cx).is_loading());
            assert!(!s.remote_preview.read(cx).is_loading());
            assert_eq!(s.browser.read(cx).id(), original.6);
            assert_eq!(s.browser.read(cx).location().unwrap().directory, original.7);
            assert_eq!(s.remote.read(cx).session_id(), original.8);
            assert_eq!(s.remote.read(cx).connection(), original.9);
            assert_eq!(s.remote.read(cx).directory(), original.10);
            assert_eq!(s.browser.read(cx).marked(), original.11);
            assert_eq!(s.remote.read(cx).marked(), original.12);
            for (scope, shown, child, absent) in [
                ("local-workspace", left, "browser-pane", "remote-pane"),
                ("remote-workspace", right, "remote-pane", "browser-pane"),
            ] {
                assert!(w.within(scope).try_find(absent).is_none());
                assert_eq!(w.within(scope).try_find("preview-header").is_some(), shown);
                assert_eq!(w.within(scope).try_find(child).is_some(), !shown);
            }
            if left {
                copy(w, "local-workspace", "LOCAL a\nUnicode Ω", cx);
                original.0.update(cx, |p, cx| p.focus(w, cx));
                assert_eq!(w.focused(cx), Some(original.4.clone()));
            }
            if right {
                copy(w, "remote-workspace", "REMOTE a\nUnicode ä", cx);
                original.1.update(cx, |p, cx| p.focus(w, cx));
                assert_eq!(w.focused(cx), Some(original.5.clone()));
            }
        })
        .unwrap();
    }
    p.shutdown(cx).await;
}

#[gpui_kit::test]
async fn changing_one_selected_file_preserves_the_opposite_document_and_cached_tabs_work_without_selection(
    cx: &mut TestAppContext,
) {
    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    p.previews(cx).await;
    let remote_id = f
        .shell
        .read_with(cx, |s, cx| s.remote_preview.read(cx).id());
    cx.update_window(f.handle, |_, w, cx| {
        w.click("local-files-tab", cx);
        w.within("local-files").click(1usize, cx);
        assert_eq!(f.shell.read(cx).local_view, PaneView::Files);
        assert_eq!(
            f.shell.read(cx).browser.read(cx).selected(),
            Some("nested/b.txt")
        );
        w.click("local-preview-tab", cx);
    })
    .unwrap();
    p.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(f.shell.read(cx).remote_view, PaneView::Preview);
        assert_eq!(f.shell.read(cx).remote_preview.read(cx).id(), remote_id);
        copy(w, "local-workspace", "LOCAL b", cx);
        copy(w, "remote-workspace", "REMOTE a\nUnicode ä", cx);
        w.click("remote-files-tab", cx);
        w.within("remote-files").click(1usize, cx);
        assert_eq!(f.shell.read(cx).remote_view, PaneView::Files);
        w.press("p", cx);
    })
    .unwrap();
    p.idle(cx).await;
    let ids = cx
        .update_window(f.handle, |_, w, cx| {
            copy(w, "local-workspace", "LOCAL b", cx);
            copy(w, "remote-workspace", "REMOTE b", cx);
            let s = f.shell.read(cx);
            let ids = (
                s.local_preview.read(cx).id(),
                s.remote_preview.read(cx).id(),
            );
            w.click("local-files-tab", cx);
            w.click("remote-files-tab", cx);
            w.click("filter", cx);
            w.input("no-such-local", cx);
            w.click("remote-filter", cx);
            w.input("no-such-remote", cx);
            ids
        })
        .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        let s = f.shell.read(cx);
        assert!(s.browser.read(cx).selected().is_none());
        assert!(s.remote.read(cx).selected().is_none());
        assert_ne!(w.find("local-preview-tab").disabled(), Some(true));
        assert_ne!(w.find("remote-preview-tab").disabled(), Some(true));
        w.click("local-preview-tab", cx);
        w.click("remote-preview-tab", cx);
        let s = f.shell.read(cx);
        assert_eq!(
            (
                s.local_preview.read(cx).id(),
                s.remote_preview.read(cx).id()
            ),
            ids
        );
        copy(w, "local-workspace", "LOCAL b", cx);
        copy(w, "remote-workspace", "REMOTE b", cx);
    })
    .unwrap();
    p.shutdown(cx).await;
}

#[gpui_kit::test]
async fn pointer_file_selection_never_reveals_or_requests_either_cached_preview(
    cx: &mut TestAppContext,
) {
    use std::{cell::Cell, rc::Rc};

    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    p.previews(cx).await;
    let original = f.shell.read_with(cx, |s, cx| {
        (
            s.local_preview.read(cx).id(),
            s.remote_preview.read(cx).id(),
            s.remote.read(cx).session_id(),
        )
    });
    let requests = Rc::new(Cell::new(0));
    let _subscriptions = cx.update(|cx| {
        let browser = f.shell.read(cx).browser.clone();
        let remote = f.shell.read(cx).remote.clone();
        let local_requests = requests.clone();
        let remote_requests = requests.clone();
        (
            cx.subscribe(&browser, move |_, event: &BrowserEvent, _| {
                if matches!(event, BrowserEvent::Selected { preview: true, .. }) {
                    local_requests.set(local_requests.get() + 1);
                }
            }),
            cx.subscribe(&remote, move |_, event: &RemoteEvent, _| {
                if matches!(event, RemoteEvent::Preview { .. }) {
                    remote_requests.set(remote_requests.get() + 1);
                }
            }),
        )
    });
    for (side, scope, own_files, other_files, own_preview, other_preview) in [
        (
            PaneSide::Local,
            "local-files",
            "local-files-tab",
            "remote-files-tab",
            "local-preview-tab",
            "remote-preview-tab",
        ),
        (
            PaneSide::Remote,
            "remote-files",
            "remote-files-tab",
            "local-files-tab",
            "remote-preview-tab",
            "local-preview-tab",
        ),
    ] {
        for opposite_preview in [true, false] {
            cx.update_window(f.handle, |_, w, cx| {
                w.click(own_files, cx);
                w.click(
                    if opposite_preview {
                        other_preview
                    } else {
                        other_files
                    },
                    cx,
                );
                for double in [false, true] {
                    w.within(scope).click(0usize, cx);
                    if double {
                        w.within(scope).double_click(1usize, cx);
                    } else {
                        w.within(scope).click(1usize, cx);
                    }
                    let s = f.shell.read(cx);
                    assert_eq!(s.pane_view(side), PaneView::Files);
                    let opposite = if side == PaneSide::Local {
                        PaneSide::Remote
                    } else {
                        PaneSide::Local
                    };
                    assert_eq!(
                        s.pane_view(opposite),
                        if opposite_preview {
                            PaneView::Preview
                        } else {
                            PaneView::Files
                        }
                    );
                    let selected = if side == PaneSide::Local {
                        s.browser.read(cx).selected()
                    } else {
                        s.remote.read(cx).selected()
                    };
                    assert!(selected.unwrap().ends_with("nested/b.txt"));
                    assert_eq!(s.local_preview.read(cx).id(), original.0);
                    assert_eq!(s.remote_preview.read(cx).id(), original.1);
                    assert_eq!(s.remote.read(cx).session_id(), original.2);
                    assert!(!s.local_preview.read(cx).is_loading());
                    assert!(!s.remote_preview.read(cx).is_loading());
                }
            })
            .unwrap();
            p.idle(cx).await;
            assert_eq!(
                requests.get(),
                0,
                "pointer selection emitted a preview request"
            );
        }
        // Return to the cached file before using the explicit tabs again.
        cx.update_window(f.handle, |_, w, cx| {
            w.within(scope).click(0usize, cx);
            w.click(own_preview, cx);
            w.click(other_preview, cx);
            copy(w, "local-workspace", "LOCAL a\nUnicode Ω", cx);
            copy(w, "remote-workspace", "REMOTE a\nUnicode ä", cx);
        })
        .unwrap();
        assert_eq!(
            requests.get(),
            0,
            "cached tabs unnecessarily reloaded a document"
        );
    }
    p.shutdown(cx).await;
}

#[gpui_kit::test]
async fn pointer_directory_selection_and_toggles_keep_both_files_tabs_and_emit_no_previews(
    cx: &mut TestAppContext,
) {
    use std::{cell::Cell, rc::Rc};

    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("up", cx);
        w.click("remote-up", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).browser.read(cx).is_loading()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    let original = f.shell.read_with(cx, |s, cx| {
        (
            s.local_preview.read(cx).id(),
            s.remote_preview.read(cx).id(),
            s.remote.read(cx).session_id(),
        )
    });
    let requests = Rc::new(Cell::new(0));
    let _subscriptions = cx.update(|cx| {
        let browser = f.shell.read(cx).browser.clone();
        let remote = f.shell.read(cx).remote.clone();
        let local_requests = requests.clone();
        let remote_requests = requests.clone();
        (
            cx.subscribe(&browser, move |_, event: &BrowserEvent, _| {
                if matches!(event, BrowserEvent::Selected { preview: true, .. }) {
                    local_requests.set(local_requests.get() + 1);
                }
            }),
            cx.subscribe(&remote, move |_, event: &RemoteEvent, _| {
                if matches!(event, RemoteEvent::Preview { .. }) {
                    remote_requests.set(remote_requests.get() + 1);
                }
            }),
        )
    });
    for scope in ["local-files", "remote-files"] {
        cx.update_window(f.handle, |_, w, cx| {
            w.within(scope).click(0usize, cx);
            let s = f.shell.read(cx);
            assert!(!s.browser.read(cx).is_loading());
            assert!(!s.remote.read(cx).is_loading());
            assert_eq!(
                s.browser.read(cx).location().unwrap().directory,
                p.local.path()
            );
            assert_eq!(
                s.remote.read(cx).directory(),
                p.server.dir.path().join("files").to_str().unwrap()
            );
            w.within(scope).double_click(0usize, cx);
        })
        .unwrap();
        cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
            !f.shell.read(cx).browser.read(cx).is_loading()
                && !f.shell.read(cx).remote.read(cx).is_loading()
        })
        .await;
        cx.update_window(f.handle, |_, w, cx| {
            // The expanded child is visible, but the pane's current directory is unchanged.
            assert!(w.within(scope).find(1usize).visible());
            w.within(scope).double_click(0usize, cx);
            let s = f.shell.read(cx);
            assert_eq!(
                (s.local_view, s.remote_view),
                (PaneView::Files, PaneView::Files)
            );
            assert_eq!(s.local_preview.read(cx).id(), original.0);
            assert_eq!(s.remote_preview.read(cx).id(), original.1);
            assert_eq!(s.remote.read(cx).session_id(), original.2);
            assert!(!s.local_preview.read(cx).is_loading());
            assert!(!s.remote_preview.read(cx).is_loading());
            assert_eq!(
                s.browser.read(cx).location().unwrap().directory,
                p.local.path()
            );
            assert_eq!(
                s.remote.read(cx).directory(),
                p.server.dir.path().join("files").to_str().unwrap()
            );
        })
        .unwrap();
    }
    assert_eq!(
        requests.get(),
        0,
        "directory pointers emitted a preview request"
    );
    p.shutdown(cx).await;
}

#[gpui_kit::test]
async fn files_tabs_do_not_cancel_either_queued_native_preview(cx: &mut TestAppContext) {
    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    let ids = cx
        .update_window(f.handle, |_, w, cx| {
            p.request_previews(w, cx);
            let s = f.shell.read(cx);
            let ids = (
                s.local_preview.read(cx).id(),
                s.remote_preview.read(cx).id(),
            );
            assert!(s.local_preview.read(cx).is_loading());
            assert!(s.remote_preview.read(cx).is_loading());
            w.click("local-files-tab", cx);
            w.click("remote-files-tab", cx);
            let s = f.shell.read(cx);
            assert!(s.local_preview.read(cx).is_loading());
            assert!(s.remote_preview.read(cx).is_loading());
            assert_eq!(
                (
                    s.local_preview.read(cx).id(),
                    s.remote_preview.read(cx).id()
                ),
                ids
            );
            ids
        })
        .unwrap();
    p.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let s = f.shell.read(cx);
        assert_eq!(
            (s.local_view, s.remote_view),
            (PaneView::Files, PaneView::Files)
        );
        assert_eq!(
            (
                s.local_preview.read(cx).id(),
                s.remote_preview.read(cx).id()
            ),
            ids
        );
        w.click("local-preview-tab", cx);
        w.click("remote-preview-tab", cx);
        copy(w, "local-workspace", "LOCAL a\nUnicode Ω", cx);
        copy(w, "remote-workspace", "REMOTE a\nUnicode ä", cx);
    })
    .unwrap();
    p.shutdown(cx).await;
}

#[gpui_kit::test]
async fn scoped_cancel_buttons_never_cancel_the_opposite_queued_preview(cx: &mut TestAppContext) {
    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    for local in [true, false] {
        cx.update_window(f.handle, |_, w, cx| {
            p.request_previews(w, cx);
            let s = f.shell.read(cx);
            let left = s.local_preview.clone();
            let right = s.remote_preview.clone();
            assert!(left.read(cx).is_loading());
            assert!(right.read(cx).is_loading());
            let opposite_id = if local {
                right.read(cx).id()
            } else {
                left.read(cx).id()
            };
            w.click(if local { "cancel" } else { "remote-cancel" }, cx);
            assert_eq!(left.read(cx).is_loading(), !local);
            assert_eq!(right.read(cx).is_loading(), local);
            assert_eq!(
                if local {
                    right.read(cx).id()
                } else {
                    left.read(cx).id()
                },
                opposite_id
            );
        })
        .unwrap();
        p.idle(cx).await;
        cx.update_window(f.handle, |_, w, cx| {
            let side = if local {
                PaneSide::Remote
            } else {
                PaneSide::Local
            };
            f.shell
                .update(cx, |s, cx| s.set_pane_view(side, PaneView::Preview, w, cx));
            copy(
                w,
                if local {
                    "remote-workspace"
                } else {
                    "local-workspace"
                },
                if local {
                    "REMOTE a\nUnicode ä"
                } else {
                    "LOCAL a\nUnicode Ω"
                },
                cx,
            );
        })
        .unwrap();
    }
    p.shutdown(cx).await;
}

#[gpui_kit::test]
async fn global_active_work_cancel_stops_both_real_previews(cx: &mut TestAppContext) {
    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    cx.update_window(f.handle, |_, w, cx| {
        p.request_previews(w, cx);
        w.press("ctrl-escape", cx);
        let s = f.shell.read(cx);
        assert!(!s.local_preview.read(cx).is_loading());
        assert!(!s.remote_preview.read(cx).is_loading());
        assert_eq!(
            (s.local_view, s.remote_view),
            (PaneView::Files, PaneView::Files)
        );
    })
    .unwrap();
    p.idle(cx).await;
    p.shutdown(cx).await;
}

#[gpui_kit::test]
async fn preview_shortcuts_help_and_settings_target_the_visible_focused_side(
    cx: &mut TestAppContext,
) {
    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    p.previews(cx).await;
    for (side, own, other, text) in [
        (
            PaneSide::Local,
            "local-workspace",
            "remote-workspace",
            "LOCAL a\nUnicode Ω",
        ),
        (
            PaneSide::Remote,
            "remote-workspace",
            "local-workspace",
            "REMOTE a\nUnicode ä",
        ),
    ] {
        cx.update_window(f.handle, |_, w, cx| {
            let preview = f.shell.read(cx).preview_pane(side);
            preview.update(cx, |p, cx| p.focus(w, cx));
            let editor = w.focused(cx).unwrap();
            assert_eq!(f.shell.read(cx).focused_side(w, cx), side);
            w.press("f1", cx);
            w.press("escape", cx);
            assert!(editor.is_focused(w));
            w.click("settings", cx);
            toolbar_scopes::assert_browser_scopes_absent(w);
            w.click("settings-close", cx);
            assert!(editor.is_focused(w));
            assert!(w.within(own).find("preview-header").visible());
            assert!(w.within(other).find("preview-header").visible());
            w.press("c", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(text)
            );
            w.press("p", cx);
            assert_eq!(f.shell.read(cx).pane_view(side), PaneView::Files);
            assert!(w.within(other).find("preview-header").visible());
            w.press("p", cx);
            w.press("ctrl-alt-p", cx);
            assert!(editor.is_focused(w));
            w.press("escape", cx);
        })
        .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            assert_eq!(f.shell.read(cx).pane_view(side), PaneView::Files);
            assert!(w.within(other).find("preview-header").visible());
            f.shell
                .update(cx, |s, cx| s.set_pane_view(side, PaneView::Preview, w, cx));
        })
        .unwrap();
    }
    p.shutdown(cx).await;
}

#[gpui_kit::test]
async fn terminal_remote_read_failure_clears_only_the_right_document_and_keeps_the_local_editor(
    cx: &mut TestAppContext,
) {
    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    cx.update_window(f.handle, |_, w, cx| w.click("local-preview-tab", cx))
        .unwrap();
    p.idle(cx).await;
    let (local, id, editor) = cx
        .update_window(f.handle, |_, w, cx| {
            let local = f.shell.read(cx).local_preview.clone();
            local.update(cx, |p, cx| p.focus(w, cx));
            let id = local.read(cx).id();
            (local, id, w.focused(cx).unwrap())
        })
        .unwrap();
    fs::write(p.server.dir.path().join("arm-empty-read"), "armed").unwrap();
    cx.update_window(f.handle, |_, w, cx| w.click("remote-preview-tab", cx))
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).remote.read(cx).has_session()
            && !f.shell.read(cx).remote_preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        let s = f.shell.read(cx);
        assert_eq!(s.local_view, PaneView::Preview);
        assert_eq!(s.remote_view, PaneView::Files);
        assert_eq!(s.local_preview, local);
        assert_eq!(local.read(cx).id(), id);
        assert!(
            !s.remote_preview
                .read(cx)
                .can_show(s.browser.read(cx).id().project)
        );
        assert!(w.within("remote-workspace").find("remote-pane").visible());
        assert!(
            w.within("remote-workspace")
                .try_find("preview-header")
                .is_none()
        );
        copy(w, "local-workspace", "LOCAL a\nUnicode Ω", cx);
        local.update(cx, |p, cx| p.focus(w, cx));
        assert!(editor.is_focused(w));
    })
    .unwrap();
}

#[gpui_kit::test]
async fn project_switch_invalidates_both_pending_previews_and_rejects_their_stale_callbacks(
    cx: &mut TestAppContext,
) {
    let p = ConnectedPanes::new(cx).await;
    let f = &p.f;
    let other = tempfile::tempdir().unwrap();
    fs::write(other.path().join("new.txt"), "NEW project").unwrap();
    let stale = cx
        .update_window(f.handle, |_, w, cx| {
            p.request_previews(w, cx);
            let s = f.shell.read(cx);
            assert!(s.local_preview.read(cx).is_loading());
            assert!(s.remote_preview.read(cx).is_loading());
            let stale = [
                (s.local_preview.clone(), s.local_preview.read(cx).id()),
                (s.remote_preview.clone(), s.remote_preview.read(cx).id()),
            ];
            f.shell
                .update(cx, |s, cx| s.open_project(other.path().into(), w, cx));
            stale
        })
        .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell
            .read(cx)
            .browser
            .read(cx)
            .location()
            .is_some_and(|l| l.directory == other.path())
            && !f.shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    p.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let project = f.shell.read(cx).browser.read(cx).id().project;
        for (preview, id) in stale {
            assert_ne!(project, id.project);
            assert!(!preview.read(cx).can_show(project));
            preview.update(cx, |_, cx| cx.emit(PreviewEvent::Loaded(id)));
        }
        w.render_frame(cx);
        let s = f.shell.read(cx);
        assert_eq!(
            (s.local_view, s.remote_view),
            (PaneView::Files, PaneView::Files)
        );
        assert!(!s.remote.read(cx).has_session());
        assert!(s.browser.read(cx).marked().is_empty());
        assert!(s.remote.read(cx).marked().is_empty());
        for scope in ["local-workspace", "remote-workspace"] {
            assert!(w.within(scope).try_find("preview-header").is_none());
        }
        w.click("remote-preview-tab", cx);
        assert_eq!(f.shell.read(cx).remote_view, PaneView::Files);
        assert_ne!(w.find("remote-preview-tab").focused(), Some(true));
    })
    .unwrap();
}
