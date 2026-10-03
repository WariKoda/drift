use super::*;
use crate::sftp_test_support as support;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, size};
use std::{fs, time::Duration};
struct RemoteWindow {
    pane: Entity<RemotePane>,
}
impl Render for RemoteWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Drift")
            .size_full()
            .flex()
            .child(self.pane.clone())
    }
}
#[gpui_kit::test]
async fn real_remote_navigation_filter_keyboard_and_project_switch(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let root = server.dir.path().join("files");
    fs::create_dir(root.join("child")).unwrap();
    fs::write(root.join("child/text.txt"), "remote").unwrap();
    fs::write(root.join("top.txt"), "top").unwrap();
    let host = server.host();
    let options = server.options();
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1000.), px(700.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
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
                    pane.update(cx, |pane, cx| pane.set_context(1, vec![host], cx));
                    RemoteWindow { pane }
                })
            },
        )
        .unwrap()
    });
    let pane = view.read_with(cx, |view, _| view.pane.clone());
    cx.update_window(handle, |_, window, cx| {
        window.click(("connect-host", 0usize), cx)
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    let session = pane.read_with(cx, |pane, _| pane.session.clone().unwrap());
    let previews = std::rc::Rc::new(std::cell::Cell::new(0));
    let _subscription = cx.update(|cx| {
        let previews = previews.clone();
        cx.subscribe(&pane, move |_, event: &RemoteEvent, _| {
            if matches!(event, RemoteEvent::Preview { .. }) {
                previews.set(previews.get() + 1);
            }
        })
    });
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(pane.read(cx).path, root.to_string_lossy());
        window.press("down", cx);
        window.press("space", cx);
        window.press("shift-down", cx);
        assert_eq!(pane.read(cx).marked().len(), 2);
        window.press("home", cx);
        pane.update(cx, |pane, cx| pane.preview_selected(window, cx));
        assert!(
            !pane
                .read(cx)
                .tree
                .node(root.join("child").to_str().unwrap())
                .unwrap()
                .expanded
        );
        window.click(0usize, cx);
        pane.update(cx, |pane, cx| pane.preview_selected(window, cx));
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(previews.get(), 0);
        assert_eq!(pane.read(cx).path, root.join("child").to_string_lossy());
        assert_eq!(pane.read(cx).marked().len(), 2);
        window.press("alt-left", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(pane.read(cx).path, root.to_string_lossy());
        window.click("remote-filter", cx);
        window.input("top", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(pane.read(cx).files.len(), 1);
        for key in ["enter", "down", "escape"] {
            let selected = pane.read(cx).selected().map(str::to_owned);
            let marked = pane.read(cx).marked();
            let range = pane.read(cx).files.range_active();
            let operation = pane.read(cx).operation;
            let scroll = pane.read(cx).scroll.0.borrow().base_handle.offset();
            window.press(key, cx);
            assert!(pane.read(cx).focus.is_focused(window));
            assert_eq!(pane.read(cx).selected(), selected.as_deref());
            assert_eq!(pane.read(cx).marked(), marked);
            assert_eq!(pane.read(cx).files.range_active(), range);
            assert_eq!(pane.read(cx).operation, operation);
            assert_eq!(pane.read(cx).scroll.0.borrow().base_handle.offset(), scroll);
            assert_eq!(pane.read(cx).filter.read(cx).value().as_str(), "top");
            assert_eq!(previews.get(), 0);
            window.press("ctrl-f", cx);
        }
        window.press("enter", cx);
        window.press("home", cx);
        pane.update(cx, |pane, cx| pane.preview_selected(window, cx));
        window.press("ctrl-shift-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            root.join("top.txt").to_string_lossy()
        );
        window.press("ctrl-f", cx);
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        window.input("missing", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        pane.read(cx).files.is_empty()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(previews.get(), 1);
        for key in ["enter", "down", "escape"] {
            window.press(key, cx);
            assert!(pane.read(cx).focus.is_focused(window));
            assert!(pane.read(cx).selected().is_none());
            assert_eq!(pane.read(cx).filter.read(cx).value().as_str(), "missing");
            assert_eq!(pane.read(cx).path, root.to_string_lossy());
            window.press("ctrl-f", cx);
        }
        // An actual outstanding directory request must survive filter Escape.
        pane.update(cx, |pane, cx| pane.refresh(&Refresh, window, cx));
        window.press("ctrl-f", cx);
        let cancel = pane.read(cx).listing.as_ref().unwrap().clone();
        window.press("escape", cx);
        assert!(!cancel.is_cancelled());
        assert!(pane.read(cx).is_loading());
        pane.update(cx, |pane, cx| pane.preview_selected(window, cx));
        pane.update(cx, |pane, cx| pane.set_context(2, vec![], cx));
        pane.update(cx, |pane, cx| pane.preview_selected(window, cx));
        assert!(pane.read(cx).session.is_none());
        assert!(pane.read(cx).entries.is_empty());
        assert!(pane.read(cx).marked().is_empty());
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, _| {
        session.state() != ConnectionState::Connected
    })
    .await;
    assert_eq!(session.state(), ConnectionState::Closed);
}

#[gpui_kit::test]
async fn lazy_remote_tree_refresh_collapse_and_stale_results_preserve_session(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let root = server.dir.path().join("files");
    fs::create_dir_all(root.join("child/nested")).unwrap();
    fs::write(root.join("child/nested/deep.txt"), "remote").unwrap();
    fs::write(root.join("child/.hidden"), "hidden").unwrap();
    fs::create_dir(root.join("child/node_modules")).unwrap();
    fs::write(root.join("top.txt"), "top").unwrap();
    let host = server.host();
    let options = server.options();
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1000.), px(700.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
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
                    pane.update(cx, |pane, cx| pane.set_context(1, vec![host], cx));
                    RemoteWindow { pane }
                })
            },
        )
        .unwrap()
    });
    let pane = view.read_with(cx, |view, _| view.pane.clone());
    cx.update_window(handle, |_, window, cx| {
        window.click(("connect-host", 0usize), cx)
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;

    let session = pane.read_with(cx, |pane, _| pane.session.clone().unwrap());
    cx.update_window(handle, |_, window, cx| {
        window.click(("remote-tree-toggle", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(pane.read(cx).path, root.to_string_lossy());
        assert!(pane.read(cx).history.previous().is_none());
        assert_eq!(pane.read(cx).files.len(), 3);
        window.press("right", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.press("right", cx);
        assert_eq!(
            pane.read(cx).selected(),
            root.join("child/nested/deep.txt").to_str()
        );
        window.press("space", cx);
        window.press("f5", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            pane.read(cx).selected(),
            root.join("child/nested/deep.txt").to_str()
        );
        assert!(
            pane.read(cx)
                .tree
                .node(root.join("child/nested").to_str().unwrap())
                .unwrap()
                .expanded
        );
        assert_eq!(pane.read(cx).marked().len(), 1);
        assert_eq!(pane.read(cx).session_id(), Some(session.id));
        window.press("left", cx);
        window.press("left", cx);
        assert_eq!(pane.read(cx).files.len(), 2);
        assert_eq!(
            pane.read(cx)
                .files
                .marked_descendants(root.join("child").to_str().unwrap()),
            1
        );
        pane.update(cx, |pane, cx| {
            let path = root.join("child").to_string_lossy().into_owned();
            pane.expand(path.clone(), window, cx);
            pane.toggle_tree(path, window, cx); // Discard the result, keep the connection.
        });
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(pane.read(cx).files.len(), 2);
        assert_eq!(session.state(), ConnectionState::Connected);
        assert_eq!(pane.read(cx).marked().len(), 1);
        pane.update(cx, |pane, cx| {
            pane.expand(
                root.join("child").to_string_lossy().into_owned(),
                window,
                cx,
            );
            pane.set_context(2, vec![], cx);
        });
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, _| {
        session.state() == ConnectionState::Closed
    })
    .await;
    assert!(pane.read_with(cx, |pane, _| pane.tree.nodes().is_empty()));
    assert!(pane.read_with(cx, |pane, _| pane.marked().is_empty()));
}
