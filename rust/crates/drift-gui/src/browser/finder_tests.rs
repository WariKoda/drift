use super::*;
use crate::pane_split::{PaneSplit, SplitCommand};
use gpui_kit::InputEvent as _;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext,
    WindowBounds, WindowOptions, size,
};
use std::{cell::RefCell, fs, rc::Rc, sync::Arc, time::Duration};

const WAIT: Duration = Duration::from_secs(60);
const SELECT_ALL: &str = if cfg!(target_os = "macos") {
    "cmd-a"
} else {
    "ctrl-a"
};
const RETURN: &str = if cfg!(target_os = "macos") {
    "cmd-alt-f"
} else {
    "ctrl-alt-f"
};

// The root owns the action route, while the panes, headers, input, popup menus
// and resizable split are production elements in one native focus tree.
struct TwoPanes {
    left: Entity<BrowserPane>,
    right: Entity<BrowserPane>,
    split: PaneSplit,
    events: Rc<RefCell<Vec<&'static str>>>,
    statuses: Rc<RefCell<Vec<String>>>,
    _subscriptions: Vec<Subscription>,
}
impl Render for TwoPanes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let left = div()
            .id("left")
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
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
            .child(self.left.clone())
            .into_any_element();
        let right = div()
            .id("right")
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(self.right.read(cx).header())
            .child(self.right.clone())
            .into_any_element();
        let split = self.split.view("finder-split", left, right, window, cx);
        div()
            .key_context("Drift")
            .flex()
            .size_full()
            .capture_action(cx.listener(|this, _: &Cancel, w, cx| {
                this.split.cancel_resize(w, cx);
            }))
            .capture_action(cx.listener(|this, _: &ClearMarks, w, cx| {
                this.split.cancel_resize(w, cx);
            }))
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::actions::Cancel, w, cx| {
                    this.split.cancel_resize(w, cx);
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_kit::component::input::Escape, w, cx| {
                    this.split.cancel_resize(w, cx);
                }),
            )
            .capture_action(cx.listener(|this, _: &OpenContextMenu, _, cx| {
                if this.split.is_resizing() {
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if event.button == MouseButton::Right && this.split.is_resizing() {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &FocusFilter, w, cx| {
                this.left
                    .update(cx, |pane, cx| pane.focus_filter_input(w, cx));
            }))
            .on_action(cx.listener(|this, _: &FindFiles, w, cx| {
                this.left
                    .update(cx, |pane, cx| pane.command(BrowserCommand::Find, w, cx));
            }))
            .on_action(cx.listener(|this, _: &ReturnFromFinder, w, cx| {
                this.left.update(cx, |pane, cx| {
                    pane.command(BrowserCommand::ReturnFinder, w, cx)
                });
            }))
            .on_action(cx.listener(|this, _: &ResizePaneLeft, w, cx| {
                this.split.command(SplitCommand::Left, w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ResizePaneRight, w, cx| {
                this.split.command(SplitCommand::Right, w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ResetPaneSizes, w, cx| {
                this.split.command(SplitCommand::Reset, w, cx);
                cx.notify();
            }))
            .child(split)
    }
}

struct Fixture {
    handle: AnyWindowHandle,
    panes: Entity<TwoPanes>,
    left: Entity<BrowserPane>,
    right: Entity<BrowserPane>,
    events: Rc<RefCell<Vec<&'static str>>>,
    statuses: Rc<RefCell<Vec<String>>>,
    root: tempfile::TempDir,
    other: tempfile::TempDir,
    _config: tempfile::TempDir,
}
impl Fixture {
    async fn new(cx: &mut TestAppContext) -> Self {
        cx.executor().allow_parking();
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src/nested")).unwrap();
        fs::create_dir(root.path().join("closed")).unwrap();
        for name in [
            "alpha.txt",
            "bravo.txt",
            "charlie.txt",
            "ÜberStraße.rs",
            "src/a.txt",
            "src/nested/deep.txt",
            "src/nested/remove.txt",
            "closed/hidden-child.txt",
        ] {
            fs::write(root.path().join(name), name).unwrap();
        }
        for index in 0..140 {
            fs::write(root.path().join(format!("row-{index:03}.txt")), "row").unwrap();
        }
        fs::write(other.path().join("other.txt"), "other").unwrap();
        let events = Rc::new(RefCell::new(vec![]));
        let statuses = Rc::new(RefCell::new(vec![]));
        let (handle, panes) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::actions::bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1200.), px(700.)),
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
                                root.path().into(),
                                window,
                                cx,
                            )
                        });
                        let right = cx.new(|cx| {
                            BrowserPane::new(store, service, other.path().into(), window, cx)
                        });
                        let subscriptions = [left.clone(), right.clone()]
                            .into_iter()
                            .map(|pane| {
                                cx.subscribe(
                                    &pane,
                                    |this: &mut TwoPanes, _, event, _| match event {
                                        BrowserEvent::Selected { preview: true, .. } => {
                                            this.events.borrow_mut().push("preview")
                                        }
                                        BrowserEvent::Compare { .. } => {
                                            this.events.borrow_mut().push("compare")
                                        }
                                        BrowserEvent::Toolbar { .. } => {
                                            this.events.borrow_mut().push("toolbar")
                                        }
                                        BrowserEvent::Status { message, .. } => {
                                            this.statuses.borrow_mut().push(message.clone())
                                        }
                                        _ => {}
                                    },
                                )
                            })
                            .collect();
                        left.update(cx, |pane, cx| pane.open(root.path().into(), window, cx));
                        right.update(cx, |pane, cx| pane.open(other.path().into(), window, cx));
                        TwoPanes {
                            left,
                            right,
                            split: PaneSplit::new(None, cx),
                            events: events.clone(),
                            statuses: statuses.clone(),
                            _subscriptions: subscriptions,
                        }
                    })
                },
            )
            .unwrap()
        });
        let (left, right) =
            panes.read_with(cx, |panes, _| (panes.left.clone(), panes.right.clone()));
        let fixture = Self {
            handle,
            panes,
            left,
            right,
            events,
            statuses,
            root,
            other,
            _config: config,
        };
        fixture.idle(cx).await;
        fixture.events.borrow_mut().clear();
        fixture.statuses.borrow_mut().clear();
        fixture
    }

    async fn idle(&self, cx: &mut TestAppContext) {
        cx.wait_for(self.handle, WAIT, |_, cx| {
            self.left.read(cx).location().is_some()
                && self.right.read(cx).location().is_some()
                && !self.left.read(cx).is_loading()
                && !self.right.read(cx).is_loading()
        })
        .await;
    }

    async fn finder(&self, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, w, cx| {
            self.left.update(cx, |pane, cx| pane.focus(w, cx));
            w.press("f", cx);
            assert!(
                self.left.read(cx).finder,
                "Finder must be active during indexing"
            );
            assert!(self.left.read(cx).finder_snapshot.is_some());
            assert!(self.left.read(cx).filter.focus_handle(cx).is_focused(w));
            assert_eq!(self.left.read(cx).filter.read(cx).value().as_str(), "");
        })
        .unwrap();
        cx.wait_for(self.handle, WAIT, |_, cx| {
            self.left.read(cx).finder && !self.left.read(cx).is_loading()
        })
        .await;
    }

    async fn query(&self, query: &str, rows: usize, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, w, cx| {
            w.within("left").click("filter", cx);
            w.press(SELECT_ALL, cx);
            w.press("backspace", cx);
            if !query.is_empty() {
                w.input(query, cx);
            }
        })
        .unwrap();
        // InputEvent::Change is deferred. Never inspect or return the saved view
        // in the transaction that edits the input.
        cx.wait_for(self.handle, WAIT, |_, cx| {
            self.left.read(cx).filter.read(cx).value().as_str() == query
                && self.left.read(cx).len() == rows
        })
        .await;
    }

    fn finish_workers(&self, cx: &mut TestAppContext) {
        let service = self.left.read_with(cx, |pane, _| pane.service.clone());
        let (done, finished) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            service.wait_for_shutdown();
            done.send(()).unwrap();
        });
        finished
            .recv_timeout(WAIT)
            .expect("real browser workers did not finish");
    }

    async fn expand(&self, path: &str, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, w, cx| {
            self.left
                .update(cx, |pane, cx| pane.expand(path.into(), w, cx));
        })
        .unwrap();
        self.idle(cx).await;
        assert!(
            self.left
                .read_with(cx, |pane, _| pane.tree.node(path).unwrap().expanded)
        );
    }
}

// Capture the whole observable view, not only its row count. Marks are excluded
// deliberately: Finder edits must win over the snapshot's old marked set.
struct SavedView {
    rows: Vec<String>,
    nodes: Vec<(String, bool, usize, bool)>,
    expanded: Vec<String>,
    entries: Vec<(PathBuf, bool, bool, u64)>,
    location: Location,
    previous: Option<(usize, PathBuf)>,
    next: Option<(usize, PathBuf)>,
    query: String,
    selected: Option<String>,
    selected_row: Option<usize>,
    range: bool,
    hidden: bool,
    ignored: bool,
    scroll: UniformListScrollHandle,
    offset: Point<Pixels>,
}
impl SavedView {
    fn capture(pane: &BrowserPane, cx: &App) -> Self {
        Self {
            rows: (0..pane.len())
                .map(|i| pane.files.row(i).unwrap().into())
                .collect(),
            nodes: pane
                .tree
                .nodes()
                .iter()
                .map(|n| (n.path.clone(), n.directory, n.depth, n.expanded))
                .collect(),
            expanded: pane.tree.expanded_paths().cloned().collect(),
            entries: pane
                .entries
                .iter()
                .map(|e| (e.path.clone(), e.directory, e.symlink, e.size))
                .collect(),
            location: pane.location().unwrap().clone(),
            previous: pane.history.previous(),
            next: pane.history.next(),
            query: pane.filter.read(cx).value().to_string(),
            selected: pane.selected().map(str::to_owned),
            selected_row: pane.files.selected_row(),
            range: pane.files.range_active(),
            hidden: pane.show_hidden,
            ignored: pane.show_ignored,
            scroll: pane.scroll.clone(),
            offset: pane.scroll.0.borrow().base_handle.offset(),
        }
    }

    fn assert_restored(&self, pane: &BrowserPane, w: &Window, cx: &App) {
        assert!(pane.browser_focus.is_focused(w));
        self.assert_view_restored(pane, cx);
    }

    fn assert_view_restored(&self, pane: &BrowserPane, cx: &App) {
        assert!(!pane.finder);
        assert!(pane.finder_snapshot.is_none());
        assert!(!pane.is_loading());
        let now = Self::capture(pane, cx);
        assert_eq!(now.rows, self.rows);
        assert_eq!(now.nodes, self.nodes);
        assert_eq!(now.expanded, self.expanded);
        assert_eq!(now.entries, self.entries);
        assert!(Arc::ptr_eq(&now.location.root, &self.location.root));
        assert_eq!(now.location.directory, self.location.directory);
        assert_eq!(now.location.slug, self.location.slug);
        assert_eq!(now.previous, self.previous);
        assert_eq!(now.next, self.next);
        assert_eq!(now.query, self.query);
        assert_eq!(now.selected, self.selected);
        assert_eq!(now.selected_row, self.selected_row);
        assert_eq!(now.range, self.range);
        assert_eq!(now.hidden, self.hidden);
        assert_eq!(now.ignored, self.ignored);
        assert!(
            Rc::ptr_eq(&now.scroll.0, &self.scroll.0),
            "return must restore the original scroll handle, not a new handle at the same offset"
        );
        assert_eq!(now.offset, self.offset);
    }
}

#[gpui_kit::test]
async fn finder_return_restores_tree_filter_cursor_history_range_and_exact_scroll_with_live_marks(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    // A middle history entry has both backward and forward destinations.
    for path in [
        f.root.path().join("src"),
        f.root.path().into(),
        f.root.path().join("src"),
    ] {
        cx.update_window(f.handle, |_, w, cx| {
            f.left
                .update(cx, |pane, cx| pane.navigate(path, Navigation::Push, w, cx));
        })
        .unwrap();
        f.idle(cx).await;
    }
    cx.update_window(f.handle, |_, w, cx| {
        f.left.update(cx, |pane, cx| pane.focus(w, cx));
        w.press("alt-left", cx);
    })
    .unwrap();
    f.idle(cx).await;
    f.expand("src", cx).await;
    f.expand("src/nested", cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        f.left.update(cx, |pane, cx| {
            pane.files.select_path("src/nested/deep.txt");
            pane.focus(w, cx);
        });
        w.press("space", cx);
        f.left
            .update(cx, |pane, cx| pane.toggle_tree("src/nested".into(), w, cx));
        assert!(!f.left.read(cx).tree.node("src/nested").unwrap().expanded);
        assert!(!f.left.read(cx).tree.node("closed").unwrap().expanded);
        assert!(f.left.read(cx).tree.node("src/nested/deep.txt").is_none());
        assert_eq!(f.left.read(cx).files.marked_descendants("src/nested"), 1);
    })
    .unwrap();
    f.query(".txt", 144, cx).await;
    let (saved, right_saved, right_id) = cx
        .update_window(f.handle, |_, w, cx| {
            w.press("escape", cx);
            w.press("end", cx);
            assert_eq!(f.left.read(cx).selected(), Some("row-139.txt"));
            w.press("v", cx);
            w.press("up", cx);
            w.press("up", cx);
            w.render_frame(cx);
            assert_eq!(f.left.read(cx).selected(), Some("row-137.txt"));
            assert!(f.left.read(cx).scroll.0.borrow().base_handle.offset().y < px(0.));
            assert!(f.left.read(cx).can_back() && f.left.read(cx).can_forward());
            (
                SavedView::capture(f.left.read(cx), cx),
                SavedView::capture(f.right.read(cx), cx),
                f.right.read(cx).id(),
            )
        })
        .unwrap();
    f.finder(cx).await;
    for (query, path) in [
        ("deep.txt", "src/nested/deep.txt"),
        ("hidden-child.txt", "closed/hidden-child.txt"),
        ("remove.txt", "src/nested/remove.txt"),
    ] {
        f.query(query, 1, cx).await;
        cx.update_window(f.handle, |_, w, cx| {
            w.press("escape", cx);
            w.press("home", cx);
            assert_eq!(f.left.read(cx).selected(), Some(path));
            w.press("space", cx);
        })
        .unwrap();
    }
    // Re-entering Finder from results must focus, not rebuild or replace its snapshot.
    cx.update_window(f.handle, |_, w, cx| {
        let id = f.left.read(cx).id();
        w.press("f", cx);
        assert_eq!(f.left.read(cx).id(), id);
        assert_eq!(
            f.left.read(cx).filter.read(cx).value().as_str(),
            "remove.txt"
        );
        assert!(f.left.read(cx).filter.focus_handle(cx).is_focused(w));
        w.within("left").click("finder-return", cx);
        w.render_frame(cx);
        saved.assert_restored(f.left.read(cx), w, cx);
        assert_eq!(
            f.left.read(cx).marked(),
            ["closed/hidden-child.txt", "src/nested/remove.txt"]
        );
        assert_eq!(f.left.read(cx).files.marked_descendants("src/nested"), 1);
        assert_eq!(f.left.read(cx).files.marked_descendants("closed"), 1);
        assert_eq!(f.right.read(cx).id(), right_id);
        let right_now = SavedView::capture(f.right.read(cx), cx);
        assert_eq!(right_now.rows, right_saved.rows);
        assert_eq!(right_now.query, right_saved.query);
        assert_eq!(right_now.selected, right_saved.selected);
        assert!(Rc::ptr_eq(&right_now.scroll.0, &right_saved.scroll.0));
        assert!(f.right.read(cx).marked().is_empty());
        // Finishing the original interval checks the saved anchor, not just its flag.
        w.press("v", cx);
        assert!(!f.left.read(cx).files.range_active());
        assert_eq!(
            f.left.read(cx).marked(),
            [
                "closed/hidden-child.txt",
                "row-137.txt",
                "row-138.txt",
                "row-139.txt",
                "src/nested/remove.txt",
            ]
        );
    })
    .unwrap();
}

#[gpui_kit::test]
async fn finder_indexing_return_cancels_token_and_repeated_f_preserves_original_snapshot(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    f.query("bravo", 1, cx).await;
    let (saved, cancelled_id) = cx
        .update_window(f.handle, |_, w, cx| {
            w.press("escape", cx);
            w.press("home", cx);
            let saved = SavedView::capture(f.left.read(cx), cx);
            f.left
                .update(cx, |pane, cx| pane.command(BrowserCommand::Find, w, cx));
            let id = f.left.read(cx).id();
            let token = f.left.read(cx).listing_cancel.as_ref().unwrap().clone();
            assert!(f.left.read(cx).finder);
            assert!(f.left.read(cx).finder_snapshot.is_some());
            assert_eq!(f.left.read(cx).filter.read(cx).value().as_str(), "");
            f.left.update(cx, |pane, cx| pane.focus(w, cx));
            w.press("f", cx);
            assert_eq!(f.left.read(cx).id(), id);
            assert!(!token.is_cancelled(), "f must not restart the active index");
            assert!(f.left.read(cx).filter.focus_handle(cx).is_focused(w));
            w.press(RETURN, cx);
            assert!(token.is_cancelled());
            assert_ne!(f.left.read(cx).id(), id);
            saved.assert_restored(f.left.read(cx), w, cx);
            (saved, id)
        })
        .unwrap();
    // Join real workers before draining the GUI executor, so a late callback is
    // actually processed before the final assertions rather than left pending.
    f.finish_workers(cx);
    cx.executor().run_until_parked();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        saved.assert_restored(f.left.read(cx), w, cx);
        assert_ne!(f.left.read(cx).id(), cancelled_id);
    })
    .unwrap();
}

#[gpui_kit::test]
async fn finder_completed_index_callback_cannot_overwrite_native_return(cx: &mut TestAppContext) {
    let f = Fixture::new(cx).await;
    f.query("charlie", 1, cx).await;
    let saved = cx
        .update_window(f.handle, |_, w, cx| {
            w.press("escape", cx);
            w.press("home", cx);
            let saved = SavedView::capture(f.left.read(cx), cx);
            f.left
                .update(cx, |pane, cx| pane.command(BrowserCommand::Find, w, cx));
            saved
        })
        .unwrap();
    // Complete the real index without polling GPUI's awaiting callback. Return
    // then invalidates a successful result, not merely an early cancellation.
    f.finish_workers(cx);
    cx.update_window(f.handle, |_, w, cx| {
        assert!(f.left.read(cx).is_loading());
        w.press(RETURN, cx);
        saved.assert_restored(f.left.read(cx), w, cx);
    })
    .unwrap();
    cx.executor().run_until_parked();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        saved.assert_restored(f.left.read(cx), w, cx);
    })
    .unwrap();
}

#[gpui_kit::test]
async fn finder_unicode_fuzzy_empty_and_nonmatching_queries_do_not_preview_or_transfer(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    f.query("ÜSR", 0, cx).await;
    f.query("straße", 1, cx).await;
    assert_eq!(
        f.left
            .read_with(cx, |pane, _| pane.files.row(0).map(str::to_owned)),
        Some("ÜberStraße.rs".into())
    );
    let saved = f.left.read_with(cx, SavedView::capture);
    f.finder(cx).await;
    assert_eq!(f.left.read_with(cx, |pane, _| pane.len()), 148);
    let source_order = f.left.read_with(cx, |pane, _| {
        (0..pane.len())
            .map(|i| pane.files.row(i).unwrap().to_owned())
            .collect::<Vec<_>>()
    });
    f.query("ÜSR", 1, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(f.left.read(cx).files.row(0), Some("ÜberStraße.rs"));
        let id = f.left.read(cx).id();
        w.press("enter", cx); // Filter -> results, not activate/preview.
        assert!(f.left.read(cx).browser_focus.is_focused(w));
        assert_eq!(f.left.read(cx).id(), id);
        w.press("home", cx);
        w.press("space", cx);
        w.press("v", cx);
        w.press("ctrl-f", cx);
    })
    .unwrap();
    f.query("not-a-match-雪", 0, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let id = f.left.read(cx).id();
        let marks = f.left.read(cx).marked();
        let range = f.left.read(cx).files.range_active();
        for key in ["enter", "down", "escape"] {
            f.left.update(cx, |pane, cx| pane.focus_filter_input(w, cx));
            w.press(key, cx);
            assert!(f.left.read(cx).browser_focus.is_focused(w));
            assert!(f.left.read(cx).selected().is_none());
            assert_eq!(
                f.left.read(cx).filter.read(cx).value().as_str(),
                "not-a-match-雪"
            );
            assert_eq!(f.left.read(cx).id(), id);
            assert_eq!(f.left.read(cx).marked(), marks);
            assert_eq!(f.left.read(cx).files.range_active(), range);
            assert!(f.left.read(cx).finder_snapshot.is_some());
        }
    })
    .unwrap();
    f.query("", 148, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(
            (0..f.left.read(cx).len())
                .map(|i| f.left.read(cx).files.row(i).unwrap().to_owned())
                .collect::<Vec<_>>(),
            source_order
        );
        assert_eq!(f.left.read(cx).marked(), ["ÜberStraße.rs"]);
        w.press(RETURN, cx);
        saved.assert_restored(f.left.read(cx), w, cx);
        assert_eq!(f.left.read(cx).marked(), ["ÜberStraße.rs"]);
    })
    .unwrap();
    assert!(
        f.events.borrow().is_empty(),
        "implicit work: {:?}",
        f.events.borrow()
    );
}

#[gpui_kit::test]
async fn finder_navigation_reload_new_root_and_invalidation_cannot_resurrect_saved_project(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    f.expand("src", cx).await;
    f.query("alpha", 1, cx).await;
    f.finder(cx).await;
    f.query("deep", 1, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.press("escape", cx);
        w.press("home", cx);
        w.press("space", cx);
        w.press("r", cx);
        assert!(!f.left.read(cx).finder);
        assert!(f.left.read(cx).finder_snapshot.is_none());
        assert_eq!(f.left.read(cx).filter.read(cx).value().as_str(), "alpha");
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, _, cx| {
        assert_eq!(f.left.read(cx).len(), 1);
        assert_eq!(f.left.read(cx).marked(), ["src/nested/deep.txt"]);
        assert!(f.left.read(cx).tree.node("src").unwrap().expanded);
    })
    .unwrap();
    f.finder(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        f.left.update(cx, |pane, cx| {
            pane.navigate(f.root.path().join("src"), Navigation::Push, w, cx)
        });
        assert!(!f.left.read(cx).finder);
        assert!(f.left.read(cx).finder_snapshot.is_none());
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(
            f.left.read(cx).location().unwrap().directory,
            f.root.path().join("src")
        );
        assert!(f.left.read(cx).can_back());
        let id = f.left.read(cx).id();
        w.press(RETURN, cx);
        assert_eq!(f.left.read(cx).id(), id);
        assert_eq!(
            f.left.read(cx).location().unwrap().directory,
            f.root.path().join("src")
        );
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        f.left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Find, w, cx);
            let token = pane.listing_cancel.as_ref().unwrap().clone();
            pane.open(f.other.path().into(), w, cx);
            assert!(token.is_cancelled());
        });
        assert!(f.left.read(cx).finder_snapshot.is_none());
        assert!(!f.left.read(cx).finder);
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.press(RETURN, cx);
        assert_eq!(
            f.left.read(cx).location().unwrap().root.base(),
            f.other.path()
        );
        assert_eq!(f.left.read(cx).files.row(0), Some("other.txt"));
        assert_eq!(f.left.read(cx).len(), 1);
        assert!(f.left.read(cx).marked().is_empty());
        assert!(!f.left.read(cx).can_back() && !f.left.read(cx).can_forward());
        f.left.update(cx, |pane, cx| pane.focus(w, cx));
        w.press("home", cx);
        w.press("space", cx);
        assert_eq!(f.left.read(cx).marked(), ["other.txt"]);
        f.left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Find, w, cx);
            assert!(pane.finder_snapshot.is_some());
            pane.invalidate_project(cx);
        });
        assert!(f.left.read(cx).finder_snapshot.is_none());
        assert!(!f.left.read(cx).finder);
        w.press(RETURN, cx);
        assert!(f.left.read(cx).location().is_none());
        assert_eq!(f.left.read(cx).len(), 0);
        assert!(f.left.read(cx).marked().is_empty());
    })
    .unwrap();
    f.finish_workers(cx);
    cx.executor().run_until_parked();
    assert!(f.left.read_with(cx, |pane, _| pane.location().is_none()
        && pane.len() == 0
        && pane.marked().is_empty()
        && pane.finder_snapshot.is_none()));
}

#[gpui_kit::test]
async fn finder_busy_browser_rejects_entry_and_return_routes_preserve_escape_resize_menu_priority(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        f.left.update(cx, |pane, cx| {
            pane.expand("src".into(), w, cx);
            let id = pane.id();
            let token = pane.listing_cancel.as_ref().unwrap().clone();
            pane.command(BrowserCommand::Find, w, cx);
            assert_eq!(pane.id(), id);
            assert!(!token.is_cancelled());
            assert!(!pane.finder);
            assert!(pane.finder_snapshot.is_none());
        });
    })
    .unwrap();
    f.idle(cx).await;
    // Both native return routes must work from both focus destinations.
    for from_filter in [true, false] {
        for button in [true, false] {
            let saved = f.left.read_with(cx, SavedView::capture);
            f.finder(cx).await;
            f.query("alpha", 1, cx).await;
            cx.update_window(f.handle, |_, w, cx| {
                let id = f.left.read(cx).id();
                w.press("escape", cx);
                assert!(f.left.read(cx).finder);
                assert!(f.left.read(cx).browser_focus.is_focused(w));
                assert_eq!(f.left.read(cx).id(), id);
                assert_eq!(f.left.read(cx).filter.read(cx).value().as_str(), "alpha");
                if from_filter {
                    w.within("left").click("filter", cx);
                }
                if button {
                    w.within("left").click("finder-return", cx);
                } else {
                    w.press(RETURN, cx);
                }
                w.render_frame(cx);
                saved.assert_restored(f.left.read(cx), w, cx);
                assert!(w.within("left").try_find("finder-return").is_none());
            })
            .unwrap();
        }
    }
    f.finder(cx).await;
    f.query("alpha", 1, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        let id = f.left.read(cx).id();
        let before = w
            .within("finder-split")
            .find("split-first")
            .bounds()
            .size
            .width;
        w.press("ctrl-alt-left", cx);
        let after = w
            .within("finder-split")
            .find("split-first")
            .bounds()
            .size
            .width;
        assert!(after < before);
        assert!(f.left.read(cx).filter.focus_handle(cx).is_focused(w));
        assert_eq!(f.left.read(cx).id(), id);
        assert_eq!(f.left.read(cx).filter.read(cx).value().as_str(), "alpha");
        let from = w
            .within("finder-split")
            .find("pane-divider")
            .bounds()
            .center();
        let focus = w.focused(cx);
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
        w.render_frame(cx);
        w.dispatch_event(
            MouseMoveEvent {
                position: point(from.x + px(60.), from.y),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        w.render_frame(cx);
        assert!(f.panes.read(cx).split.is_resizing());
        w.press("escape", cx);
        assert!(!f.panes.read(cx).split.is_resizing());
        assert!(!cx.has_active_drag());
        assert_eq!(
            w.focused(cx),
            focus,
            "resize Escape must not bridge the filter"
        );
        assert_eq!(f.left.read(cx).id(), id);
        assert!(f.left.read(cx).finder);
        assert_eq!(f.left.read(cx).filter.read(cx).value().as_str(), "alpha");
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
        w.press("escape", cx);
        w.press("home", cx);
        w.press("menu", cx);
        assert_eq!(w.find("popup-menu").focused(), Some(true));
        let selected = f.left.read(cx).selected().map(str::to_owned);
        let marks = f.left.read(cx).marked();
        for key in [RETURN, "ctrl-alt-left", "f", "space", "v"] {
            w.press(key, cx);
            assert_eq!(w.find("popup-menu").focused(), Some(true));
            assert_eq!(f.left.read(cx).id(), id);
            assert!(f.left.read(cx).finder);
            assert!(f.left.read(cx).finder_snapshot.is_some());
            assert_eq!(f.left.read(cx).selected(), selected.as_deref());
            assert_eq!(f.left.read(cx).marked(), marks);
        }
        w.press("escape", cx);
        assert!(f.left.read(cx).finder);
        assert!(f.left.read(cx).browser_focus.is_focused(w));
        w.press(RETURN, cx);
        assert!(!f.left.read(cx).finder);
        assert!(f.left.read(cx).finder_snapshot.is_none());
    })
    .unwrap();
    assert!(f.events.borrow().is_empty());
}

#[cfg(unix)]
#[gpui_kit::test]
async fn finder_walk_failure_restores_snapshot_before_reporting_error(cx: &mut TestAppContext) {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let f = Fixture::new(cx).await;
    f.query("bravo", 1, cx).await;
    let saved = cx
        .update_window(f.handle, |_, w, cx| {
            w.press("escape", cx);
            w.press("home", cx);
            w.press("v", cx);
            SavedView::capture(f.left.read(cx), cx)
        })
        .unwrap();
    // An actual unreadable listing, independent of root privileges and chmod.
    // It is nested so the already-loaded ordinary view remains valid.
    fs::write(
        f.root
            .path()
            .join("closed")
            .join(OsString::from_vec(vec![0xff])),
        "bad name",
    )
    .unwrap();
    f.statuses.borrow_mut().clear();
    cx.update_window(f.handle, |_, w, cx| {
        f.left
            .update(cx, |pane, cx| pane.command(BrowserCommand::Find, w, cx));
        assert!(f.left.read(cx).finder);
    })
    .unwrap();
    cx.wait_for(f.handle, WAIT, |_, cx| {
        !f.left.read(cx).is_loading() && !f.left.read(cx).finder
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        saved.assert_view_restored(f.left.read(cx), cx);
        assert!(f.left.read(cx).filter.focus_handle(cx).is_focused(w));
        assert!(f.left.read(cx).marked().is_empty());
    })
    .unwrap();
    assert!(
        f.statuses.borrow().last().unwrap().contains("UTF-8"),
        "restoration must not overwrite the failure status: {:?}",
        f.statuses.borrow()
    );
    assert!(f.events.borrow().is_empty());
}

#[gpui_kit::test]
async fn finder_native_return_button_and_narrow_header_preserve_file_cursor(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx).await;
    for key in ["enter", "space"] {
        f.finder(cx).await;
        f.query("alpha", 1, cx).await;
        cx.update_window(f.handle, |_, w, cx| {
            w.press("enter", cx);
            w.press("home", cx);
            let selected = f.left.read(cx).selected().map(str::to_owned);
            let marks = f.left.read(cx).marked();
            w.within("right").click("filter", cx);
            for _ in 0..8 {
                w.press("shift-tab", cx);
                if w.within("left").find("finder-return").focused() == Some(true) {
                    break;
                }
            }
            assert_eq!(w.within("left").find("finder-return").focused(), Some(true));
            w.press("down", cx);
            assert_eq!(f.left.read(cx).selected(), selected.as_deref());
            assert_eq!(f.left.read(cx).marked(), marks);
            w.press(key, cx);
            assert!(!f.left.read(cx).finder);
            assert!(f.left.read(cx).browser_focus.is_focused(w));
        })
        .unwrap();
    }
    f.finder(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        for _ in 0..30 {
            w.press("ctrl-alt-left", cx);
        }
        let pane = w.within("left").find("local-files-pane").bounds();
        let button = w.within("left").find("finder-return").bounds();
        assert!(button.size.width <= pane.size.width);
        assert!(button.right() <= pane.right());
    })
    .unwrap();
    assert!(
        f.events.borrow().is_empty(),
        "button must not preview or compare"
    );
}
