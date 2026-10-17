use super::*;
mod input;
mod preferences;
mod selection;
use crate::{actions::bind_keys, sftp_test_support as support};
use gpui_kit::base::test_support::snapshots;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, TestAppContext, WindowBounds, WindowOptions, point, px, size,
};
use std::{fs, time::Duration};

struct Fixture {
    handle: AnyWindowHandle,
    shell: Entity<Shell>,
    local: tempfile::TempDir,
    server: support::Server,
    _config: tempfile::TempDir,
    preferences_writer: Option<drift_app::gui_preferences::PreferencesWriter>,
}
impl Fixture {
    async fn new(cx: &mut TestAppContext) -> Self {
        Self::build(cx, None).await
    }
    async fn build(
        cx: &mut TestAppContext,
        preferences: Option<drift_core::gui_preferences::GuiPreferences>,
    ) -> Self {
        cx.executor().allow_parking();
        let server = support::Server::new(false);
        let local = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        fs::create_dir(local.path().join("folder")).unwrap();
        for i in 0..60 {
            let name = format!("keep-{i:02}.txt");
            fs::write(local.path().join(&name), format!("local {i}\n")).unwrap();
            fs::write(
                server.dir.path().join("files").join(name),
                format!("remote {i}\n"),
            )
            .unwrap();
        }
        let store = Store::new(config.path().into());
        store.save_host(None, None, server.host()).unwrap();
        let preferences_writer = preferences.as_ref().map(|_| {
            drift_app::gui_preferences::PreferencesWriter::open(store.clone(), Default::default())
                .unwrap()
        });
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
                        let shell = Shell::new(
                            w,
                            cx,
                            store,
                            service.clone(),
                            RemoteService::with_options(service, server.options()),
                            local.path().to_path_buf(),
                        );
                        match preferences {
                            Some(initial) => shell.with_preferences(
                                initial,
                                preferences_writer.clone().unwrap(),
                                None,
                                w,
                                cx,
                            ),
                            None => shell,
                        }
                    })
                },
            )
            .unwrap()
        });
        let fixture = Self {
            handle,
            shell,
            local,
            server,
            _config: config,
            preferences_writer,
        };
        fixture.idle(cx).await;
        cx.update_window(handle, |_, w, cx| {
            w.click("remote", cx);
            w.click(("connect-host", 0usize), cx);
        })
        .unwrap();
        fixture.idle(cx).await;
        assert!(
            fixture
                .shell
                .read_with(cx, |s, cx| s.remote.read(cx).has_session())
        );
        fixture
    }
    async fn idle(&self, cx: &mut TestAppContext) {
        cx.wait_for(self.handle, Duration::from_secs(60), |_, cx| {
            let s = self.shell.read(cx);
            s.browser.read(cx).location().is_some()
                && !s.browser.read(cx).is_loading()
                && !s.remote.read(cx).is_loading()
                && !s.preview.read(cx).is_loading()
                && !s.comparison.read(cx).is_loading()
        })
        .await;
    }
}

fn browser_state(s: &Shell, cx: &gpui_kit::App) -> String {
    let browser = s.browser.read(cx);
    let remote = s.remote.read(cx);
    let preview = s.preview.read(cx);
    format!(
        "entities={:?}; local={:?}; remote={:?}; preview={:?}",
        (
            s.browser.entity_id(),
            s.remote.entity_id(),
            s.preview.entity_id()
        ),
        (
            browser.id(),
            browser.selected(),
            browser.marked(),
            browser.can_back(),
            browser.can_forward(),
            browser.location().map(|l| &l.directory),
            browser.len(),
            browser.is_loading(),
            browser.shows_hidden(),
            browser.shows_ignored()
        ),
        (
            remote.project(),
            remote.connection(),
            remote.session_id(),
            remote.selected(),
            remote.marked(),
            remote.directory(),
            remote.is_loading(),
            remote.has_session()
        ),
        (
            preview.id(),
            preview.is_loading(),
            preview.is_loading_remote()
        )
    )
}

fn first_width(w: &mut Window, body: &'static str) -> Pixels {
    w.within(body).find("split-first").bounds().size.width
}
fn drag(w: &mut Window, body: &'static str, delta: f32, cx: &mut gpui_kit::App) {
    w.render_frame(cx);
    let from = w.within(body).find("pane-divider").bounds().center();
    w.drag(from, point(from.x + px(delta), from.y), cx);
}
fn row_positions(w: &Window, list: &str) -> Vec<(String, Pixels)> {
    let id = gpui_kit::ElementId::from(list.to_owned());
    let mut rows: Vec<_> = snapshots(w)
        .into_iter()
        .filter(|s| s.visible() && s.path().contains(&id))
        .map(|s| {
            let start = s.path().iter().position(|part| *part == id).unwrap();
            (format!("{:?}", &s.path()[start..]), s.bounds().origin.y)
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!rows.is_empty(), "no rendered rows in {list}");
    rows
}
fn begin_drag(w: &mut Window, body: &'static str, cx: &mut gpui_kit::App) -> Point<Pixels> {
    w.render_frame(cx);
    let position = w.within(body).find("pane-divider").bounds().center();
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
    w.render_frame(cx);
    position
}
fn finish_drag(w: &mut Window, position: Point<Pixels>, cx: &mut gpui_kit::App) {
    w.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            modifiers: Default::default(),
        }
        .to_platform_input(),
        cx,
    );
    w.render_frame(cx);
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
    w.render_frame(cx);
}

#[gpui_kit::test]
async fn resize_browser_modes_retains_entities_navigation_range_scroll_and_preview(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    let s = &f.shell;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("local-browser", cx);
        w.within("local-files").click(0usize, cx); // Real directory navigation.
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| w.click("up", cx))
        .unwrap();
    f.idle(cx).await;
    for mode in ["local", "listing", "remote-preview"] {
        cx.update_window(f.handle, |_, w, cx| {
            if mode == "local" {
                w.click("filter", cx);
                w.input("keep", cx);
            } else {
                w.click("remote", cx);
                if mode == "listing" {
                    w.click("remote-filter", cx);
                    w.input("keep", cx);
                }
            }
        })
        .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            if mode == "local" {
                s.read(cx).browser.focus_handle(cx).focus(w, cx);
            } else {
                s.read(cx).remote.focus_handle(cx).focus(w, cx);
            }
            if mode != "remote-preview" {
                w.press("end", cx);
                w.press("space", cx);
                w.press("v", cx);
                w.press("up", cx);
            }
            if mode != "listing" {
                w.press("enter", cx);
            }
        })
        .unwrap();
        f.idle(cx).await;
        cx.update_window(f.handle, |_, w, cx| {
            let state = browser_state(s.read(cx), cx);
            let focus = w.focused(cx);
            let list = if mode == "local" {
                "local-files"
            } else {
                "remote-files"
            };
            w.render_frame(cx);
            let rows = row_positions(w, list);
            let initial = first_width(w, "browser-split");
            drag(w, "browser-split", 100., cx);
            assert!(first_width(w, "browser-split") > initial + px(30.));
            let dragged = first_width(w, "browser-split");
            w.press("ctrl-alt-left", cx);
            assert!(first_width(w, "browser-split") < dragged);
            w.press("ctrl-alt-right", cx);
            assert!((first_width(w, "browser-split") - dragged).abs() < px(2.));
            w.press("ctrl-alt-0", cx);
            assert!((first_width(w, "browser-split") - initial).abs() < px(2.));
            assert_eq!(browser_state(s.read(cx), cx), state);
            assert_eq!(w.focused(cx), focus);
            let shell = s.read(cx);
            assert_eq!(row_positions(w, list), rows);
            assert_eq!(
                w.find(if mode == "local" {
                    "filter"
                } else {
                    "remote-filter"
                })
                .value(),
                Some("keep")
            );
            if mode == "local" {
                assert!(shell.browser.focus_handle(cx).is_focused(w));
                w.click("copy-preview", cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some("local 58\n")
                );
            } else {
                assert!(shell.remote.focus_handle(cx).is_focused(w));
                if mode == "remote-preview" {
                    w.click("copy-preview", cx);
                    assert_eq!(
                        cx.read_from_clipboard().unwrap().text().as_deref(),
                        Some("remote 58\n")
                    );
                }
            }
            if mode != "listing" {
                w.press("v", cx); // Complete the interval begun before any resize.
                if mode == "local" {
                    assert_eq!(
                        s.read(cx).browser.read(cx).marked(),
                        ["keep-58.txt", "keep-59.txt"]
                    );
                } else {
                    assert_eq!(s.read(cx).remote.read(cx).marked().len(), 2);
                }
            }
        })
        .unwrap();
    }
}

#[gpui_kit::test]
async fn resize_menu_dismissal_and_pending_io_do_not_change_rows_or_restart_operations(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    for remote in [false, true] {
        cx.update_window(f.handle, |_, w, cx| {
            let s = f.shell.read(cx);
            if remote {
                s.remote.focus_handle(cx).focus(w, cx);
            } else {
                s.browser.focus_handle(cx).focus(w, cx);
            }
            w.press("end", cx);
            w.press("space", cx);
            w.press("v", cx);
            w.press("up", cx);
            w.press("shift-f10", cx);
            w.render_frame(cx);
            let width = first_width(w, "browser-split");
            let state = browser_state(f.shell.read(cx), cx);
            w.press("ctrl-alt-left", cx);
            w.press("ctrl-alt-right", cx);
            w.press("ctrl-alt-0", cx);
            assert!(w.find("popup-menu").visible());
            assert_eq!(first_width(w, "browser-split"), width);
            drag(w, "browser-split", -80., cx); // Outside click dismisses; no row receives it.
            assert_eq!(browser_state(f.shell.read(cx), cx), state);
        })
        .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            assert!(w.try_find("popup-menu").is_none());
            w.press("v", cx);
            assert_eq!(
                if remote {
                    f.shell.read(cx).remote.read(cx).marked().len()
                } else {
                    f.shell.read(cx).browser.read(cx).marked().len()
                },
                2
            );
            // The real async refresh stays pending during the event turn.
            w.press("f5", cx);
            let s = f.shell.read(cx);
            let state = browser_state(s, cx);
            assert!(if remote {
                s.remote.read(cx).is_loading()
            } else {
                s.browser.read(cx).is_loading()
            });
            drag(w, "browser-split", 60., cx);
            w.press("ctrl-alt-right", cx);
            let s = f.shell.read(cx);
            assert_eq!(browser_state(s, cx), state);
            assert!(if remote {
                s.remote.read(cx).is_loading()
            } else {
                s.browser.read(cx).is_loading()
            });
        })
        .unwrap();
        f.idle(cx).await;
        assert!(
            f.shell
                .read_with(cx, |s, cx| s.remote.read(cx).has_session())
        );
    }
    cx.update_window(f.handle, |_, w, cx| w.click("local-preview", cx))
        .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        let s = f.shell.read(cx);
        let location = s.browser.read(cx).location().unwrap().clone();
        let project = s.browser.read(cx).id().project;
        let preview = s.preview.clone();
        preview.update(cx, |pane, cx| {
            pane.show(location, "keep-00.txt".into(), project, w, cx)
        });
        let id = f.shell.read(cx).preview.read(cx).id();
        assert!(f.shell.read(cx).preview.read(cx).is_loading());
        drag(w, "browser-split", -60., cx);
        w.press("ctrl-alt-left", cx);
        assert_eq!(f.shell.read(cx).preview.read(cx).id(), id);
        assert!(f.shell.read(cx).preview.read(cx).is_loading());
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("local 0\n")
        );
    })
    .unwrap();
}

#[gpui_kit::test]
async fn resize_comparison_keeps_diff_decisions_filter_and_pending_confirmation(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    let before = "before\n".repeat(60);
    let after = "after\n".repeat(60);
    fs::write(
        f.local.path().join("keep-00.txt"),
        before.clone() + "local\n" + &after,
    )
    .unwrap();
    let remote_path = f.server.dir.path().join("files/keep-00.txt");
    fs::write(&remote_path, before + "remote\n" + &after).unwrap();
    let browser_width = cx
        .update_window(f.handle, |_, w, cx| {
            drag(w, "browser-split", 80., cx);
            let browser_width = first_width(w, "browser-split");
            let from = begin_drag(w, "browser-split", cx);
            f.shell.update(cx, |s, cx| {
                s.toolbar_event(&ToolbarEvent::CompareProject, w, cx)
            });
            w.render_frame(cx);
            assert!(f.shell.read(cx).comparison.read(cx).is_loading());
            let width = first_width(w, "comparison-split");
            finish_drag(w, point(from.x + px(180.), from.y), cx);
            assert_eq!(first_width(w, "comparison-split"), width);
            drag(w, "comparison-split", 50., cx);
            w.press("ctrl-alt-left", cx);
            assert!(f.shell.read(cx).comparison.read(cx).is_loading());
            browser_width
        })
        .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| w.click("comparison-back", cx))
        .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(first_width(w, "browser-split"), browser_width);
        w.click("compare-project", cx);
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("comparison-filter", cx);
        w.input("keep", cx);
        f.shell.read(cx).comparison.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.click("comparison-action", cx);
        w.click(("diff-row", 0usize), cx); // Unfold the real leading equal run.
        w.click(("diff-row", 2usize), cx);
        w.press("pagedown", cx);
        w.press("ctrl-c", cx);
        let selected_text = cx.read_from_clipboard().unwrap().text().unwrap();
        w.click("copy-diff", cx);
        let diff_text = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(diff_text.contains("before"));
        let action = w.find("comparison-action").label().unwrap().to_owned();
        let pane = f.shell.read(cx).comparison.clone();
        let entity = pane.entity_id();
        let result = pane.read(cx).session.as_ref().unwrap().entries[0]
            .result
            .as_ref()
            .unwrap()
            .clone();
        w.render_frame(cx);
        let rows = row_positions(w, "diff-lines");
        w.scroll(
            ("comparison-file", 0usize),
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-300.))),
            cx,
        );
        let files = row_positions(w, "comparison-list");
        let focus = w.focused(cx);
        let initial = first_width(w, "comparison-split");
        drag(w, "comparison-split", 100., cx);
        assert!(first_width(w, "comparison-split") > initial + px(30.));
        w.press("ctrl-alt-left", cx);
        w.press("ctrl-alt-right", cx);
        assert_eq!(pane.entity_id(), entity);
        assert!(std::sync::Arc::ptr_eq(
            &result,
            pane.read(cx).session.as_ref().unwrap().entries[0]
                .result
                .as_ref()
                .unwrap()
        ));
        assert!(!pane.read(cx).is_loading());
        assert_eq!(w.find("comparison-action").label(), Some(action.as_str()));
        assert_eq!(w.find("comparison-filter").value(), Some("keep"));
        assert_eq!(row_positions(w, "diff-lines"), rows);
        assert_eq!(row_positions(w, "comparison-list"), files);
        assert_eq!(w.focused(cx), focus);
        w.press("ctrl-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            selected_text
        );
        w.click("copy-diff", cx);
        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), diff_text);
        pane.focus_handle(cx).focus(w, cx);
        w.press("shift-s", cx);
        assert!(w.find("sync-confirm").visible());
        drag(w, "comparison-split", -90., cx);
        w.press("ctrl-alt-right", cx);
        w.press("ctrl-alt-0", cx);
        assert!(w.find("sync-confirm").visible());
        assert!(!pane.read(cx).is_loading());
        assert!(pane.read(cx).sync_result_for_test().is_none());
        w.click("sync-dismiss", cx);
        drag(w, "comparison-split", 90., cx);
        let width = first_width(w, "comparison-split");
        w.click("comparison-back", cx);
        w.click("compare-project", cx);
        w.render_frame(cx);
        assert!((first_width(w, "comparison-split") - width).abs() < px(2.));
    })
    .unwrap();
    f.idle(cx).await;
    assert!(!f.server.dir.path().join("write-started").exists());
    assert!(
        fs::read_to_string(f.local.path().join("keep-00.txt"))
            .unwrap()
            .contains("local\n")
    );
    assert!(
        fs::read_to_string(remote_path)
            .unwrap()
            .contains("remote\n")
    );
}

#[gpui_kit::test]
async fn resize_small_viewports_remount_and_stale_project_drag_are_confined(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    for body_id in ["browser-split", "comparison-split"] {
        cx.simulate_window_resize(f.handle, size(px(1400.), px(900.)));
        if body_id == "comparison-split" {
            cx.update_window(f.handle, |_, w, cx| w.click("compare-project", cx))
                .unwrap();
            f.idle(cx).await;
        }
        for width in [720., 420., 180.] {
            cx.simulate_window_resize(f.handle, size(px(width), px(900.)));
            cx.update_window(f.handle, |_, w, cx| {
                for delta in [-2000., 2000.] {
                    drag(w, body_id, delta, cx);
                    let body = w.find(body_id).bounds();
                    let first = w.within(body_id).find("split-first").bounds();
                    let second = w.within(body_id).find("split-second").bounds();
                    assert!(first.size.width > px(0.) && second.size.width > px(0.));
                    assert!(body.origin.x >= px(0.) && body.right() <= px(width + 1.));
                    assert!(
                        first.origin.x >= body.origin.x && second.right() <= body.right() + px(1.)
                    );
                    assert!(first.size.width + second.size.width <= body.size.width + px(1.));
                }
            })
            .unwrap();
        }
    }
    cx.simulate_window_resize(f.handle, size(px(1400.), px(900.)));
    cx.update_window(f.handle, |_, w, cx| w.click("comparison-back", cx))
        .unwrap();
    cx.simulate_window_resize(f.handle, size(px(1400.), px(900.)));
    let other = tempfile::tempdir().unwrap();
    fs::write(other.path().join("new.txt"), "new project").unwrap();
    let width = cx
        .update_window(f.handle, |_, w, cx| {
            w.press("ctrl-alt-0", cx);
            drag(w, "browser-split", 100., cx);
            let width = first_width(w, "browser-split");
            w.click("hosts", cx);
            w.press("escape", cx);
            width
        })
        .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(first_width(w, "browser-split"), width);
        let position = begin_drag(w, "browser-split", cx);
        f.shell
            .update(cx, |s, cx| s.open_project(other.path().into(), w, cx));
        w.render_frame(cx);
        let new_width = first_width(w, "browser-split");
        let id = f.shell.read(cx).browser.read(cx).id();
        finish_drag(w, point(position.x + px(250.), position.y), cx);
        assert_eq!(first_width(w, "browser-split"), new_width);
        assert_eq!(f.shell.read(cx).browser.read(cx).id(), id);
        assert!(f.shell.read(cx).browser.read(cx).is_loading());
    })
    .unwrap();
    f.idle(cx).await;
    assert!(
        !f.shell
            .read_with(cx, |s, cx| s.remote.read(cx).has_session())
    );
    assert_eq!(
        f.shell.read_with(cx, |s, cx| s
            .browser
            .read(cx)
            .location()
            .unwrap()
            .directory
            .clone()),
        other.path()
    );
}

#[gpui_kit::test]
async fn resize_active_real_sync_does_not_cancel_or_schedule_another_transfer(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    let contents = vec![b'x'; 2 * 1024 * 1024];
    fs::write(f.local.path().join("z-upload"), &contents).unwrap();
    cx.update_window(f.handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    f.idle(cx).await;
    fs::write(f.server.dir.path().join("slow-data"), "").unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.press("end", cx);
        w.click("sync-selected", cx);
        w.click("sync-confirm", cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(60), |_, cx| {
        f.shell.read(cx).comparison.read(cx).is_loading()
            && f.server.dir.path().join("write-started").exists()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        let pane = f.shell.read(cx).comparison.clone();
        let result = pane
            .read(cx)
            .session
            .as_ref()
            .unwrap()
            .entries
            .last()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .clone();
        let connection = f.shell.read(cx).remote.read(cx).session_id();
        let browser_id = f.shell.read(cx).browser.read(cx).id();
        let preview_id = f.shell.read(cx).preview.read(cx).id();
        drag(w, "comparison-split", 90., cx);
        w.press("ctrl-alt-left", cx);
        w.press("ctrl-alt-right", cx);
        w.press("ctrl-alt-0", cx);
        assert!(pane.read(cx).is_loading());
        assert!(pane.read(cx).sync_result_for_test().is_none());
        assert!(std::sync::Arc::ptr_eq(
            &result,
            pane.read(cx)
                .session
                .as_ref()
                .unwrap()
                .entries
                .last()
                .unwrap()
                .result
                .as_ref()
                .unwrap()
        ));
        assert_eq!(f.shell.read(cx).remote.read(cx).session_id(), connection);
        assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
        assert_eq!(f.shell.read(cx).preview.read(cx).id(), preview_id);
        fs::remove_file(f.server.dir.path().join("slow-data")).unwrap();
    })
    .unwrap();
    f.idle(cx).await;
    f.shell.read_with(cx, |s, cx| {
        let report = s.comparison.read(cx).sync_result_for_test().unwrap();
        assert!(report.stopped.is_none());
        assert_eq!(
            report
                .outcomes
                .iter()
                .filter(|o| **o == drift_app::sync::ItemOutcome::Completed)
                .count(),
            1
        );
        assert!(s.remote.read(cx).has_session());
    });
    assert_eq!(
        fs::read(f.server.dir.path().join("files/z-upload")).unwrap(),
        contents
    );
    assert_eq!(
        fs::read(f.server.dir.path().join("files/keep-00.txt")).unwrap(),
        b"remote 0\n"
    );
}

#[gpui_kit::test]
async fn resize_press_blocks_menus_before_native_drag_starts(cx: &mut TestAppContext) {
    let f = Fixture::new(cx).await;
    for remote in [false, true] {
        cx.update_window(f.handle, |_, w, cx| {
            if remote {
                f.shell.read(cx).remote.focus_handle(cx).focus(w, cx);
            } else {
                f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
            }
            w.render_frame(cx);
            let state = browser_state(f.shell.read(cx), cx);
            let from = w
                .within("browser-split")
                .find("pane-divider")
                .bounds()
                .center();
            w.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            assert!(!cx.has_active_drag());
            assert!(f.shell.read(cx).split.is_resizing(), "press claimed");
            w.render_frame(cx);
            assert!(f.shell.read(cx).split.is_resizing(), "press survives frame");
            w.press("menu", cx);
            w.render_frame(cx);
            assert!(w.try_find("popup-menu").is_none());
            let row = w
                .within(if remote {
                    "remote-files"
                } else {
                    "local-files"
                })
                .find(0usize)
                .bounds()
                .center();
            w.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Right,
                    position: row,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            w.render_frame(cx);
            assert!(w.try_find("popup-menu").is_none());
            assert_eq!(browser_state(f.shell.read(cx), cx), state);
            w.press("escape", cx);
            assert!(!cx.has_active_drag());
            w.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position: from,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            w.render_frame(cx);
        })
        .unwrap();
    }
}
