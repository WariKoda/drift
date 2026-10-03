use super::*;
use crate::sftp_test_support as support;
use drift_app::{browser::Location, comparison::ScopeOptions, remote::RemoteService};
use drift_core::{config::RuntimeConfig, local::ProjectRoot};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    Bounds, EntityInputHandler as _, InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, TestAppContext, WindowBounds, WindowOptions, point, size,
};
use std::{fs, time::Duration};

#[gpui_kit::test]
async fn comparison_filter_keyboard_bridges_preserve_decisions_confirmation_and_work(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    for name in ["alpha.txt", "beta.txt"] {
        fs::write(local.path().join(name), "local contents").unwrap();
        fs::write(
            server.dir.path().join("files").join(name),
            "remote contents",
        )
        .unwrap();
    }
    let service = BrowserService::new().unwrap();
    let remote = RemoteService::with_options(service.clone(), server.options());
    let host = server.host();
    let connection = remote
        .connect(
            host.clone(),
            OperationId {
                project: 1,
                operation: 1,
            },
        )
        .task
        .await
        .unwrap()
        .unwrap()
        .session;
    let request = LoadRequest {
        location: Location {
            root: Arc::new(ProjectRoot::open(local.path()).unwrap()),
            directory: local.path().into(),
            slug: None,
            config: RuntimeConfig {
                hosts: vec![host.clone()],
                mappings: vec![],
                ui: Default::default(),
            },
        },
        host,
        connection: connection.clone(),
        local: vec![],
        remote: vec![],
        scope: ScopeOptions::default(),
    };
    let (handle, pane) = cx.update(|cx| {
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
                    let mut pane = ComparisonPane::new(service, window, cx);
                    pane.open(request, 1, window, cx);
                    pane
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).files.len(), 2);
        w.press("home", cx);
        w.press("ctrl-f", cx);
        assert!(pane.read(cx).filter.focus_handle(cx).is_focused(w));
        w.input("alpha", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        pane.read(cx).files.len() == 1
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        for key in ["enter", "down", "escape"] {
            let selected = pane.read(cx).selected;
            let decisions = pane.read(cx).decisions.clone();
            let id = pane.read(cx).id;
            let scroll = pane.read(cx).scroll.0.borrow().base_handle.offset();
            w.press(key, cx);
            assert!(pane.read(cx).focus.is_focused(w));
            assert_eq!(pane.read(cx).selected, selected);
            assert_eq!(pane.read(cx).decisions, decisions);
            assert_eq!(pane.read(cx).id, id);
            assert_eq!(pane.read(cx).scroll.0.borrow().base_handle.offset(), scroll);
            assert!(pane.read(cx).visible());
            assert!(!pane.read(cx).is_loading());
            assert_eq!(pane.read(cx).filter.read(cx).value().as_str(), "alpha");
            w.press("ctrl-f", cx);
        }
        // Native Escape must finish composition before the bubbling focus bridge.
        let filter = pane.read(cx).filter.clone();
        filter.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(Some(0..5), "alpha", Some(0..5), w, cx);
            assert!(input.marked_text_range(w, cx).is_some());
        });
        w.render_frame(cx);
        let from = w.find("pane-divider").bounds().center();
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
        w.dispatch_event(
            MouseMoveEvent {
                position: from + point(px(20.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        assert!(cx.has_active_drag());
        w.press("escape", cx);
        assert!(!cx.has_active_drag());
        assert!(filter.focus_handle(cx).is_focused(w));
        filter.update(cx, |input, cx| {
            assert!(input.marked_text_range(w, cx).is_some())
        });
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
        assert!(pane.read(cx).focus.is_focused(w));
        filter.update(cx, |input, cx| {
            assert_eq!(input.marked_text_range(w, cx), None)
        });
        assert_eq!(filter.read(cx).value().as_str(), "alpha");
        pane.update(cx, |pane, cx| pane.prepare_sync(true, cx));
        assert!(pane.read(cx).confirm_sync.is_some());
        w.press("ctrl-f", cx);
        w.press("enter", cx);
        assert!(pane.read(cx).confirm_sync.is_some());
        assert!(!pane.read(cx).is_loading());
        w.press("ctrl-f", cx);
        w.press("escape", cx);
        assert!(pane.read(cx).confirm_sync.is_some());
        pane.update(cx, |pane, _| pane.confirm_sync = None);
        w.press("ctrl-f", cx);
        w.press("ctrl-a", cx);
        w.input("missing", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        pane.read(cx).files.is_empty()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        for key in ["enter", "down", "escape"] {
            let decisions = pane.read(cx).decisions.clone();
            w.press(key, cx);
            assert!(pane.read(cx).focus.is_focused(w));
            assert!(pane.read(cx).selected.is_none());
            assert_eq!(pane.read(cx).decisions, decisions);
            assert_eq!(pane.read(cx).filter.read(cx).value().as_str(), "missing");
            assert!(pane.read(cx).visible());
            w.press("ctrl-f", cx);
        }
        pane.update(cx, |pane, cx| pane.refresh(&Refresh, w, cx));
        w.press("ctrl-f", cx);
        let cancel = pane.read(cx).loading.as_ref().unwrap().clone();
        w.press("escape", cx);
        assert!(!cancel.is_cancelled());
        assert!(pane.read(cx).is_loading());
        assert!(pane.read(cx).visible());
        assert_eq!(pane.read(cx).filter.read(cx).value().as_str(), "missing");
        w.press("ctrl-f", cx);
        w.press("ctrl-escape", cx);
        assert!(cancel.is_cancelled());
        assert!(!pane.read(cx).is_loading());
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    connection.shutdown().await;
}
