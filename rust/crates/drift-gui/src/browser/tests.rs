use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, size};
use std::{fs, time::Duration};

// Exercise the production pane twice in the same focus tree. No shared pane
// state or replacement service: both panes browse actual temporary directories.
struct TwoPanes {
    left: Entity<BrowserPane>,
    right: Entity<BrowserPane>,
}
impl Render for TwoPanes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Drift")
            .flex()
            .size_full()
            .child(
                div()
                    .id("left")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .on_action(cx.listener(|this, _: &FindFiles, w, cx| {
                        this.left
                            .update(cx, |pane, cx| pane.command(BrowserCommand::Find, w, cx));
                    }))
                    .child(
                        div()
                            .key_context("DriftLocalFilter")
                            .on_action(cx.listener(|this, _: &FocusResults, w, cx| {
                                this.left.update(cx, |pane, cx| pane.focus(w, cx));
                            }))
                            .on_action(cx.listener(|this, _: &Cancel, w, cx| {
                                this.left.update(cx, |pane, cx| pane.focus(w, cx));
                            }))
                            .on_action(cx.listener(
                                |this, _: &gpui_kit::component::input::Escape, w, cx| {
                                    this.left.update(cx, |pane, cx| pane.focus(w, cx));
                                },
                            ))
                            .child(self.left.read(cx).header()),
                    )
                    .child(self.left.clone()),
            )
            .child(
                div()
                    .id("right")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(self.right.read(cx).header())
                    .child(self.right.clone()),
            )
    }
}

#[gpui_kit::test]
async fn panes_keep_filter_selection_history_and_cancellation_independent(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let left_root = tempfile::tempdir().unwrap();
    let right_root = tempfile::tempdir().unwrap();
    let right_directory = right_root
        .path()
        .join("long-project-directory-name-".repeat(7));
    fs::create_dir(&right_directory).unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir(left_root.path().join("child")).unwrap();
    fs::write(left_root.path().join("left.txt"), "left").unwrap();
    fs::write(right_directory.join("right.txt"), "right").unwrap();
    fs::write(right_directory.join("other.txt"), "other").unwrap();
    let (handle, panes) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1200.), px(800.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let service = BrowserService::new().unwrap();
                    let store = Store::new(config.path().into());
                    let left = cx.new(|cx| {
                        BrowserPane::new(
                            store.clone(),
                            service.clone(),
                            left_root.path().into(),
                            window,
                            cx,
                        )
                    });
                    let right = cx.new(|cx| {
                        BrowserPane::new(store, service, right_directory.clone(), window, cx)
                    });
                    left.update(cx, |pane, cx| {
                        pane.open(left_root.path().into(), window, cx)
                    });
                    right.update(cx, |pane, cx| {
                        pane.open(right_directory.clone(), window, cx)
                    });
                    TwoPanes { left, right }
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        let panes = panes.read(cx);
        panes.left.read(cx).location().is_some() && panes.right.read(cx).location().is_some()
    })
    .await;
    let (left, right) = panes.read_with(cx, |panes, _| (panes.left.clone(), panes.right.clone()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("right").click("filter", cx);
        assert!(
            right.read(cx).filter.focus_handle(cx).is_focused(window),
            "right filter did not receive focus after clicking its bounds"
        );
        window.input("right", cx);
    })
    .unwrap();
    // Keep the filter clickable even with paths longer than the pane, including
    // macOS temporary roots. Then wait for the asynchronous input notification.
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        right.read(cx).len() == 1
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(left.read(cx).len(), 2);
        assert_eq!(right.read(cx).len(), 1);
        window.within("right").click(0usize, cx);
        assert_eq!(right.read(cx).selected(), Some("right.txt"));
        assert_eq!(left.read(cx).selected(), None);
        window.within("left").click(0usize, cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !left.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            left.read(cx).location().unwrap().directory,
            left_root.path().join("child")
        );
        assert!(left.read(cx).can_back());
        assert!(!right.read(cx).can_back());
        assert_eq!(
            right.read(cx).location().unwrap().directory,
            right_directory
        );
        assert_eq!(right.read(cx).selected(), Some("right.txt"));
        window.press("alt-left", cx); // Focus still belongs to the left pane.
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !left.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            left.read(cx).location().unwrap().directory,
            left_root.path()
        );
        assert_eq!(right.read(cx).selected(), Some("right.txt"));
        assert_eq!(right.read(cx).filter.read(cx).value().as_str(), "right");
        window.render_frame(cx);
        window.within("right").click(0usize, cx);
        window.press("ctrl-shift-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("right.txt")
        );
        window.press("space", cx);
        assert_eq!(right.read(cx).marked(), ["right.txt"]);
        assert!(left.read(cx).marked().is_empty());
        window.press("v", cx);
        window.press("escape", cx); // Cancel range, preserving the filter/mark.
        assert_eq!(right.read(cx).marked(), ["right.txt"]);
        assert_eq!(right.read(cx).filter.read(cx).value().as_str(), "right");
        window.press("*", cx);
        assert!(right.read(cx).marked().is_empty());
        window.press("shift-v", cx);
        assert_eq!(right.read(cx).marked(), ["right.txt"]);
        window.press("ctrl-f", cx);
        assert!(right.read(cx).filter.focus_handle(cx).is_focused(window));
        assert!(!left.read(cx).filter.focus_handle(cx).is_focused(window));
        let right_id = right.read(cx).id();
        left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Find, window, cx);
            let cancel = pane.listing_cancel.as_ref().unwrap().clone();
            let stale_id = pane.id();
            let location = pane.location().unwrap().clone();
            pane.command(BrowserCommand::Cancel, window, cx);
            assert!(cancel.is_cancelled());
            assert!(!pane.is_loading());
            // A late listing from the cancelled operation must neither replace
            // the rows nor commit a pending history transition.
            pane.apply_directory(
                stale_id,
                Directory {
                    entries: location
                        .root
                        .entries(std::path::Path::new("child"))
                        .unwrap(),
                    location,
                    registry: Registry::default(),
                },
                window,
                cx,
            );
            assert_eq!(pane.len(), 2);
            assert!(pane.can_forward());
        });
        assert_eq!(right.read(cx).id(), right_id);
        assert_eq!(right.read(cx).selected(), Some("right.txt"));
    })
    .unwrap();
    cx.executor().run_until_parked();
    assert_eq!(left.read_with(cx, |pane, _| pane.len()), 2);
    assert_eq!(
        right.read_with(cx, |pane, _| pane.selected().map(str::to_owned)),
        Some("right.txt".into())
    );
    cx.update_window(handle, |_, window, cx| {
        right.update(cx, |pane, cx| pane.focus_filter_input(window, cx));
        window.input("v * jk", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        right.read(cx).len() == 0
    })
    .await;
    assert_eq!(right.read_with(cx, |pane, _| pane.marked()), ["right.txt"]);
    assert!(!right.read_with(cx, |pane, _| pane.files.range_active()));
}

#[gpui_kit::test]
async fn lazy_local_trees_keep_marks_scope_visibility_and_cursor(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let left_root = tempfile::tempdir().unwrap();
    let right_root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir_all(left_root.path().join("src/nested")).unwrap();
    fs::write(left_root.path().join("src/nested/deep.txt"), "deep").unwrap();
    fs::write(left_root.path().join("src/a.txt"), "a").unwrap();
    fs::write(left_root.path().join("src/.hidden"), "hidden").unwrap();
    fs::create_dir(left_root.path().join("src/.secret")).unwrap();
    fs::write(left_root.path().join("src/.secret/inside.txt"), "secret").unwrap();
    fs::write(left_root.path().join("src/ignored.txt"), "ignored").unwrap();
    fs::write(left_root.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(left_root.path().join("top.txt"), "top").unwrap();
    fs::create_dir_all(left_root.path().join("src/node_modules")).unwrap();
    fs::write(right_root.path().join("other.txt"), "other").unwrap();
    let (handle, panes) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1200.), px(800.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let service = BrowserService::new().unwrap();
                    let store = Store::new(config.path().into());
                    let left = cx.new(|cx| {
                        BrowserPane::new(
                            store.clone(),
                            service.clone(),
                            left_root.path().into(),
                            window,
                            cx,
                        )
                    });
                    let right = cx.new(|cx| {
                        BrowserPane::new(
                            store,
                            service,
                            right_root.path().to_path_buf(),
                            window,
                            cx,
                        )
                    });
                    left.update(cx, |pane, cx| {
                        pane.open(left_root.path().into(), window, cx)
                    });
                    right.update(cx, |pane, cx| {
                        pane.open(right_root.path().to_path_buf(), window, cx)
                    });
                    TwoPanes { left, right }
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        let panes = panes.read(cx);
        panes.left.read(cx).location().is_some() && panes.right.read(cx).location().is_some()
    })
    .await;
    let (left, right) = panes.read_with(cx, |panes, _| (panes.left.clone(), panes.right.clone()));

    cx.update_window(handle, |_, window, cx| {
        assert_eq!(left.read(cx).len(), 2);
        window
            .within("left")
            .click(("local-tree-toggle", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !left.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            left.read(cx).location().unwrap().directory,
            left_root.path()
        );
        assert!(!left.read(cx).can_back());
        assert_eq!(left.read(cx).len(), 4);
        assert_eq!(right.read(cx).len(), 1);
        window.press("right", cx); // Expanded src selects its first child.
        assert_eq!(left.read(cx).selected(), Some("src/nested"));
        window.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !left.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.press("right", cx);
        assert_eq!(left.read(cx).selected(), Some("src/nested/deep.txt"));
        window.press("space", cx);
        window.press("left", cx); // Collapse parent; retain hidden child mark.
        assert_eq!(left.read(cx).selected(), Some("src/nested"));
        assert_eq!(left.read(cx).files.marked_descendants("src"), 1);
        assert_eq!(left.read(cx).len(), 4);
        window.press("right", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !left.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.press("right", cx);
        assert_eq!(left.read(cx).selected(), Some("src/nested/deep.txt"));
        left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Hidden, window, cx)
        });
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !left.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert!(left.read(cx).tree.node("src/nested").unwrap().expanded);
        assert_eq!(left.read(cx).selected(), Some("src/nested/deep.txt"));
        assert!(left.read(cx).tree.node("src/.hidden").is_some());
        assert!(left.read(cx).tree.node("src/ignored.txt").is_none());
        assert!(left.read(cx).tree.node("src/node_modules").is_none());
        left.update(cx, |pane, cx| pane.expand("src/.secret".into(), window, cx));
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !left.read(cx).is_loading()
    })
    .await;
    for _ in 0..2 {
        cx.update_window(handle, |_, window, cx| {
            left.update(cx, |pane, cx| {
                pane.command(BrowserCommand::Hidden, window, cx)
            });
        })
        .unwrap();
        cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
            !left.read(cx).is_loading()
        })
        .await;
    }
    cx.update_window(handle, |_, window, cx| {
        assert!(left.read(cx).tree.node("src/.secret").unwrap().expanded);
        assert!(left.read(cx).tree.node("src/.secret/inside.txt").is_some());
        left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Ignored, window, cx)
        });
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !left.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert!(left.read(cx).tree.node("src/ignored.txt").is_some());
        assert_eq!(left.read(cx).marked(), ["src/nested/deep.txt"]);
        assert!(right.read(cx).marked().is_empty());
        // A collapse during a pending listing invalidates its eventual result.
        left.update(cx, |pane, cx| {
            pane.toggle_tree("src/nested".into(), window, cx);
            pane.expand("src/nested".into(), window, cx);
            let cancel = pane.listing_cancel.as_ref().unwrap().clone();
            pane.toggle_tree("src/nested".into(), window, cx);
            assert!(cancel.is_cancelled());
        });
    })
    .unwrap();
    cx.executor().run_until_parked();
    assert!(!left.read_with(cx, |pane, _| pane.tree.node("src/nested").unwrap().expanded));
    assert_eq!(
        left.read_with(cx, |pane, _| pane.marked()),
        ["src/nested/deep.txt"]
    );
    fs::rename(
        left_root.path().join("src/nested"),
        left_root.path().join("moved"),
    )
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        left.update(cx, |pane, cx| pane.expand("src/nested".into(), window, cx));
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !left.read(cx).is_loading()
    })
    .await;
    assert!(!left.read_with(cx, |pane, _| pane.tree.node("src/nested").unwrap().expanded));
    assert_eq!(
        left.read_with(cx, |pane, _| pane.marked()),
        ["src/nested/deep.txt"]
    );
    assert_eq!(
        left.read_with(cx, |pane, _| pane.location().unwrap().directory.clone()),
        left_root.path()
    );
}

#[gpui_kit::test]
async fn finder_keyboard_focuses_query_and_bridges_to_results_without_preview(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("child")).unwrap();
    fs::write(root.path().join("child/needle.txt"), "needle").unwrap();
    fs::write(root.path().join("other.txt"), "other").unwrap();
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let service = BrowserService::new().unwrap();
                let store = Store::new(config.path().into());
                let left = cx.new(|cx| {
                    BrowserPane::new(
                        store.clone(),
                        service.clone(),
                        root.path().into(),
                        window,
                        cx,
                    )
                });
                let right =
                    cx.new(|cx| BrowserPane::new(store, service, root.path().into(), window, cx));
                left.update(cx, |pane, cx| pane.open(root.path().into(), window, cx));
                TwoPanes { left, right }
            })
        })
        .unwrap()
    });
    let pane = view.read_with(cx, |view, _| view.left.clone());
    let previews = std::rc::Rc::new(std::cell::Cell::new(0));
    let _subscription = cx.update(|cx| {
        let previews = previews.clone();
        cx.subscribe(&pane, move |_, event: &BrowserEvent, _| {
            if matches!(event, BrowserEvent::Selected { preview: true, .. }) {
                previews.set(previews.get() + 1);
            }
        })
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        pane.update(cx, |pane, cx| pane.focus(w, cx));
        w.press("home", cx);
        pane.update(cx, |pane, cx| pane.preview_selected(w, cx));
        assert!(!pane.read(cx).tree.node("child").unwrap().expanded);
        w.press("f", cx);
        assert!(pane.read(cx).filter.focus_handle(cx).is_focused(w));
        pane.update(cx, |pane, cx| pane.preview_selected(w, cx));
        w.input("needle", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        pane.read(cx).finder && !pane.read(cx).is_loading() && pane.read(cx).len() == 1
    })
    .await;
    for key in ["enter", "down"] {
        cx.update_window(handle, |_, w, cx| {
            let selected = pane.read(cx).selected().map(str::to_owned);
            let id = pane.read(cx).id();
            let marks = pane.read(cx).marked();
            let range = pane.read(cx).files.range_active();
            let scroll = pane.read(cx).scroll.0.borrow().base_handle.offset();
            w.press(key, cx);
            assert!(pane.read(cx).browser_focus.is_focused(w));
            assert_eq!(pane.read(cx).selected(), selected.as_deref());
            assert_eq!(pane.read(cx).filter.read(cx).value().as_str(), "needle");
            assert_eq!(pane.read(cx).id(), id);
            assert_eq!(pane.read(cx).marked(), marks);
            assert_eq!(pane.read(cx).files.range_active(), range);
            assert_eq!(pane.read(cx).scroll.0.borrow().base_handle.offset(), scroll);
            assert_eq!(previews.get(), 0);
            w.press("home", cx);
            assert_eq!(pane.read(cx).selected(), Some("child/needle.txt"));
            if key == "enter" {
                w.press("space", cx);
                w.press("v", cx);
                w.press("ctrl-f", cx);
            }
        })
        .unwrap();
    }
    cx.update_window(handle, |_, w, cx| {
        let selected = pane.read(cx).selected().map(str::to_owned);
        pane.update(cx, |pane, cx| pane.preview_selected(w, cx));
        assert_eq!(pane.read(cx).selected(), selected.as_deref());
        assert_eq!(pane.read(cx).marked(), ["child/needle.txt"]);
        assert!(pane.read(cx).files.range_active());
        w.press("ctrl-f", cx);
        w.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        w.input("missing", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        pane.read(cx).len() == 0
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        for key in ["enter", "down", "escape"] {
            w.press(key, cx);
            assert!(pane.read(cx).browser_focus.is_focused(w));
            assert!(pane.read(cx).selected().is_none());
            assert_eq!(pane.read(cx).filter.read(cx).value().as_str(), "missing");
            w.press("ctrl-f", cx);
        }
        assert_eq!(previews.get(), 1);
    })
    .unwrap();
}
