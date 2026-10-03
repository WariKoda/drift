use super::*;
use gpui_kit::InputEvent as _;
use gpui_kit::base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, deferred, size,
};
use std::{cell::RefCell, fs, rc::Rc, time::Duration};

#[derive(Debug, PartialEq)]
enum ObservedEvent {
    Selected(&'static str, Option<PathBuf>, bool),
    Compare(&'static str),
    Toolbar(&'static str),
}

struct TwoPanes {
    left: Entity<BrowserPane>,
    right: Entity<BrowserPane>,
    retained_popup: Option<Entity<PopupMenu>>,
    events: Rc<RefCell<Vec<ObservedEvent>>>,
    _subscriptions: Vec<Subscription>,
}
impl Render for TwoPanes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Drift")
            .relative()
            .flex()
            .size_full()
            .child(
                div()
                    .id("left")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(self.left.read(cx).header())
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
            .children(self.retained_popup.as_ref().map(|popup| {
                // Render the original production entity to dispatch its captured
                // callback: PopupMenu exposes neither its items nor Confirm handler.
                deferred(
                    div()
                        .id("retained-popup")
                        .absolute()
                        .top(px(100.))
                        .right(px(8.))
                        .child(popup.clone()),
                )
                .with_priority(gpui_kit::base::POPUP_PRIORITY + 1)
            }))
    }
}

struct Fixture {
    handle: AnyWindowHandle,
    panes: Entity<TwoPanes>,
    left: Entity<BrowserPane>,
    right: Entity<BrowserPane>,
    events: Rc<RefCell<Vec<ObservedEvent>>>,
    left_root: tempfile::TempDir,
    _right_root: tempfile::TempDir,
    _config: tempfile::TempDir,
}
impl Fixture {
    async fn new(cx: &mut TestAppContext) -> Self {
        cx.executor().allow_parking();
        let left_root = tempfile::tempdir().unwrap();
        let right_root = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        fs::create_dir_all(left_root.path().join("src/nested")).unwrap();
        for (name, contents) in [
            ("alpha.txt", "alpha"),
            ("bravo.txt", "bravo"),
            ("charlie.txt", "charlie"),
            ("src/a.txt", "a"),
            ("src/nested/deep.txt", "deep"),
            (".hidden", "hidden"),
        ] {
            fs::write(left_root.path().join(name), contents).unwrap();
        }
        fs::write(right_root.path().join("other.txt"), "other").unwrap();
        fs::write(right_root.path().join("spare.txt"), "spare").unwrap();
        let events = Rc::new(RefCell::new(vec![]));
        let (handle, panes) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::actions::bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1200.), px(1000.)),
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
                            BrowserPane::new(store, service, right_root.path().into(), window, cx)
                        });
                        let subscriptions = [("left", &left), ("right", &right)]
                            .into_iter()
                            .map(|(name, pane)| {
                                cx.subscribe(pane, move |this: &mut TwoPanes, _, event, _| {
                                    let event = match event {
                                        BrowserEvent::Selected { path, preview, .. } => {
                                            ObservedEvent::Selected(name, path.clone(), *preview)
                                        }
                                        BrowserEvent::Compare { .. } => {
                                            ObservedEvent::Compare(name)
                                        }
                                        BrowserEvent::Toolbar { .. } => {
                                            ObservedEvent::Toolbar(name)
                                        }
                                        _ => return,
                                    };
                                    this.events.borrow_mut().push(event);
                                })
                            })
                            .collect();
                        left.update(cx, |pane, cx| {
                            pane.open(left_root.path().into(), window, cx)
                        });
                        right.update(cx, |pane, cx| {
                            pane.open(right_root.path().into(), window, cx)
                        });
                        TwoPanes {
                            left,
                            right,
                            retained_popup: None,
                            events: events.clone(),
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
            left_root,
            _right_root: right_root,
            _config: config,
        };
        fixture.wait_idle(cx).await;
        cx.update_window(handle, |_, window, cx| {
            let connection = fixture.right.read(cx).id();
            fixture.left.update(cx, |pane, cx| {
                pane.set_comparison_context(Some(connection), window, cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
        fixture.events.borrow_mut().clear();
        fixture
    }

    async fn wait_idle(&self, cx: &mut TestAppContext) {
        cx.wait_for(self.handle, Duration::from_secs(60), |_, cx| {
            self.left.read(cx).location().is_some()
                && self.right.read(cx).location().is_some()
                && !self.left.read(cx).is_loading()
                && !self.right.read(cx).is_loading()
        })
        .await;
    }

    async fn filter_left(&self, query: &str, expected_rows: usize, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, window, cx| {
            window.within("left").click("filter", cx);
            self.left
                .update(cx, |pane, cx| pane.focus_filter_input(window, cx));
            window.press("ctrl-a", cx);
            window.input(query, cx);
        })
        .unwrap();
        cx.wait_for(self.handle, Duration::from_secs(60), |_, cx| {
            self.left.read(cx).filter.read(cx).value().as_str() == query
                && self.left.read(cx).len() == expected_rows
        })
        .await;
    }

    fn replay_confirm(&self, popup: Entity<PopupMenu>, label: &str, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, window, cx| {
            self.panes.update(cx, |panes, cx| {
                panes.retained_popup = Some(popup.clone());
                cx.notify();
            });
            popup.focus_handle(cx).focus(window, cx);
            window.render_frame(cx);
            assert_eq!(
                window.within("retained-popup").find("popup-menu").focused(),
                Some(true)
            );
            let item = snapshots(window)
                .into_iter()
                .find(|element| {
                    element.path().contains(&"retained-popup".into())
                        && element.label() == Some(label)
                })
                .unwrap_or_else(|| panic!("retained popup lost {label:?}"));
            let id = item.path().last().unwrap().clone();
            window
                .within("retained-popup")
                .within("popup-menu")
                .hover(id.clone(), cx);
            assert_eq!(
                window
                    .within("retained-popup")
                    .within("popup-menu")
                    .find(id)
                    .selected(),
                Some(true),
                "the old callback must actually be selected before Confirm"
            );
            window.press("enter", cx);
            self.panes.update(cx, |panes, cx| {
                panes.retained_popup = None;
                cx.notify();
            });
            window.render_frame(cx);
        })
        .unwrap();
    }
}

fn menu_item(window: &Window, label: &str) -> ElementSnapshot {
    let items: Vec<_> = snapshots(window)
        .into_iter()
        .filter(|element| {
            element.path().contains(&"popup-menu".into()) && element.label() == Some(label)
        })
        .collect();
    assert_eq!(items.len(), 1, "expected one popup item labelled {label:?}");
    items.into_iter().next().unwrap()
}

fn click_item(window: &mut Window, label: &str, cx: &mut App) {
    window.render_frame(cx);
    let item = menu_item(window, label);
    assert_ne!(item.disabled(), Some(true), "{label} was disabled");
    window
        .within("popup-menu")
        .click(item.path().last().unwrap().clone(), cx);
}

fn popup(pane: &Entity<BrowserPane>, cx: &App) -> Entity<PopupMenu> {
    pane.read(cx).menu.as_ref().unwrap().1.popup.clone()
}

#[gpui_kit::test]
async fn row_menu_changes_only_cursor_and_escape_restores_clicked_pane(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    fixture.filter_left("txt", 3, cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.left.focus_handle(cx).focus(window, cx);
        window.press("home", cx);
        window.press("space", cx);
        window.press("v", cx);
        window.within("right").click("filter", cx);
        assert!(
            fixture
                .right
                .read(cx)
                .filter
                .focus_handle(cx)
                .is_focused(window)
        );
    })
    .unwrap();
    fixture.events.borrow_mut().clear();
    cx.update_window(fixture.handle, |_, window, cx| {
        let id = fixture.left.read(cx).id();
        window
            .within("left")
            .within("local-files")
            .right_click(2usize, cx);
        let pane = fixture.left.read(cx);
        assert_eq!(pane.selected(), Some("charlie.txt"));
        assert_eq!(pane.marked(), ["alpha.txt"]);
        assert!(pane.files.range_active());
        assert_eq!(pane.filter.read(cx).value().as_str(), "txt");
        assert_eq!(pane.id(), id);
        assert_eq!(window.find("popup-menu").focused(), Some(true));
        assert_ne!(menu_item(window, "Mark").checked(), Some(true));
        assert_ne!(
            menu_item(window, "Compare this file/folder").disabled(),
            Some(true)
        );
        window.press("escape", cx);
        let pane = fixture.left.read(cx);
        assert!(pane.browser_focus.is_focused(window));
        assert!(
            !fixture
                .right
                .read(cx)
                .filter
                .focus_handle(cx)
                .is_focused(window)
        );
        assert_eq!(pane.id(), id);
        assert_eq!(pane.marked(), ["alpha.txt"]);
        assert!(pane.files.range_active());
        assert_eq!(pane.filter.read(cx).value().as_str(), "txt");
    })
    .unwrap();
    assert!(fixture.left.read_with(cx, |pane, _| pane.menu.is_none()));
    assert!(
        fixture.events.borrow().is_empty(),
        "right click/Escape emitted a browser event"
    );
    cx.update_window(fixture.handle, |_, window, cx| {
        // The token is captured and Escape dispatched before the real Find
        // completion can enter the UI executor.
        fixture.left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Find, window, cx)
        });
        let cancel = fixture
            .left
            .read(cx)
            .listing_cancel
            .as_ref()
            .unwrap()
            .clone();
        let busy_id = fixture.left.read(cx).id();
        fixture
            .right
            .update(cx, |pane, cx| pane.focus_filter_input(window, cx));
        fixture.left.update(cx, |pane, cx| {
            pane.open_context_menu(
                Some("charlie.txt".into()),
                point(px(250.), px(200.)),
                window,
                cx,
            );
        });
        window.press("escape", cx);
        let pane = fixture.left.read(cx);
        assert!(pane.browser_focus.is_focused(window));
        assert_eq!(pane.id(), busy_id);
        assert!(pane.is_loading());
        assert!(!cancel.is_cancelled());
        assert!(pane.files.range_active());
        assert_eq!(pane.marked(), ["alpha.txt"]);
        assert_eq!(pane.filter.read(cx).value().as_str(), "txt");
        fixture.left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Cancel, window, cx)
        });
    })
    .unwrap();
    assert!(fixture.left.read_with(cx, |pane, _| pane.menu.is_none()));
    cx.executor().run_until_parked();
    assert!(
        fixture
            .events
            .borrow()
            .iter()
            .all(|event| matches!(event, ObservedEvent::Selected(_, _, false)))
    );
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.left.focus_handle(cx).focus(window, cx);
        window.press("v", cx);
        assert_eq!(
            fixture.left.read(cx).marked(),
            ["alpha.txt", "bravo.txt", "charlie.txt"]
        );
        assert!(!fixture.left.read(cx).files.range_active());
        assert_eq!(
            fixture.left.read(cx).filter.read(cx).value().as_str(),
            "txt"
        );
        assert!(fixture.right.read(cx).marked().is_empty());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn right_pane_menu_restores_right_focus_not_the_left_filter(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    fixture.filter_left("alpha", 1, cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        window.within("right").click("filter", cx);
        window.input("other", cx);
    })
    .unwrap();
    cx.wait_for(fixture.handle, Duration::from_secs(60), |_, cx| {
        fixture.right.read(cx).len() == 1
    })
    .await;
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.right.focus_handle(cx).focus(window, cx);
        window.press("home", cx);
        window.press("space", cx);
        window.press("v", cx);
        window.within("left").click("filter", cx);
        assert!(
            fixture
                .left
                .read(cx)
                .filter
                .focus_handle(cx)
                .is_focused(window)
        );
    })
    .unwrap();
    fixture.events.borrow_mut().clear();
    cx.update_window(fixture.handle, |_, window, cx| {
        let id = fixture.right.read(cx).id();
        window
            .within("right")
            .within("local-files")
            .right_click(0usize, cx);
        assert_eq!(window.find("popup-menu").focused(), Some(true));
        assert_eq!(menu_item(window, "Unmark").label(), Some("Unmark"));
        window.press("escape", cx);
        let pane = fixture.right.read(cx);
        assert!(pane.browser_focus.is_focused(window));
        assert!(
            !fixture
                .left
                .read(cx)
                .filter
                .focus_handle(cx)
                .is_focused(window)
        );
        assert_eq!(pane.id(), id);
        assert_eq!(pane.selected(), Some("other.txt"));
        assert_eq!(pane.marked(), ["other.txt"]);
        assert!(pane.files.range_active());
        assert_eq!(pane.filter.read(cx).value().as_str(), "other");
        assert_eq!(
            fixture.left.read(cx).filter.read(cx).value().as_str(),
            "alpha"
        );
        assert!(fixture.left.read(cx).marked().is_empty());
    })
    .unwrap();
    assert!(fixture.right.read_with(cx, |pane, _| pane.menu.is_none()));
    assert!(fixture.events.borrow().is_empty());
}

#[gpui_kit::test]
async fn keyboard_menu_blocks_browser_shortcuts_and_down_enter_runs_real_item(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.left.focus_handle(cx).focus(window, cx);
        window.press("home", cx);
        window.press("j", cx);
        assert_eq!(fixture.left.read(cx).selected(), Some("alpha.txt"));
        window.press("v", cx);
        cx.write_to_clipboard(ClipboardItem::new_string("untouched".into()));
    })
    .unwrap();
    fixture.events.borrow_mut().clear();
    cx.update_window(fixture.handle, |_, window, cx| {
        let id = fixture.left.read(cx).id();
        window.press("shift-f10", cx);
        assert_eq!(window.find("popup-menu").focused(), Some(true));
        assert_eq!(menu_item(window, "Preview").disabled(), None);
        for key in [
            "j",
            "k",
            "space",
            "v",
            "shift-v",
            "*",
            "s",
            "r",
            "f",
            ".",
            "shift-i",
            "alt-left",
            "alt-right",
            "alt-up",
            "backspace",
            "home",
            "end",
            "ctrl-f",
            "f5",
            "ctrl-shift-c",
            "shift-f10",
            "menu",
        ] {
            window.press(key, cx);
            let pane = fixture.left.read(cx);
            assert_eq!(pane.id(), id, "{key} started a browser operation");
            assert_eq!(
                pane.selected(),
                Some("alpha.txt"),
                "{key} moved the browser cursor"
            );
            assert!(pane.marked().is_empty(), "{key} changed marks");
            assert!(pane.files.range_active(), "{key} changed the range");
            assert!(!pane.shows_hidden());
            assert!(!pane.shows_ignored());
            assert_eq!(window.find("popup-menu").focused(), Some(true));
        }
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("untouched")
        );
        window.press("down", cx); // Preview.
        window.press("down", cx); // Mark, skipping the separator.
        window.press("enter", cx);
        assert!(fixture.left.read(cx).menu.is_none());
        assert_eq!(fixture.left.read(cx).marked(), ["alpha.txt"]);
        assert!(fixture.left.read(cx).browser_focus.is_focused(window));
        window.press("menu", cx);
        assert_eq!(menu_item(window, "Unmark").label(), Some("Unmark"));
        assert!(fixture.left.read(cx).files.is_marked("alpha.txt"));
        let menu_id = popup(&fixture.left, cx).entity_id();
        for key in ["tab", "shift-tab"] {
            window.press(key, cx);
            assert_eq!(window.find("popup-menu").focused(), Some(true));
            assert_eq!(fixture.left.read(cx).selected(), Some("alpha.txt"));
            assert_eq!(fixture.left.read(cx).marked(), ["alpha.txt"]);
            assert!(fixture.right.read(cx).marked().is_empty());
        }
        let unmark = menu_item(window, "Unmark");
        window
            .within("popup-menu")
            .right_click(unmark.path().last().unwrap().clone(), cx);
        assert_eq!(popup(&fixture.left, cx).entity_id(), menu_id);
        assert_eq!(fixture.left.read(cx).marked(), ["alpha.txt"]);
        window.press("escape", cx);
    })
    .unwrap();
    assert!(
        fixture.events.borrow().is_empty(),
        "menu keys leaked browser events: {:?}",
        fixture.events.borrow()
    );
    assert!(
        fixture
            .right
            .read_with(cx, |pane, _| pane.selected().is_none())
    );
}

#[gpui_kit::test]
async fn outside_click_dismisses_and_focuses_the_clicked_control(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    for target in ["row", "filter"] {
        cx.update_window(fixture.handle, |_, window, cx| {
            window
                .within("left")
                .within("local-files")
                .right_click(1usize, cx);
            assert_eq!(fixture.left.read(cx).selected(), Some("alpha.txt"));
            if target == "row" {
                window
                    .within("right")
                    .within("local-files")
                    .click(0usize, cx);
                assert!(fixture.right.read(cx).browser_focus.is_focused(window));
                assert_eq!(fixture.right.read(cx).selected(), Some("other.txt"));
            } else {
                window.within("right").click("filter", cx);
                assert!(
                    fixture
                        .right
                        .read(cx)
                        .filter
                        .focus_handle(cx)
                        .is_focused(window)
                );
            }
            assert!(!fixture.left.read(cx).browser_focus.is_focused(window));
            assert_eq!(fixture.left.read(cx).selected(), Some("alpha.txt"));
        })
        .unwrap();
        assert!(fixture.left.read_with(cx, |pane, _| pane.menu.is_none()));
    }
}

#[gpui_kit::test]
async fn background_menu_has_no_row_actions_or_stale_cursor_target(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.left.focus_handle(cx).focus(window, cx);
        window.press("end", cx);
        window.press("space", cx);
        assert_eq!(fixture.left.read(cx).selected(), Some("charlie.txt"));
        let row = window.within("left").within("local-files").find(3usize);
        let position = point(row.bounds().center().x, row.bounds().origin.y + px(200.));
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
        window.render_frame(cx);
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
        window.render_frame(cx);
        for label in [
            "Preview",
            "Open folder",
            "Expand folder",
            "Collapse folder",
            "Mark",
            "Unmark",
            "Compare this file/folder",
            "Copy path",
        ] {
            assert!(
                !snapshots(window)
                    .iter()
                    .any(|element| element.label() == Some(label)),
                "background menu offered row action {label}"
            );
        }
        let siblings = menu_item(window, "Mark siblings");
        // Kit 0.7 does not advertise the disabled/checked flags natively;
        // exercise the disabled item instead of treating None as enabled.
        window
            .within("popup-menu")
            .click(siblings.path().last().unwrap().clone(), cx);
        assert_eq!(fixture.left.read(cx).selected(), Some("charlie.txt"));
        assert_eq!(fixture.left.read(cx).marked(), ["charlie.txt"]);
        // Change the cursor while the background popup is open. A background
        // callback must not restore the row selected when it was opened.
        fixture
            .left
            .update(cx, |pane, _| pane.files.select_path("alpha.txt"));
        click_item(window, "Invert visible marks", cx);
        assert_eq!(fixture.left.read(cx).selected(), Some("alpha.txt"));
        assert_eq!(
            fixture.left.read(cx).marked(),
            ["alpha.txt", "bravo.txt", "src"]
        );
        assert!(fixture.right.read(cx).marked().is_empty());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn menu_targets_filter_projection_reordered_rows_and_collapsed_marks(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        window
            .within("left")
            .within("local-files")
            .right_click(0usize, cx);
        click_item(window, "Expand folder", cx);
    })
    .unwrap();
    fixture.wait_idle(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        assert!(fixture.left.read(cx).tree.node("src").unwrap().expanded);
        assert!(!fixture.left.read(cx).can_back());
        window
            .within("left")
            .within("local-files")
            .right_click(1usize, cx);
        click_item(window, "Expand folder", cx);
    })
    .unwrap();
    fixture.wait_idle(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        assert_eq!(
            fixture.left.read(cx).files.row(2),
            Some("src/nested/deep.txt")
        );
        window
            .within("left")
            .within("local-files")
            .right_click(2usize, cx);
        click_item(window, "Mark", cx);
        window
            .within("left")
            .within("local-files")
            .right_click(0usize, cx);
        click_item(window, "Collapse folder", cx);
        assert_eq!(fixture.left.read(cx).len(), 4);
        assert_eq!(fixture.left.read(cx).marked(), ["src/nested/deep.txt"]);
        assert_eq!(fixture.left.read(cx).files.marked_descendants("src"), 1);
    })
    .unwrap();
    fixture.filter_left("txt", 3, cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        window
            .within("left")
            .within("local-files")
            .right_click(2usize, cx);
        click_item(window, "Copy path", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("charlie.txt")
        );
    })
    .unwrap();
    fs::write(fixture.left_root.path().join("a-first.txt"), "first").unwrap();
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Refresh, window, cx)
        });
    })
    .unwrap();
    fixture.wait_idle(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        assert_eq!(fixture.left.read(cx).files.row(0), Some("a-first.txt"));
        assert_eq!(fixture.left.read(cx).files.row(1), Some("alpha.txt"));
        window
            .within("left")
            .within("local-files")
            .right_click(1usize, cx);
        click_item(window, "Mark", cx);
        assert_eq!(
            fixture.left.read(cx).marked(),
            ["alpha.txt", "src/nested/deep.txt"]
        );
        window
            .within("left")
            .within("local-files")
            .right_click(1usize, cx);
        assert_eq!(menu_item(window, "Unmark").label(), Some("Unmark"));
        assert!(fixture.left.read(cx).files.is_marked("alpha.txt"));
        assert_ne!(menu_item(window, "Show hidden").checked(), Some(true));
        click_item(window, "Invert visible marks", cx);
        assert_eq!(
            fixture.left.read(cx).marked(),
            [
                "a-first.txt",
                "bravo.txt",
                "charlie.txt",
                "src/nested/deep.txt"
            ]
        );
        fixture.left.focus_handle(cx).focus(window, cx);
        window.press("v", cx);
        window.press("menu", cx);
        click_item(window, "Clear all marks", cx);
        assert!(fixture.left.read(cx).marked().is_empty());
        assert!(!fixture.left.read(cx).files.range_active());
        assert_eq!(
            fixture.left.read(cx).filter.read(cx).value().as_str(),
            "txt"
        );
        assert!(fixture.right.read(cx).marked().is_empty());
        assert_eq!(
            fixture.left.read(cx).location().unwrap().directory,
            fixture.left_root.path()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
async fn captured_menu_callback_is_rejected_after_filter_change(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let old = cx
        .update_window(fixture.handle, |_, window, cx| {
            window
                .within("left")
                .within("local-files")
                .right_click(1usize, cx);
            window.press("down", cx);
            window.press("down", cx); // Capture the real Mark callback.
            popup(&fixture.left, cx)
        })
        .unwrap();
    fixture.filter_left("bravo", 1, cx).await;
    assert!(fixture.left.read_with(cx, |pane, _| pane.menu.is_none()));
    fixture.events.borrow_mut().clear();
    fixture.replay_confirm(old, "Mark", cx);
    assert!(
        fixture
            .left
            .read_with(cx, |pane, _| pane.marked())
            .is_empty()
    );
    assert!(
        fixture
            .left
            .read_with(cx, |pane, _| pane.selected().is_none())
    );
    assert!(fixture.events.borrow().is_empty());
}

#[gpui_kit::test]
async fn captured_menu_callbacks_are_rejected_after_navigation_and_context_change(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let old = cx
        .update_window(fixture.handle, |_, window, cx| {
            window
                .within("left")
                .within("local-files")
                .right_click(1usize, cx);
            window.press("down", cx); // Preview would emit a selection event.
            let old = popup(&fixture.left, cx);
            fixture.left.update(cx, |pane, cx| {
                pane.open(fixture.left_root.path().join("src"), window, cx);
            });
            old
        })
        .unwrap();
    fixture.wait_idle(cx).await;
    fixture.events.borrow_mut().clear();
    fixture.replay_confirm(old, "Preview", cx);
    assert!(fixture.events.borrow().is_empty());
    assert!(
        fixture
            .left
            .read_with(cx, |pane, _| pane.selected().is_none())
    );
    assert_eq!(
        fixture
            .left
            .read_with(cx, |pane, _| pane.location().unwrap().directory.clone()),
        fixture.left_root.path().join("src")
    );

    let old = cx
        .update_window(fixture.handle, |_, window, cx| {
            window
                .within("left")
                .within("local-files")
                .right_click(1usize, cx);
            let item = menu_item(window, "Compare this file/folder");
            assert_ne!(item.disabled(), Some(true));
            window
                .within("popup-menu")
                .hover(item.path().last().unwrap().clone(), cx);
            let old = popup(&fixture.left, cx);
            fixture
                .left
                .update(cx, |pane, cx| pane.set_comparison_context(None, window, cx));
            assert!(fixture.left.read(cx).menu.is_none());
            let connection = OperationId {
                project: 99,
                operation: 7,
            };
            fixture.left.update(cx, |pane, cx| {
                pane.set_comparison_context(Some(connection), window, cx)
            });
            old
        })
        .unwrap();
    fixture.events.borrow_mut().clear();
    fixture.replay_confirm(old, "Compare this file/folder", cx);
    assert!(
        fixture.events.borrow().is_empty(),
        "old connection callback emitted {:?}",
        fixture.events.borrow()
    );
    assert_eq!(
        fixture
            .left
            .read_with(cx, |pane, _| pane.selected().map(str::to_owned)),
        Some("a.txt".into())
    );
}

#[gpui_kit::test]
async fn captured_menu_callback_cannot_mutate_or_dismiss_reopened_menu(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (old, current) = cx
        .update_window(fixture.handle, |_, window, cx| {
            window
                .within("left")
                .within("local-files")
                .right_click(1usize, cx);
            window.press("down", cx);
            window.press("down", cx);
            let old = popup(&fixture.left, cx);
            fixture.left.update(cx, |pane, cx| {
                pane.open_context_menu(
                    Some("bravo.txt".into()),
                    point(px(250.), px(200.)),
                    window,
                    cx,
                );
            });
            (old, popup(&fixture.left, cx))
        })
        .unwrap();
    fixture.events.borrow_mut().clear();
    fixture.replay_confirm(old, "Mark", cx);
    cx.update_window(fixture.handle, |_, window, cx| {
        assert_eq!(popup(&fixture.left, cx).entity_id(), current.entity_id());
        assert_eq!(fixture.left.read(cx).selected(), Some("bravo.txt"));
        assert!(fixture.left.read(cx).marked().is_empty());
        current.focus_handle(cx).focus(window, cx);
        window.render_frame(cx);
        assert_eq!(window.find("popup-menu").focused(), Some(true));
        window.press("escape", cx);
        assert!(fixture.right.read(cx).marked().is_empty());
    })
    .unwrap();
    assert!(fixture.left.read_with(cx, |pane, _| pane.menu.is_none()));
    assert!(fixture.events.borrow().is_empty());
}

#[gpui_kit::test]
async fn busy_row_menu_only_cancels_real_listing_without_marks_compare_or_preview(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    fixture.events.borrow_mut().clear();
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Find, window, cx);
            let cancel = pane.listing_cancel.as_ref().unwrap().clone();
            pane.open_context_menu(
                Some("alpha.txt".into()),
                point(px(250.), px(200.)),
                window,
                cx,
            );
            assert!(!cancel.is_cancelled());
        });
        let cancel = fixture
            .left
            .read(cx)
            .listing_cancel
            .as_ref()
            .unwrap()
            .clone();
        let id = fixture.left.read(cx).id();
        window.render_frame(cx);
        window.press("down", cx); // Kit selects disabled index zero on a fresh menu.
        window.press("enter", cx);
        assert!(!cancel.is_cancelled());
        assert!(fixture.left.read(cx).is_loading());
        assert!(fixture.left.read(cx).marked().is_empty());
        fixture.left.update(cx, |pane, cx| {
            pane.open_context_menu(
                Some("alpha.txt".into()),
                point(px(250.), px(200.)),
                window,
                cx,
            );
        });
        window.render_frame(cx);
        for label in [
            "Preview",
            "Mark",
            "Mark siblings",
            "Invert visible marks",
            "Compare this file/folder",
            "Compare marked files",
            "Compare project",
            "Refresh",
            "Find files",
        ] {
            let item = menu_item(window, label);
            if let Some(disabled) = item.disabled() {
                assert!(disabled, "busy row enabled {label}");
            }
        }
        for label in ["Preview", "Mark", "Compare this file/folder"] {
            let item = menu_item(window, label);
            window
                .within("popup-menu")
                .click(item.path().last().unwrap().clone(), cx);
            assert!(popup(&fixture.left, cx).focus_handle(cx).is_focused(window));
            assert!(fixture.left.read(cx).marked().is_empty());
            // Also dispatch Confirm to the real disabled item: the captured
            // callback must remain inert, not just its mouse click.
            window
                .within("popup-menu")
                .hover(item.path().last().unwrap().clone(), cx);
            window.press("enter", cx);
            fixture.left.update(cx, |pane, cx| {
                pane.open_context_menu(
                    Some("alpha.txt".into()),
                    point(px(250.), px(200.)),
                    window,
                    cx,
                );
            });
            window.render_frame(cx);
        }
        assert_ne!(menu_item(window, "Cancel loading").disabled(), Some(true));
        for key in ["space", "v", "s", "r", "j", "k"] {
            window.press(key, cx);
        }
        assert_eq!(fixture.left.read(cx).id(), id);
        assert_eq!(fixture.left.read(cx).selected(), Some("alpha.txt"));
        assert!(fixture.left.read(cx).marked().is_empty());
        assert!(!fixture.left.read(cx).files.range_active());
        // Up wraps to the last enabled item, Cancel loading, even when it
        // initially lies below the popup's scroll viewport.
        window.press("up", cx);
        window.press("enter", cx);
        assert!(cancel.is_cancelled());
        assert!(!fixture.left.read(cx).is_loading());
        assert!(fixture.left.read(cx).menu.is_none());
        assert_ne!(fixture.left.read(cx).id(), id);
        assert!(fixture.left.read(cx).browser_focus.is_focused(window));
        assert!(fixture.right.read(cx).marked().is_empty());
    })
    .unwrap();
    cx.executor().run_until_parked();
    assert!(
        fixture
            .events
            .borrow()
            .iter()
            .all(|event| matches!(event, ObservedEvent::Selected("left", _, false))),
        "busy row leaked actions: {:?}",
        fixture.events.borrow()
    );
    assert_eq!(fixture.left.read_with(cx, |pane, _| pane.len()), 4);
    assert!(!fixture.left.read_with(cx, |pane, _| pane.finder));
}

#[gpui_kit::test]
async fn edge_menu_stays_bounded_and_survives_a_virtualized_trigger(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    for index in 0..200 {
        fs::write(
            fixture.left_root.path().join(format!("zz-{index:03}.txt")),
            "file",
        )
        .unwrap();
    }
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.left.update(cx, |pane, cx| {
            pane.command(BrowserCommand::Refresh, window, cx)
        });
    })
    .unwrap();
    fixture.wait_idle(cx).await;
    cx.simulate_window_resize(fixture.handle, size(px(800.), px(360.)));
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.left.update(cx, |pane, cx| {
            pane.set_comparison_context(None, window, cx);
            pane.open_context_menu(
                Some("alpha.txt".into()),
                point(px(790.), px(350.)),
                window,
                cx,
            );
        });
        window.render_frame(cx);
        let menu_id = popup(&fixture.left, cx).entity_id();
        let bounds = window.find("popup-menu").bounds();
        let viewport = window.viewport_size();
        assert!(bounds.origin.x >= px(0.) && bounds.origin.y >= px(0.));
        assert!(bounds.right() <= viewport.width && bounds.bottom() <= viewport.height);
        assert!(bounds.size.height <= viewport.height * 0.5 + px(4.));
        fixture.left.update(cx, |pane, _| {
            assert_eq!(pane.files.len(), 204);
            pane.scroll
                .scroll_to_item(pane.files.len() - 1, ScrollStrategy::Top);
        });
        window.render_frame(cx);
        assert!(
            window
                .within("left")
                .within("local-files")
                .try_find(1usize)
                .is_none()
        );
        assert_eq!(popup(&fixture.left, cx).entity_id(), menu_id);
        assert_eq!(window.find("popup-menu").focused(), Some(true));
        assert_eq!(fixture.left.read(cx).selected(), Some("alpha.txt"));
        // Preview, Mark, siblings, invert, Copy; comparisons are disabled.
        for _ in 0..5 {
            window.press("down", cx);
        }
        assert!(menu_item(window, "Copy path").visible());
        window.press("enter", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("alpha.txt")
        );
        assert!(fixture.left.read(cx).marked().is_empty());
        assert!(fixture.left.read(cx).browser_focus.is_focused(window));
    })
    .unwrap();
    assert!(fixture.left.read_with(cx, |pane, _| pane.menu.is_none()));
}

#[gpui_kit::test]
async fn stale_rendered_row_cannot_open_a_menu_or_change_the_cursor(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    for projection_change in [true, false] {
        cx.update_window(fixture.handle, |_, window, cx| {
            window.render_frame(cx);
            let position = window
                .within("left")
                .within("local-files")
                .find(1usize)
                .bounds()
                .center();
            fixture
                .right
                .update(cx, |pane, cx| pane.focus_filter_input(window, cx));
            fixture.left.update(cx, |pane, cx| {
                pane.files.select_path("bravo.txt");
                if projection_change {
                    pane.rebuild_tree(cx);
                } else {
                    pane.stop_listing(cx);
                }
                cx.notify();
            });
            // Dispatch against the previous frame, before the new projection paints.
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
            assert!(fixture.left.read(cx).menu.is_none());
            assert_eq!(fixture.left.read(cx).selected(), Some("bravo.txt"));
            assert!(
                fixture
                    .right
                    .read(cx)
                    .filter
                    .focus_handle(cx)
                    .is_focused(window)
            );
            fixture.left.update(cx, |pane, cx| {
                pane.open_context_menu(Some("vanished.txt".into()), position, window, cx);
                assert_eq!(pane.selected(), Some("bravo.txt"));
                assert!(pane.menu.is_none());
            });
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
            window.render_frame(cx);
            window
                .within("left")
                .within("local-files")
                .right_click(1usize, cx);
            assert_eq!(fixture.left.read(cx).selected(), Some("alpha.txt"));
            assert!(fixture.left.read(cx).menu.is_some());
            window.press("escape", cx);
        })
        .unwrap();
    }
    assert!(fixture.left.read_with(cx, |pane, _| pane.menu.is_none()));
}
