use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, ElementId, Pixels, ScrollDelta, TestAppContext, WindowBounds,
    WindowOptions, point, size,
};
use std::{fs, time::Duration};

struct InsetPicker {
    picker: Entity<LinkPicker>,
    inset: Pixels,
    chosen: usize,
    closed: usize,
    _subscription: Subscription,
}
impl Render for InsetPicker {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().size_full().p(self.inset).child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(self.picker.clone()),
        )
    }
}
fn fixture(
    store: &Store,
    width: f32,
    height: f32,
    inset: f32,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<InsetPicker>, Entity<LinkPicker>) {
    let (handle, root) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(width), px(height)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    let previous = cx.focus_handle();
                    previous.focus(w, cx);
                    let picker = cx.new(|cx| {
                        LinkPicker::new(
                            store.clone(),
                            BrowserService::new().unwrap(),
                            "dest".into(),
                            w,
                            cx,
                        )
                    });
                    assert_eq!(picker.read(cx).previous_focus.as_ref(), Some(&previous));
                    let subscription =
                        cx.subscribe(&picker, |this: &mut InsetPicker, _, event, cx| {
                            match event {
                                LinkEvent::Chosen { .. } => this.chosen += 1,
                                LinkEvent::Close => this.closed += 1,
                            }
                            cx.notify();
                        });
                    InsetPicker {
                        picker,
                        inset: px(inset),
                        chosen: 0,
                        closed: 0,
                        _subscription: subscription,
                    }
                })
            },
        )
        .unwrap()
    });
    cx.simulate_window_resize(handle, size(px(width), px(height)));
    frame(handle, cx);
    let picker = root.read_with(cx, |root, _| root.picker.clone());
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.viewport_size(), size(px(width), px(height)));
        for id in ["host-link-details", "host-link-list"] {
            let viewport = w.find(id).bounds();
            assert!(viewport.top() >= px(inset));
            assert!(viewport.bottom() <= px(height - inset));
            assert!(viewport.size.height > px(32.));
            assert!(viewport.size.height < px(180.));
        }
    })
    .unwrap();
    (handle, root, picker)
}
async fn idle(handle: AnyWindowHandle, picker: &Entity<LinkPicker>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        picker.read(cx).cancel.is_none()
    })
    .await;
    frame(handle, cx);
    frame(handle, cx);
}
fn frame(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
}
fn press(handle: AnyWindowHandle, key: &str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, cx| w.press(key, cx))
        .unwrap();
    frame(handle, cx);
    frame(handle, cx);
}
fn visible(
    handle: AnyWindowHandle,
    id: impl Into<ElementId>,
    viewport: &'static str,
    cx: &mut TestAppContext,
) {
    let id = id.into();
    cx.update_window(handle, |_, w, _| {
        let viewport = w.find(viewport).bounds();
        let target = w.find(id.clone());
        let bounds = target.bounds();
        assert!(
            target.visible(),
            "{id:?} clipped: {bounds:?}, viewport {viewport:?}"
        );
        let tolerance = px(1.);
        assert!(
            bounds.top() >= viewport.top() - tolerance
                && bounds.bottom() <= viewport.bottom() + tolerance
                && bounds.left() >= viewport.left() - tolerance
                && bounds.right() <= viewport.right() + tolerance,
            "{id:?} outside viewport: {bounds:?}, viewport {viewport:?}"
        );
    })
    .unwrap();
}
fn tab_to(
    handle: AnyWindowHandle,
    id: impl Into<ElementId>,
    viewport: &'static str,
    cx: &mut TestAppContext,
) {
    let id = id.into();
    for _ in 0..100 {
        press(handle, "tab", cx);
        if cx
            .update_window(handle, |_, w, _| w.find(id.clone()).focused() == Some(true))
            .unwrap()
        {
            visible(handle, id, viewport, cx);
            return;
        }
    }
    panic!("Tab never reached {id:?}");
}
fn populate(store: &Store) -> Vec<Host> {
    let hosts: Vec<_> = (0..10)
        .map(|i| Host {
            name: format!("promotion {i:02} with a long deployment description for review"),
            hostname: format!("endpoint-{i}.very-long-subdomain.deployment.example.test"),
            user: "deployment-operator-with-a-long-name".into(),
            root_path: format!("/srv/deployment/{i}"),
            protocol: "sftp".into(),
            port: 22,
            ..Default::default()
        })
        .collect();
    for host in &hosts {
        store.save_host(Some("source"), None, host.clone()).unwrap();
    }
    hosts
}
fn filter(handle: AnyWindowHandle, value: &str, cx: &mut TestAppContext) {
    press(handle, "ctrl-f", cx);
    cx.update_window(handle, |_, w, cx| w.input(value, cx))
        .unwrap();
    frame(handle, cx);
    frame(handle, cx);
    visible(handle, "host-link-filter", "host-link-details", cx);
}
fn review_last(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    press(handle, "down", cx);
    press(handle, "end", cx);
    press(handle, "enter", cx);
}

#[gpui_kit::test]
async fn link_picker_native_tabs_reveal_controls_in_both_bounded_viewports(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for (width, height, inset) in [(320., 300., 16.), (480., 280., 24.)] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        let hosts = populate(&store);
        let (handle, root, picker) = fixture(&store, width, height, inset, cx);
        idle(handle, &picker, cx).await;
        let (query, previous, operation) = picker.read_with(cx, |p, _| {
            (p.query.clone(), p.previous_focus.clone(), p.operation)
        });
        filter(handle, "promotion", cx);
        let targets: Vec<ElementId> = ["host-link-filter", "host-link-reload", "host-link-close"]
            .into_iter()
            .map(Into::into)
            .chain((0usize..hosts.len()).map(|i| ("host-link-target", i).into()))
            .collect();
        for key in ["tab", "shift-tab"] {
            let mut visited = std::collections::HashSet::new();
            for _ in 0..160 {
                press(handle, key, cx);
                for (i, id) in targets.iter().enumerate() {
                    if cx
                        .update_window(handle, |_, w, _| w.find(id.clone()).focused() == Some(true))
                        .unwrap()
                    {
                        visible(
                            handle,
                            id.clone(),
                            if i < 3 {
                                "host-link-details"
                            } else {
                                "host-link-list"
                            },
                            cx,
                        );
                        visited.insert(id.clone());
                    }
                }
                if visited.len() == targets.len() {
                    break;
                }
            }
            assert_eq!(
                visited.len(),
                targets.len(),
                "{key} missed a native control"
            );
        }
        picker.read_with(cx, |p, cx| {
            assert_eq!(p.query, query);
            assert_eq!(p.query.read(cx).value(), "promotion");
            assert_eq!(p.previous_focus, previous);
            assert_eq!(p.operation, operation);
            assert!(p.selected.is_none());
            assert!(p.cursor.is_none());
        });
        root.read_with(cx, |r, _| assert_eq!((r.chosen, r.closed), (0, 0)));
        assert!(store.project("source").unwrap().hosts == hosts);
        assert!(store.global().unwrap().hosts.is_empty());
    }
}

#[gpui_kit::test]
async fn link_picker_wrapped_confirmation_back_reload_never_promote_on_native_activation(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let hosts = populate(&store);
    let source = fs::read(dir.path().join("projects/source.toml")).unwrap();
    let (handle, root, picker) = fixture(&store, 320., 300., 16., cx);
    idle(handle, &picker, cx).await;
    filter(handle, "promotion", cx);
    for key in ["enter", "space"] {
        review_last(handle, cx);
        let (cursor, selected, offset, query, operation) = picker.read_with(cx, |p, _| {
            (
                p.cursor.clone(),
                p.selected.clone().unwrap(),
                p.scroll.offset(),
                p.query.clone(),
                p.operation,
            )
        });
        assert!(selected.host == hosts[9]);
        for direction in ["tab", "shift-tab"] {
            let mut visited = std::collections::HashSet::new();
            for _ in 0..16 {
                press(handle, direction, cx);
                for id in [
                    "host-promote-confirm",
                    "host-link-reload",
                    "host-link-close",
                ] {
                    if cx
                        .update_window(handle, |_, w, _| w.find(id).focused() == Some(true))
                        .unwrap()
                    {
                        visible(handle, id, "host-link-details", cx);
                        visited.insert(id);
                    }
                }
                if visited.len() == 3 {
                    break;
                }
            }
            assert_eq!(visited.len(), 3);
        }
        cx.update_window(handle, |_, w, _| {
            assert!(w.try_find("host-link-filter").is_none());
            assert!(w.try_find(("host-link-target", 9usize)).is_none());
        })
        .unwrap();
        tab_to(handle, "host-link-close", "host-link-details", cx);
        press(handle, key, cx);
        picker.read_with(cx, |p, cx| {
            assert!(p.selected.is_none());
            assert!(p.cancel.is_none());
            assert_eq!(p.operation, operation);
            assert_eq!(p.cursor, cursor);
            assert_eq!(p.scroll.offset(), offset);
            assert_eq!(p.query, query);
            assert_eq!(p.query.read(cx).value(), "promotion");
        });
        cx.update_window(handle, |_, w, cx| {
            assert!(picker.read(cx).list_focus.is_focused(w))
        })
        .unwrap();
        press(handle, "enter", cx);
        tab_to(handle, "host-link-reload", "host-link-details", cx);
        press(handle, key, cx);
        assert!(!picker.read_with(cx, |p, _| p.writing));
        idle(handle, &picker, cx).await;
        picker.read_with(cx, |p, cx| {
            assert_eq!(p.operation, operation + 1);
            assert!(p.selected.is_none());
            assert_eq!(p.cursor, cursor);
            assert_eq!(p.query, query);
            assert_eq!(p.query.read(cx).value(), "promotion");
        });
        assert_eq!(
            fs::read(dir.path().join("projects/source.toml")).unwrap(),
            source
        );
        assert!(store.global().unwrap().hosts.is_empty());
        assert!(!dir.path().join("projects/dest.toml").exists());
        root.read_with(cx, |r, _| assert_eq!((r.chosen, r.closed), (0, 0)));
    }
    // Only activation of the explicit confirmation commits the selected snapshot.
    press(handle, "enter", cx);
    tab_to(handle, "host-promote-confirm", "host-link-details", cx);
    press(handle, "space", cx);
    idle(handle, &picker, cx).await;
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    assert_eq!(
        store.project("source").unwrap().hosts[9].server,
        hosts[9].name
    );
    assert!(!dir.path().join("projects/dest.toml").exists());
    root.read_with(cx, |r, _| assert_eq!((r.chosen, r.closed), (1, 0)));
}

#[gpui_kit::test]
async fn link_picker_confirmation_resize_reveals_focus_wheel_does_not_snap_or_replace_snapshot(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let hosts = populate(&store);
    let (handle, root, picker) = fixture(&store, 360., 310., 18., cx);
    idle(handle, &picker, cx).await;
    filter(handle, "promotion", cx);
    review_last(handle, cx);
    tab_to(handle, "host-link-close", "host-link-details", cx);
    let (scroll, cursor, list_offset, query, previous, operation) = picker.read_with(cx, |p, _| {
        (
            p.reveal.scroll_handle().clone(),
            p.cursor.clone(),
            p.scroll.offset(),
            p.query.clone(),
            p.previous_focus.clone(),
            p.operation,
        )
    });
    let before = scroll.offset();
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "host-link-details",
            ScrollDelta::Pixels(point(px(0.), px(160.))),
            cx,
        );
    })
    .unwrap();
    frame(handle, cx);
    let after = scroll.offset();
    assert!(
        after.y > before.y,
        "wheel must move the confirmation viewport"
    );
    for _ in 0..4 {
        frame(handle, cx);
    }
    assert_eq!(
        scroll.offset(),
        after,
        "repaint snapped back to unchanged focus"
    );
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.find("host-link-close").focused(), Some(true))
    })
    .unwrap();
    let mut changed = hosts[9].clone();
    changed.hostname = "changed-after-review.example.test".into();
    store
        .save_host(Some("source"), Some(&hosts[9]), changed.clone())
        .unwrap();
    for height in [240., 340.] {
        root.update(cx, |r, cx| {
            r.inset = px(24.);
            cx.notify();
        });
        cx.simulate_window_resize(handle, size(px(320.), px(height)));
        frame(handle, cx);
        frame(handle, cx);
        visible(handle, "host-link-close", "host-link-details", cx);
        picker.read_with(cx, |p, cx| {
            assert!(p.selected.as_ref().unwrap().host == hosts[9]);
            assert_eq!(p.cursor, cursor);
            assert_eq!(p.scroll.offset(), list_offset);
            assert_eq!(p.query, query);
            assert_eq!(p.query.read(cx).value(), "promotion");
            assert_eq!(p.previous_focus, previous);
            assert_eq!(p.operation, operation);
            assert!(p.cancel.is_none());
        });
    }
    press(handle, "enter", cx);
    assert!(picker.read_with(cx, |p, _| p.selected.is_none()));
    assert!(store.project("source").unwrap().hosts[9] == changed);
    assert!(store.global().unwrap().hosts.is_empty());
    root.read_with(cx, |r, _| assert_eq!((r.chosen, r.closed), (0, 0)));
}

#[gpui_kit::test]
async fn link_picker_native_row_focus_resize_and_wheel_keep_cursor_and_list_handle(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let hosts = populate(&store);
    let (handle, _, picker) = fixture(&store, 360., 310., 18., cx);
    idle(handle, &picker, cx).await;
    filter(handle, "promotion", cx);
    press(handle, "down", cx);
    press(handle, "end", cx);
    let (scroll, cursor, operation) =
        picker.read_with(cx, |p, _| (p.scroll.clone(), p.cursor.clone(), p.operation));
    tab_to(handle, ("host-link-target", 9usize), "host-link-list", cx);
    let before = scroll.offset();
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "host-link-list",
            ScrollDelta::Pixels(point(px(0.), px(100.))),
            cx,
        );
    })
    .unwrap();
    frame(handle, cx);
    let after = scroll.offset();
    assert!(after.y > before.y);
    for _ in 0..4 {
        frame(handle, cx);
    }
    assert_eq!(scroll.offset(), after);
    cx.simulate_window_resize(handle, size(px(320.), px(260.)));
    frame(handle, cx);
    frame(handle, cx);
    visible(handle, ("host-link-target", 9usize), "host-link-list", cx);
    // Native activation reviews this row; it must not propagate into promotion.
    for key in ["enter", "space"] {
        press(handle, key, cx);
        picker.read_with(cx, |p, cx| {
            assert!(p.selected.as_ref().unwrap().host == hosts[9]);
            assert_eq!(p.cursor, cursor);
            assert_eq!(p.operation, operation);
            assert!(p.cancel.is_none());
            assert_eq!(p.query.read(cx).value(), "promotion");
        });
        assert!(store.global().unwrap().hosts.is_empty());
        assert!(store.project("source").unwrap().hosts == hosts);
        tab_to(handle, "host-link-close", "host-link-details", cx);
        press(handle, "space", cx);
        tab_to(handle, ("host-link-target", 9usize), "host-link-list", cx);
    }
}
