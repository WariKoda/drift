use super::*;
use crate::{browser_menu::MenuAction, sftp_test_support as support};
use drift_core::{
    config::RuntimeConfig, local::ProjectRoot, pathmap::Mapping, remote::ConnectOptions,
};
use gpui_kit::InputEvent as _;
use gpui_kit::test::{ElementSnapshot, TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, ScrollDelta, TestAppContext, WindowBounds, WindowOptions, size,
};
use std::{fs, sync::Arc, time::Duration};

struct MenuWindow {
    pane: Entity<RemotePane>,
    events: Vec<String>,
    _subscription: Subscription,
}
impl Render for MenuWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Drift")
            .size_full()
            .flex()
            .child(self.pane.clone())
    }
}

fn open(
    cx: &mut TestAppContext,
    host: Host,
    options: ConnectOptions,
    location: Option<Location>,
) -> (AnyWindowHandle, Entity<MenuWindow>, Entity<RemotePane>) {
    cx.executor().allow_parking();
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1000.), px(1000.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx: &mut Context<MenuWindow>| {
                    let pane = cx.new(|cx| {
                        RemotePane::new(
                            RemoteService::with_options(
                                drift_app::browser::BrowserService::new().unwrap(),
                                options,
                            ),
                            window,
                            cx,
                        )
                    });
                    pane.update(cx, |pane, cx| {
                        pane.set_context(7, vec![host], cx);
                        pane.set_selection_location(location, window, cx);
                        pane.set_comparison_ready(true, window, cx);
                    });
                    let subscription = cx.subscribe(&pane, |this, _, event, _| {
                        let event = match event {
                            RemoteEvent::SelectionChanged { .. } => "selection".into(),
                            RemoteEvent::Preview { path, .. } => format!("preview:{path}"),
                            RemoteEvent::Compare {
                                project,
                                connection,
                                path,
                            } => {
                                format!("compare:{project}:{connection}:{path}")
                            }
                            RemoteEvent::Toolbar {
                                project,
                                connection,
                                event,
                            } => {
                                let scope = match event {
                                    crate::toolbar::ToolbarEvent::CompareMarked => "marked",
                                    crate::toolbar::ToolbarEvent::CompareProject => "project",
                                    _ => panic!("unexpected menu toolbar event"),
                                };
                                format!("toolbar:{project}:{connection}:{scope}")
                            }
                            _ => return,
                        };
                        this.events.push(event);
                    });
                    MenuWindow {
                        pane,
                        events: vec![],
                        _subscription: subscription,
                    }
                })
            },
        )
        .unwrap()
    });
    let pane = view.read_with(cx, |view, _| view.pane.clone());
    (handle, view, pane)
}

async fn connected(cx: &mut TestAppContext, handle: AnyWindowHandle, pane: &Entity<RemotePane>) {
    cx.update_window(handle, |_, w, cx| w.click(("connect-host", 0usize), cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    assert!(pane.read_with(cx, |pane, _| pane.has_session()));
}

fn item(window: &mut Window, label: &str) -> (usize, ElementSnapshot) {
    let menu = window.within("popup-menu");
    (0..24)
        .find_map(|index| {
            let snapshot = menu.try_find(index)?;
            (snapshot.label() == Some(label)).then_some((index, snapshot))
        })
        .unwrap_or_else(|| panic!("missing menu item {label}"))
}

fn click_menu(window: &mut Window, label: &str, cx: &mut App) {
    let (index, snapshot) = item(window, label);
    if !snapshot.visible() {
        window.scroll(
            "popup-menu",
            ScrollDelta::Pixels(point(px(0.), px(-1000.))),
            cx,
        );
    }
    window.within("popup-menu").click(index, cx);
}

fn assert_disabled(window: &mut Window, label: &str, pane: &Entity<RemotePane>, cx: &mut App) {
    let id = pane.read(cx).menu.as_ref().unwrap().1.popup.entity_id();
    click_menu(window, label, cx);
    assert_eq!(
        pane.read(cx)
            .menu
            .as_ref()
            .map(|(_, menu)| menu.popup.entity_id()),
        Some(id),
        "disabled {label} must not invoke its callback"
    );
}

#[gpui_kit::test]
async fn right_click_keyboard_escape_and_focus_preserve_marks_range_and_preview(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let root = server.dir.path().join("files");
    fs::write(root.join("a.txt"), "a").unwrap();
    fs::write(root.join("b.txt"), "b").unwrap();
    std::os::unix::fs::symlink("a.txt", root.join("z-link")).unwrap();
    let (handle, view, pane) = open(cx, server.host(), server.options(), None);
    connected(cx, handle, &pane).await;
    cx.update_window(handle, |_, w, cx| {
        w.click(0usize, cx);
        pane.update(cx, |pane, _| {
            pane.files.toggle_mark();
            pane.files.visual_range();
        });
    })
    .unwrap();
    let events = view.read_with(cx, |view, _| view.events.clone());
    cx.update_window(handle, |_, w, cx| {
        w.right_click(1usize, cx);
        assert!(w.find("popup-menu").visible());
        let remote = pane.read(cx);
        assert_eq!(remote.selected(), root.join("b.txt").to_str());
        assert_eq!(remote.marked(), [root.join("a.txt").to_string_lossy()]);
        assert!(remote.files.range_active());
        assert!(
            remote
                .menu
                .as_ref()
                .unwrap()
                .1
                .popup
                .focus_handle(cx)
                .is_focused(w)
        );
        w.press("down", cx);
        assert_eq!(pane.read(cx).selected(), root.join("b.txt").to_str());
        w.press("shift-f10", cx);
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(pane.read(cx).menu.is_none());
        assert!(pane.read(cx).focus.is_focused(w));
        assert!(pane.read(cx).files.range_active());
        assert_eq!(pane.read(cx).marked().len(), 1);
        w.press("shift-f10", cx);
        assert!(w.find("popup-menu").visible());
        assert_eq!(
            w.find("popup-menu").bounds().origin,
            pane.read(cx).menu_anchor
        );
        w.press("escape", cx);
        w.press("menu", cx);
        assert!(w.find("popup-menu").visible());
        click_menu(w, "Mark", cx);
        assert_eq!(pane.read(cx).marked().len(), 2);
        assert!(pane.read(cx).files.range_active());
        w.press("shift-f10", cx);
        click_menu(w, "Unmark", cx);
        assert_eq!(pane.read(cx).marked().len(), 1);
        w.press("shift-f10", cx);
        click_menu(w, "Clear all marks", cx);
        assert!(pane.read(cx).marked().is_empty());
        assert!(!pane.read(cx).files.range_active());
        w.press("shift-f10", cx);
        w.click("remote-filter", cx);
        assert!(pane.read(cx).filter.focus_handle(cx).is_focused(w));
        pane.update(cx, |pane, cx| pane.focus.focus(w, cx));
        w.press("shift-f10", cx);
        pane.update(cx, |pane, cx| {
            pane.focus_filter(&FocusFilter, w, cx);
            pane.dismiss_context_menu(w, cx);
        });
        assert!(pane.read(cx).filter.focus_handle(cx).is_focused(w));
        w.press("shift-f10", cx);
        assert!(pane.read(cx).menu.is_none());
        w.right_click(2usize, cx);
        assert_disabled(w, "Preview", &pane, cx);
        w.press("escape", cx);
    })
    .unwrap();
    assert_eq!(view.read_with(cx, |view, _| view.events.clone()), events);
}

#[gpui_kit::test]
async fn filtered_rows_use_paths_and_stale_callbacks_cannot_act_or_steal_focus(
    cx: &mut TestAppContext,
) {
    let server = support::ftp::Server::new(4);
    let root = server.dir.path().join("files");
    fs::write(root.join("a.txt"), "a").unwrap();
    fs::write(root.join("b.txt"), "b").unwrap();
    let (handle, _, pane) = open(cx, server.host(), server.options(), None);
    connected(cx, handle, &pane).await;
    cx.update_window(handle, |_, w, cx| {
        w.click("remote-filter", cx);
        w.input("b.txt", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).files.len(), 1);
        w.right_click(0usize, cx);
        click_menu(w, "Copy path", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("/b.txt")
        );
        pane.update(cx, |pane, _| {
            pane.files.toggle_mark();
            pane.files.visual_range();
        });
        w.press("shift-f10", cx);
        click_menu(w, "Clear all marks", cx);
        assert_eq!(pane.read(cx).filter.read(cx).value().as_str(), "b.txt");
        assert!(pane.read(cx).marked().is_empty());
        assert!(!pane.read(cx).files.range_active());
        w.press("shift-f10", cx);
        pane.update(cx, |pane, cx| {
            let (snapshot, menu) = pane.menu.as_ref().unwrap();
            let snapshot = snapshot.clone();
            let id = menu.popup.entity_id();
            pane.filter
                .update(cx, |filter, cx| filter.set_value("a.txt", w, cx));
            pane.files.filter("a.txt");
            pane.menu_action(&snapshot, Some(id), MenuAction::Mark, w, cx);
        });
        assert!(pane.read(cx).menu.is_none());
        assert!(pane.read(cx).marked().is_empty());
        w.right_click(0usize, cx);
        let old_popup = pane.read(cx).menu.as_ref().unwrap().1.popup.clone();
        pane.update(cx, |pane, cx| {
            pane.open_menu(Some("/a.txt".into()), point(px(500.), px(250.)), w, cx);
        });
        let new_id = pane.read(cx).menu.as_ref().unwrap().1.popup.entity_id();
        old_popup.update(cx, |_, cx| cx.emit(gpui_kit::DismissEvent));
        assert_eq!(
            pane.read(cx).menu.as_ref().unwrap().1.popup.entity_id(),
            new_id
        );
        pane.update(cx, |pane, cx| pane.set_comparison_ready(false, w, cx));
        assert!(pane.read(cx).menu.is_none());
        assert!(pane.read(cx).focus.is_focused(w));
        w.press("shift-f10", cx);
        assert_disabled(w, "Compare this file", &pane, cx);
        pane.update(cx, |pane, cx| {
            pane.focus_filter(&FocusFilter, w, cx);
            pane.show_hidden = !pane.show_hidden;
        });
        w.render_frame(cx);
        assert!(pane.read(cx).menu.is_none());
        assert!(pane.read(cx).filter.focus_handle(cx).is_focused(w));
        pane.update(cx, |pane, cx| pane.focus.focus(w, cx));
        w.press("shift-f10", cx);
        pane.update(cx, |pane, cx| pane.set_context(8, vec![], cx));
        w.render_frame(cx);
        assert!(pane.read(cx).menu.is_none());
        assert!(!pane.read(cx).has_session());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn menus_enforce_mappings_exclusions_and_collapsed_tree_visibility(cx: &mut TestAppContext) {
    let server = support::Server::new(false);
    let root = server.dir.path().join("files");
    fs::create_dir(root.join("mapped")).unwrap();
    fs::create_dir(root.join("outside")).unwrap();
    fs::create_dir(root.join("node_modules")).unwrap();
    fs::write(root.join("mapped/a.txt"), "a").unwrap();
    fs::write(root.join("mapped/b.txt"), "b").unwrap();
    fs::write(root.join("mapped/.hidden"), "hidden").unwrap();
    fs::write(
        root.join("mapped/.a.drift-tmp-0123456789abcdef0123456789abcdef"),
        "staging",
    )
    .unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::create_dir(local.path().join("src")).unwrap();
    let location = Location {
        root: Arc::new(ProjectRoot::open(local.path()).unwrap()),
        directory: local.path().into(),
        slug: Some("test".into()),
        config: RuntimeConfig {
            hosts: vec![],
            mappings: vec![Mapping {
                local: "src".into(),
                remote: "mapped".into(),
            }],
            ui: Default::default(),
        },
    };
    let (handle, view, pane) = open(cx, server.host(), server.options(), Some(location));
    connected(cx, handle, &pane).await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).files.len(), 2);
        w.right_click(1usize, cx);
        assert_disabled(w, "Mark", &pane, cx);
        assert_disabled(w, "Compare this folder", &pane, cx);
        w.press("escape", cx);
        pane.update(cx, |pane, cx| {
            pane.open_menu(
                Some(root.join("node_modules").to_string_lossy().into_owned()),
                point(px(400.), px(200.)),
                w,
                cx,
            );
        });
        assert!(pane.read(cx).menu.is_none());
        w.right_click(0usize, cx);
        assert!(item(w, "Compare this folder").1.visible());
        click_menu(w, "Expand folder", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).files.len(), 4);
        w.right_click(1usize, cx);
        click_menu(w, "Mark siblings", cx);
        assert_eq!(pane.read(cx).marked().len(), 2);
        w.press("shift-f10", cx);
        pane.update(cx, |pane, cx| {
            // Collapse without changing operation or filter: cached entries alone are not a guard.
            pane.tree.collapse(root.join("mapped").to_str().unwrap());
            pane.files
                .replace_entries(pane.tree.nodes().iter().map(|n| n.path.clone()).collect());
            cx.notify();
        });
        w.render_frame(cx);
        assert!(pane.read(cx).menu.is_none());
        assert_eq!(pane.read(cx).marked().len(), 2);
        w.right_click(0usize, cx);
        click_menu(w, "Expand folder", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.right_click(1usize, cx);
        click_menu(w, "Compare this file", cx);
        w.press("shift-f10", cx);
        click_menu(w, "Compare marked files", cx);
        w.press("shift-f10", cx);
        click_menu(w, "Compare project", cx);
        w.right_click(0usize, cx);
        click_menu(w, "Collapse folder", cx);
        assert_eq!(pane.read(cx).files.len(), 2);
        assert_eq!(pane.read(cx).marked().len(), 2);
        w.right_click(0usize, cx);
        click_menu(w, "Expand folder", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.right_click(1usize, cx);
        pane.update(cx, |pane, cx| {
            let (snapshot, menu) = pane.menu.as_ref().unwrap();
            let snapshot = snapshot.clone();
            let id = menu.popup.entity_id();
            pane.files.restrict_marks(Default::default());
            pane.menu_action(&snapshot, Some(id), MenuAction::Mark, w, cx);
        });
        assert!(pane.read(cx).marked().is_empty());
        assert!(pane.read(cx).menu.is_none());
    })
    .unwrap();
    let connection = pane.read_with(cx, |pane, _| pane.connection());
    let events = view.read_with(cx, |view, _| view.events.clone());
    assert!(events.contains(&format!(
        "compare:7:{connection}:{}",
        root.join("mapped/a.txt").display()
    )));
    assert!(events.contains(&format!("toolbar:7:{connection}:marked")));
    assert!(events.contains(&format!("toolbar:7:{connection}:project")));
}

#[gpui_kit::test]
async fn background_menu_hidden_marks_navigation_preview_and_refresh_use_existing_operations(
    cx: &mut TestAppContext,
) {
    let server = support::ftp::Server::new(4);
    let root = server.dir.path().join("files");
    fs::create_dir(root.join("folder")).unwrap();
    fs::write(root.join("folder/inside.txt"), "inside").unwrap();
    fs::write(root.join("a.txt"), "a").unwrap();
    fs::write(root.join(".hidden"), "hidden").unwrap();
    let (handle, view, pane) = open(cx, server.host(), server.options(), None);
    connected(cx, handle, &pane).await;
    cx.update_window(handle, |_, w, cx| {
        w.right_click("remote-files-pane", cx);
        assert!(w.within("popup-menu").try_find(0usize).is_some());
        assert_eq!(item(w, "Invert visible marks").0, 0);
        assert!(item(w, "Copy path").1.visible());
        click_menu(w, "Copy path", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("/")
        );
        w.right_click("remote-files-pane", cx);
        click_menu(w, "Show hidden", cx);
        assert!(pane.read(cx).show_hidden);
        assert_eq!(pane.read(cx).files.len(), 3);
        w.right_click("remote-files-pane", cx);
        click_menu(w, "Invert visible marks", cx);
        assert_eq!(pane.read(cx).marked().len(), 3);
        w.right_click("remote-files-pane", cx);
        click_menu(w, "Clear all marks", cx);
        assert!(pane.read(cx).marked().is_empty());
        w.right_click(2usize, cx);
        click_menu(w, "Preview", cx);
        w.right_click(0usize, cx);
        click_menu(w, "Open folder", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    assert!(view.read_with(cx, |view, _| view.events.contains(&"preview:/a.txt".into())));
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).path, "/folder");
        w.right_click("remote-files-pane", cx);
        click_menu(w, "Back", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).path, "/");
        w.right_click("remote-files-pane", cx);
        click_menu(w, "Forward", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).path, "/folder");
        w.right_click("remote-files-pane", cx);
        click_menu(w, "Up", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    fs::write(root.join("new.txt"), "new").unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.right_click("remote-files-pane", cx);
        click_menu(w, "Refresh", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    assert_eq!(pane.read_with(cx, |pane, _| pane.files.len()), 4);
}

#[gpui_kit::test]
async fn busy_menu_cancel_disconnect_and_stale_operation_use_real_ftp_sessions(
    cx: &mut TestAppContext,
) {
    let server = support::ftp::Server::new(4);
    fs::write(server.dir.path().join("files/a.txt"), "a").unwrap();
    let (handle, _, pane) = open(cx, server.host(), server.options(), None);
    for command in ["Cancel loading", "Disconnect"] {
        connected(cx, handle, &pane).await;
        let session = pane.read_with(cx, |pane, _| pane.session.clone().unwrap());
        let operation = cx
            .update_window(handle, |_, w, cx| {
                w.right_click(0usize, cx);
                let operation = pane.update(cx, |pane, cx| {
                    let (snapshot, menu) = pane.menu.as_ref().unwrap();
                    let snapshot = snapshot.clone();
                    let id = menu.popup.entity_id();
                    pane.operation += 1;
                    let operation = pane.service.list(
                        session.clone(),
                        pane.path.clone(),
                        OperationId {
                            project: pane.project,
                            operation: pane.operation,
                        },
                    );
                    // Retain the real operation without applying its completion until cancellation.
                    pane.listing = Some(operation.cancel.clone());
                    pane.menu_action(&snapshot, Some(id), MenuAction::ComparePath, w, cx);
                    operation
                });
                assert!(pane.read(cx).menu.is_none());
                w.right_click(0usize, cx);
                for label in [
                    "Preview",
                    "Mark",
                    "Mark siblings",
                    "Invert visible marks",
                    "Compare this file",
                    "Compare project",
                    "Refresh",
                ] {
                    assert_disabled(w, label, &pane, cx);
                }
                item(w, "Cancel loading");
                item(w, "Disconnect");
                click_menu(w, command, cx);
                assert!(pane.read(cx).menu.is_none());
                assert!(!pane.read(cx).has_session());
                assert!(!pane.read(cx).is_loading());
                operation
            })
            .unwrap();
        assert!(operation.cancel.is_cancelled());
        cx.wait_for(handle, Duration::from_secs(60), |_, _| {
            session.state() == ConnectionState::Closed
        })
        .await;
    }
    cx.update_window(handle, |_, w, cx| {
        w.right_click("remote-files-pane", cx);
        assert_disabled(w, "Disconnect", &pane, cx);
        assert_disabled(w, "Cancel loading", &pane, cx);
        w.press("escape", cx);
        assert!(pane.read(cx).focus.is_focused(w));
        w.press("shift-f10", cx);
        assert_eq!(
            w.find("popup-menu").bounds().origin,
            pane.read(cx).menu_anchor
        );
        w.press("escape", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
async fn mapping_revisions_reject_callbacks_even_when_the_target_remains_eligible(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    fs::create_dir(server.dir.path().join("files/mapped")).unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::create_dir(local.path().join("src")).unwrap();
    fs::create_dir(local.path().join("next")).unwrap();
    let location = Location {
        root: Arc::new(ProjectRoot::open(local.path()).unwrap()),
        directory: local.path().into(),
        slug: Some("test".into()),
        config: RuntimeConfig {
            hosts: vec![],
            mappings: vec![Mapping {
                local: "src".into(),
                remote: "mapped".into(),
            }],
            ui: Default::default(),
        },
    };
    let (handle, view, pane) = open(cx, server.host(), server.options(), Some(location.clone()));
    connected(cx, handle, &pane).await;
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| view.events.clear());
        window.render_frame(cx);
        let position = window.find(0usize).bounds().center();
        pane.update(cx, |pane, cx| {
            let mut changed = location.clone();
            changed.config.mappings[0].local = "next".into();
            pane.set_selection_location(Some(changed), window, cx);
        });
        window.dispatch_event(
            gpui_kit::MouseDownEvent {
                button: MouseButton::Right,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        assert!(pane.read(cx).menu.is_none());
        assert!(pane.read(cx).selected().is_none());
        window.dispatch_event(
            gpui_kit::MouseUpEvent {
                button: MouseButton::Right,
                position,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        pane.update(cx, |pane, cx| {
            pane.set_selection_location(Some(location.clone()), window, cx)
        });
        window.right_click(0usize, cx);
        pane.update(cx, |pane, cx| {
            let (snapshot, menu) = pane.menu.as_ref().unwrap();
            let snapshot = snapshot.clone();
            let menu_id = menu.popup.entity_id();
            let target = pane.files.selected().unwrap().to_owned();
            assert!(pane.files.can_mark(&target));
            let mut changed = location.clone();
            changed.config.mappings[0].local = "next".into();
            pane.set_selection_location(Some(changed), window, cx);
            assert!(pane.files.can_mark(&target));
            pane.set_selection_location(Some(location.clone()), window, cx);
            assert!(pane.files.can_mark(&target));
            assert!(!pane.menu_valid(&snapshot, cx));
            pane.menu_action(
                &snapshot,
                Some(menu_id),
                MenuAction::ComparePath,
                window,
                cx,
            );
            assert!(pane.menu.is_none());
        });
    })
    .unwrap();
    assert!(view.read_with(cx, |view, _| view.events.is_empty()));
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    assert!(view.read_with(cx, |view, _| {
        view.events
            .iter()
            .all(|event| !event.starts_with("compare:") && !event.starts_with("preview:"))
    }));
    cx.update_window(handle, |_, window, cx| {
        window.right_click(0usize, cx);
        click_menu(window, "Compare this folder", cx);
    })
    .unwrap();
    assert!(view.read_with(cx, |view, _| {
        view.events
            .iter()
            .any(|event| event.starts_with("compare:"))
    }));
}
