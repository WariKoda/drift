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
    }
}

#[gpui_kit::test]
async fn panes_keep_filter_selection_history_and_cancellation_independent(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let left_root = tempfile::tempdir().unwrap();
    let right_root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir(left_root.path().join("child")).unwrap();
    fs::write(left_root.path().join("left.txt"), "left").unwrap();
    fs::write(right_root.path().join("right.txt"), "right").unwrap();
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
                        BrowserPane::new(store, service, right_root.path().into(), window, cx)
                    });
                    left.update(cx, |pane, cx| {
                        pane.open(left_root.path().into(), window, cx)
                    });
                    right.update(cx, |pane, cx| {
                        pane.open(right_root.path().into(), window, cx)
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
        window.input("right", cx);
    })
    .unwrap();
    // Input notifications may arrive after a frame on macOS.
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
            right_root.path()
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
}
