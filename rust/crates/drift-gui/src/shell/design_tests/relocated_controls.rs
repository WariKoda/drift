use super::*;
use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent};

fn sftp_store(server: &support::Server, config: &Path) -> Store {
    let store = Store::new(config.into());
    store.save_host(None, None, server.host()).unwrap();
    store
}

async fn connect(f: &Fixture, cx: &mut TestAppContext) {
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
}

async fn shutdown(f: &Fixture, cx: &mut TestAppContext) {
    let connection = f.shell.read_with(cx, |shell, cx| {
        shell.remote.read(cx).comparison_target().unwrap().1
    });
    connection.shutdown().await;
}

async fn hosts_ready(f: &Fixture, cx: &mut TestAppContext) {
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell
            .read(cx)
            .hosts
            .as_ref()
            .is_some_and(|hosts| hosts.read(cx).can_leave())
    })
    .await;
}

fn resize(w: &mut Window, width: f32, height: f32, font: f32, cx: &mut gpui_kit::App) {
    w.resize(size(px(width), px(height)));
    w.bounds_changed(cx);
    Theme::update(cx, |theme| theme.font_size = px(font));
    crate::test_support::settle(w, cx);
}

fn press_then_release(
    w: &mut Window,
    position: gpui_kit::Point<gpui_kit::Pixels>,
    between: impl FnOnce(&mut Window, &mut gpui_kit::App),
    cx: &mut gpui_kit::App,
) {
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
    // No repaint: the release still targets the frame that received the press.
    between(w, cx);
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

#[gpui_kit::test]
async fn global_hosts_toggles_the_manager_and_stays_inert_while_listing_or_covered(
    cx: &mut TestAppContext,
) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("keep.txt"), "unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.register("Review", local.path().into()).unwrap();
    let f = Fixture::new(1200., 900., store.clone(), local.path(), None, cx).await;
    let browser_id = cx
        .update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            let [projects, hosts, folder] = ["projects", "hosts", "open-folder"]
                .map(|id| w.within("workspace-toolbar").find(id).bounds());
            assert!(projects.right() <= hosts.left() && hosts.right() <= folder.left());
            assert_eq!(projects.top(), hosts.top());
            assert_eq!(hosts.top(), folder.top());
            assert!(
                hosts.size.width > metric(token::CONTROL, cx),
                "Hosts shows its label"
            );
            let button = w.within("workspace-toolbar").find("hosts");
            assert_eq!(button.role(), Some(gpui_kit::Role::Button));
            assert_eq!(button.label(), Some("Hosts"));
            assert!(!f.shell.read(cx).toolbar_state(cx).hosts_open);
            for pane in ["local-workspace", "remote-workspace", "workspace-footer"] {
                assert!(
                    w.within(pane).try_find("hosts").is_none(),
                    "hosts in {pane}"
                );
            }
            let browser_id = f.shell.read(cx).browser.read(cx).id();
            w.click("hosts", cx);
            assert!(f.shell.read(cx).hosts.is_some());
            browser_id
        })
        .unwrap();
    hosts_ready(&f, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        // `hosts_open` drives the button's selected presentation.
        assert!(f.shell.read(cx).toolbar_state(cx).hosts_open);
        toolbar_scopes::assert_browser_scopes_absent(w);
        // The same control closes the open manager and returns to the browser.
        w.click("hosts", cx);
        assert!(f.shell.read(cx).hosts.is_none());
        assert!(f.shell.read(cx).browser_screen_active(cx));
        assert!(f.shell.read(cx).browser.focus_handle(cx).is_focused(w));
        assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
        w.render_frame(cx);
        assert!(!f.shell.read(cx).toolbar_state(cx).hosts_open);
        toolbar_scopes::assert_workspace_toolbar_scopes(w);
        // Settings over the browser hands over to the manager.
        w.click("settings", cx);
        assert!(f.shell.read(cx).settings.is_some());
        w.render_frame(cx);
        assert!(!f.shell.read(cx).toolbar_state(cx).hosts_open);
        w.click("hosts", cx);
        assert!(f.shell.read(cx).settings.is_none());
        assert!(f.shell.read(cx).hosts.is_some());
    })
    .unwrap();
    hosts_ready(&f, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("hosts", cx);
        assert!(f.shell.read(cx).hosts.is_none());
        let browser = f.shell.read(cx).browser.clone();
        browser.update(cx, |browser, cx| browser.refresh(&Refresh, w, cx));
        assert!(browser.read(cx).is_loading());
        w.render_frame(cx);
        w.click("hosts", cx);
        f.shell.update(cx, |shell, cx| {
            shell.toolbar_event(&ToolbarEvent::Hosts, w, cx)
        });
        assert!(f.shell.read(cx).hosts.is_none());
        assert!(f.shell.read(cx).browser_screen_active(cx));
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.press("f1", cx);
        assert!(f.shell.read(cx).help.is_some());
        w.render_frame(cx);
        w.click("hosts", cx);
        f.shell.update(cx, |shell, cx| {
            shell.toolbar_event(&ToolbarEvent::Hosts, w, cx)
        });
        assert!(f.shell.read(cx).hosts.is_none());
        assert!(f.shell.read(cx).help.is_some());
        w.press("escape", cx);
        assert!(f.shell.read(cx).help.is_none());
        w.click("projects", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).projects.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("project-new", cx);
        w.input("Draft", cx);
        w.render_frame(cx);
        w.click("hosts", cx);
        f.shell.update(cx, |shell, cx| {
            shell.toolbar_event(&ToolbarEvent::Hosts, w, cx)
        });
        assert!(f.shell.read(cx).hosts.is_none());
        assert_eq!(w.find("project-edit-name").value(), Some("Draft"));
        w.click("project-close", cx);
        w.click("home", cx);
        assert!(f.shell.read(cx).browser_screen_active(cx));
    })
    .unwrap();
    assert_eq!(store.registry().unwrap().projects.len(), 1);
    assert_eq!(
        fs::read_to_string(local.path().join("keep.txt")).unwrap(),
        "unchanged"
    );
}

#[gpui_kit::test]
async fn global_hosts_is_disabled_without_a_project_location(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
            cx.new(|cx| {
                let service = BrowserService::new().unwrap();
                Shell::new(
                    w,
                    cx,
                    Store::new(config.path().into()),
                    service.clone(),
                    RemoteService::new(service),
                    missing.clone(),
                )
            })
        })
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        !shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert!(shell.read(cx).browser.read(cx).location().is_none());
        assert!(shell.read(cx).browser_screen_active(cx));
        w.render_frame(cx);
        w.click("hosts", cx);
        w.press("shift-h", cx);
        shell.update(cx, |shell, cx| {
            shell.toolbar_event(&ToolbarEvent::Hosts, w, cx)
        });
        assert!(shell.read(cx).hosts.is_none());
        assert!(shell.read(cx).browser_screen_active(cx));
    })
    .unwrap();
    assert!(!missing.exists());
}

#[gpui_kit::test]
async fn hosts_from_settings_opened_over_the_manager_returns_to_it(cx: &mut TestAppContext) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.register("Review", local.path().into()).unwrap();
    let f = Fixture::new(1200., 900., store, local.path(), None, cx).await;
    cx.update_window(f.handle, |_, w, cx| w.click("hosts", cx))
        .unwrap();
    hosts_ready(&f, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let manager = f.shell.read(cx).hosts.clone();
        w.click("settings", cx);
        assert!(f.shell.read(cx).settings.is_some());
        w.render_frame(cx);
        // Settings is the visible screen, so Hosts offers to show the manager again.
        assert!(!f.shell.read(cx).toolbar_state(cx).hosts_open);
        w.click("hosts", cx);
        assert!(
            f.shell.read(cx).settings.is_none(),
            "Hosts must close Settings (manager still open underneath: {})",
            f.shell.read(cx).hosts.is_some()
        );
        assert_eq!(
            f.shell.read(cx).hosts,
            manager,
            "Hosts from Settings must show the manager, not discard it behind Settings"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
async fn compare_footer_centres_wraps_and_rejects_stale_or_covered_frames_with_real_sftp(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("sample.txt"), "local contents").unwrap();
    let remote_file = server.dir.path().join("files/sample.txt");
    fs::write(&remote_file, "remote contents").unwrap();
    let store = sftp_store(&server, config.path());
    let f = Fixture::new(
        1400.,
        1000.,
        store,
        local.path(),
        Some(server.options()),
        cx,
    )
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        let footer = w.find("workspace-footer").bounds();
        assert!(footer.top() >= w.find("browser-split").bounds().bottom());
        assert!(
            w.within("workspace-footer")
                .find("workspace-status")
                .visible()
        );
        for (id, label) in COMPARE_CONTROLS {
            let item = w.within("workspace-compare").find(id);
            assert_eq!(item.label(), Some(label));
            // Without a session every scope is inert.
            w.click(id, cx);
        }
        assert!(!f.shell.read(cx).comparison.read(cx).visible());
        for pane in ["local-workspace", "remote-workspace", "workspace-toolbar"] {
            for (id, _) in COMPARE_CONTROLS {
                assert!(w.within(pane).try_find(id).is_none(), "{id} in {pane}");
            }
        }
    })
    .unwrap();
    connect(&f, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        f.shell.update(cx, |shell, _| shell.status = "Ready".into());
        for (width, height, font) in [(1400., 1000., 16.), (1400., 1000., 24.), (1000., 900., 16.)]
        {
            resize(w, width, height, font, cx);
            let footer = w.find("workspace-footer").bounds();
            let group = w
                .within("workspace-footer")
                .find("workspace-compare")
                .bounds();
            let status = w
                .within("workspace-footer")
                .find("workspace-status")
                .bounds();
            assert!(
                (group.center().x - footer.center().x).abs() <= px(1.),
                "compare must be centred at {width}/{font}: group={group:?}, footer={footer:?}"
            );
            assert!(status.left() >= footer.left() && status.right() <= group.left());
            assert!(footer.left() >= px(0.) && footer.right() <= px(width));
            assert!(footer.bottom() <= px(height));
            for (id, label) in COMPARE_CONTROLS {
                let item = w.within("workspace-compare").find(id);
                assert_eq!(item.label(), Some(label));
                let bounds = item.bounds();
                assert!(
                    bounds.left() >= group.left() && bounds.right() <= group.right(),
                    "{id}: {bounds:?}"
                );
                assert_eq!(bounds.size.height, metric(token::CONTROL, cx), "{id}");
                assert!(
                    bounds.size.width > metric(token::CONTROL, cx),
                    "{id} is labelled"
                );
            }
        }
        for (width, font) in [(640., 24.), (420., 16.), (420., 24.)] {
            resize(w, width, 1000., font, cx);
            let footer = w.find("workspace-footer").bounds();
            assert!(
                footer.left() >= px(0.) && footer.right() <= px(width),
                "footer at {width}/{font}: {footer:?}"
            );
            assert!(footer.bottom() <= px(1000.));
            assert!(footer.top() >= w.find("browser-split").bounds().bottom());
            assert!(w.find("browser-split").bounds().size.height > px(0.));
            let group = w
                .within("workspace-footer")
                .find("workspace-compare")
                .bounds();
            assert!(group.left() >= footer.left() && group.right() <= footer.right());
            for (id, _) in COMPARE_CONTROLS {
                let item = w.within("workspace-compare").find(id);
                let bounds = item.bounds();
                assert!(item.visible(), "{id} at {width}/{font}");
                assert!(
                    bounds.left() >= footer.left() && bounds.right() <= footer.right(),
                    "{id} at {width}/{font}: {bounds:?} outside {footer:?}"
                );
                assert!(bounds.top() >= footer.top() && bounds.bottom() <= footer.bottom());
            }
        }
        resize(w, 1400., 1000., 16., cx);
    })
    .unwrap();
    // A press on a visible compare button released after the browser was covered, or after a
    // covered round trip, belongs to an old frame.
    for (settings, round_trip) in [(true, false), (true, true), (false, false), (false, true)] {
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            let position = w
                .within("workspace-compare")
                .find("compare-project")
                .bounds()
                .center();
            press_then_release(
                w,
                position,
                |w, cx| {
                    f.shell.update(cx, |shell, cx| {
                        let event = if settings {
                            ToolbarEvent::Settings
                        } else {
                            ToolbarEvent::Projects
                        };
                        shell.toolbar_event(&event, w, cx);
                        if round_trip {
                            if settings {
                                shell.toolbar_event(&event, w, cx);
                            } else {
                                shell.projects.update(cx, |projects, cx| {
                                    projects.cancel_view(&Cancel, w, cx)
                                });
                            }
                        }
                    });
                },
                cx,
            );
            assert!(
                !f.shell.read(cx).comparison.read(cx).visible(),
                "stale compare: settings={settings}, round_trip={round_trip}"
            );
            if round_trip {
                assert!(f.shell.read(cx).browser_screen_active(cx));
            } else {
                w.render_frame(cx);
                toolbar_scopes::assert_browser_scopes_absent(w);
            }
        })
        .unwrap();
        cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
            !f.shell.read(cx).projects.read(cx).is_loading()
        })
        .await;
        cx.update_window(f.handle, |_, w, cx| {
            if !round_trip {
                w.click("home", cx);
            }
            assert!(f.shell.read(cx).browser_screen_active(cx));
        })
        .unwrap();
    }
    cx.update_window(f.handle, |_, w, cx| w.click("hosts", cx))
        .unwrap();
    hosts_ready(&f, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        toolbar_scopes::assert_browser_scopes_absent(w);
        w.click("home", cx);
        w.render_frame(cx);
        toolbar_scopes::assert_workspace_toolbar_scopes(w);
        w.click("compare-project", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).comparison.read(cx).visible()
            && !f.shell.read(cx).comparison.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        // The comparison keeps its own status row and has no compare scopes.
        assert!(w.try_find("workspace-footer").is_none());
        assert!(w.try_find("workspace-compare").is_none());
        for (id, _) in COMPARE_CONTROLS {
            assert!(w.try_find(id).is_none(), "{id} over the comparison");
        }
        w.click("home", cx);
        assert!(f.shell.read(cx).browser_screen_active(cx));
        w.render_frame(cx);
        assert!(w.find("workspace-footer").visible());
    })
    .unwrap();
    shutdown(&f, cx).await;
    assert_eq!(
        fs::read_to_string(local.path().join("sample.txt")).unwrap(),
        "local contents"
    );
    assert_eq!(fs::read_to_string(remote_file).unwrap(), "remote contents");
}

#[gpui_kit::test]
async fn remote_connection_row_fits_narrow_panes_with_real_sftp(cx: &mut TestAppContext) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("local.txt"), "local unchanged").unwrap();
    let store = sftp_store(&server, config.path());
    let f = Fixture::new(
        1400.,
        1000.,
        store,
        local.path(),
        Some(server.options()),
        cx,
    )
    .await;
    connect(&f, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let session = f.shell.read(cx).remote.read(cx).session_id();
        let mut outside = vec![];
        // Window widths that give the remote pane 210, 420 and 700 pixels at the default split.
        for (width, font) in [
            (420., 16.),
            (420., 24.),
            (840., 16.),
            (840., 24.),
            (1400., 16.),
            (1400., 24.),
        ] {
            resize(w, width, 1200., font, cx);
            let pane = w.within("browser-split").find("split-second").bounds();
            assert!((pane.size.width - px(width / 2.)).abs() <= px(1.));
            let workspace = w.find("remote-workspace").bounds();
            let tabs = w
                .within("remote-workspace")
                .find("remote-pane-tabs")
                .bounds();
            let header = metric(token::HEADER, cx);
            // Narrow panes wrap the picker below the tabs; the tabs scroll on their line.
            assert!(
                tabs.size.height >= header && tabs.size.height <= header * 2.,
                "remote tabs at {width}/{font}: {tabs:?}"
            );
            if width >= 1400. || (width >= 840. && font == 16.) {
                assert_eq!(tabs.size.height, header, "{width}/{font} keeps one row");
            }
            workspace_headers::assert_aligned_headers(w, cx);
            let select = w
                .within("remote-connection-controls")
                .find("remote-host-select");
            assert_eq!(select.role(), Some(gpui_kit::Role::ComboBox));
            assert_eq!(select.label(), Some("Remote host"));
            let disconnect = w.within("remote-connection-controls").find("disconnect");
            assert_eq!(disconnect.label(), Some("Disconnect"));
            assert!(select.bounds().right() <= disconnect.bounds().left());
            for item in [&select, &disconnect] {
                let bounds = item.bounds();
                assert!(
                    bounds.top() >= tabs.top() && bounds.bottom() <= tabs.bottom(),
                    "{:?} clipped vertically at {width}/{font}: {bounds:?} in {tabs:?}",
                    item.label()
                );
                if !(bounds.left() >= workspace.left() && bounds.right() <= workspace.right()) {
                    outside.push(format!(
                        "{:?} at {width}px/{font}px: {bounds:?} outside remote pane {workspace:?}",
                        item.label()
                    ));
                }
            }
            assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), session);
        }
        assert!(outside.is_empty(), "{}", outside.join("\n"));
    })
    .unwrap();
    shutdown(&f, cx).await;
    assert_eq!(
        fs::read_to_string(local.path().join("local.txt")).unwrap(),
        "local unchanged"
    );
}

#[gpui_kit::test]
async fn finder_head_keeps_paired_rows_aligned_with_real_sftp(cx: &mut TestAppContext) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("alpha.txt"), "alpha").unwrap();
    fs::write(local.path().join("beta.txt"), "beta").unwrap();
    let remote_file = server.dir.path().join("files/remote.txt");
    fs::write(&remote_file, "remote unchanged").unwrap();
    let store = sftp_store(&server, config.path());
    let f = Fixture::new(
        1400.,
        1100.,
        store,
        local.path(),
        Some(server.options()),
        cx,
    )
    .await;
    connect(&f, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(
            w.try_find("local-actions-header").is_none(),
            "Files has no extra row"
        );
        w.click("find", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).browser.read(cx).finder_active()
            && !f.shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    for remote_preview in [false, true] {
        if remote_preview {
            cx.update_window(f.handle, |_, w, cx| {
                f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
                w.press("home", cx);
                w.click("remote-preview-tab", cx);
            })
            .unwrap();
            cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
                !f.shell.read(cx).remote_preview.read(cx).is_loading()
            })
            .await;
        }
        cx.update_window(f.handle, |_, w, cx| {
            for (width, height, font) in [
                (1400., 1100., 16.),
                (1400., 1100., 24.),
                (840., 1200., 16.),
                (420., 1600., 24.),
                (1400., 1100., 16.),
            ] {
                resize(w, width, height, font, cx);
                assert!(f.shell.read(cx).browser.read(cx).finder_active());
                assert_eq!(
                    f.shell.read(cx).remote_view == PaneView::Preview,
                    remote_preview
                );
                let actions = w
                    .within("local-workspace")
                    .find("local-actions-header")
                    .bounds();
                assert!(
                    w.within("local-actions-header")
                        .find("finder-return")
                        .visible()
                );
                assert_eq!(
                    actions.top(),
                    w.find("local-aligned-filter").bounds().bottom(),
                    "Finder's row follows the filter"
                );
                workspace_headers::assert_aligned_headers(w, cx);
            }
        })
        .unwrap();
    }
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote-files-tab", cx);
        w.click("finder-return", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        !f.shell.read(cx).browser.read(cx).finder_active()
            && !f.shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        crate::test_support::settle(w, cx);
        assert!(w.try_find("local-actions-header").is_none());
        workspace_headers::assert_aligned_headers(w, cx);
    })
    .unwrap();
    shutdown(&f, cx).await;
    assert_eq!(fs::read_to_string(remote_file).unwrap(), "remote unchanged");
    assert_eq!(
        fs::read_to_string(local.path().join("alpha.txt")).unwrap(),
        "alpha"
    );
}
