use super::*;
use crate::sftp_test_support as support;
use drift_app::browser::BrowserService;
use gpui_kit::InputEvent as _;
use gpui_kit::base::test_support::snapshots;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, size};
use std::{fs, time::Duration};

struct View(Entity<RemotePane>);
impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().child(self.0.clone())
    }
}

async fn exercise(
    cx: &mut TestAppContext,
    disk: &std::path::Path,
    mut host: Host,
    options: drift_core::remote::ConnectOptions,
) {
    cx.executor().allow_parking();
    let name = "日本語🦀-long-directory-".repeat(5);
    fs::create_dir_all(disk.join("boundary").join(&name).join("leaf")).unwrap();
    fs::write(
        disk.join("boundary").join(&name).join("leaf/file.txt"),
        "real remote",
    )
    .unwrap();
    host.root_path = format!("{}/boundary", host.root_path.trim_end_matches('/'));
    let root = host.root_path.clone();
    let middle = format!("{root}/{name}");
    let deep = format!("{middle}/leaf");
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(480.), px(750.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let pane = cx.new(|cx| {
                        RemotePane::new(
                            RemoteService::with_options(BrowserService::new().unwrap(), options),
                            window,
                            cx,
                        )
                    });
                    pane.update(cx, |p, cx| p.set_context(7, vec![host], cx));
                    View(pane)
                })
            },
        )
        .unwrap()
    });
    let pane = view.read_with(cx, |v, _| v.0.clone());
    cx.update_window(handle, |_, w, cx| crate::test_support::connect_first(w, cx))
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    let session = pane.read_with(cx, |p, _| p.session_id().unwrap());
    cx.update_window(handle, |_, w, cx| {
        pane.update(cx, |p, cx| {
            p.navigate(deep.clone(), Navigation::Push, w, cx)
        })
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(
            pane.read(cx).breadcrumb_paths(),
            [root.clone(), middle.clone(), deep.clone()]
        );
        w.render_frame(cx);
        for (id, label) in [
            ("remote-back", "Back"),
            ("remote-forward", "Forward"),
            ("remote-up", "Up"),
            ("remote-refresh", "Refresh"),
            ("remote-hidden", "Show hidden"),
            ("remote-ignored", "Show ignored"),
        ] {
            let s = snapshots(w)
                .into_iter()
                .find(|s| s.path().last() == Some(&gpui_kit::ElementId::from(id)))
                .unwrap();
            assert_eq!(s.label(), Some(label));
            assert!(
                s.bounds().size.width <= metric(size::CONTROL, cx) + px(12.),
                "icon-only {id}: {:?}",
                s.bounds()
            );
        }
        let viewport = w.find("remote-breadcrumbs").bounds();
        assert!(viewport.size.width <= px(480.));
        assert!(
            w.find(("remote-crumb", 1usize)).bounds().size.width > viewport.size.width,
            "long segment must scroll, not truncate"
        );
        w.scroll(
            "remote-breadcrumbs",
            gpui_kit::ScrollDelta::Pixels(point(px(-3000.), px(0.))),
            cx,
        );
        w.render_frame(cx);
        let current = w.find(("remote-crumb-current", 2usize)).bounds();
        assert!(
            current.right() <= viewport.right() + px(1.),
            "current {current:?}, viewport {viewport:?}"
        );
        w.scroll(
            "remote-breadcrumbs",
            gpui_kit::ScrollDelta::Pixels(point(px(3000.), px(0.))),
            cx,
        );
        w.render_frame(cx);
        w.click(("remote-crumb", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).path, root);
        assert_eq!(pane.read(cx).session_id(), Some(session));
        w.render_frame(cx);
        assert!(
            !pane
                .read(cx)
                .control_enabled(pane.read(cx).navigation_stamp(), Control::Up)
        );
        let operation = pane.read(cx).operation;
        w.click("remote-up", cx);
        assert_eq!(pane.read(cx).operation, operation);
        mouse(w, cx, "remote-files-pane", NavigationDirection::Back);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).path, deep);
        mouse(w, cx, "remote-breadcrumbs", NavigationDirection::Forward);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(pane.read(cx).path, root);
        pane.update(cx, |p, cx| {
            p.files.select(0);
            p.files.toggle_mark();
            p.filter.update(cx, |s, cx| s.set_value("日本", w, cx));
        });
    })
    .unwrap();
    let busy_stamp = cx
        .update_window(handle, |_, w, cx| {
            pane.update(cx, |p, cx| {
                let id = p.operation;
                let filter = p.filter.entity_id();
                let selected = p.selected().map(str::to_owned);
                let marks = p.marked();
                let stamp = p.navigation_stamp();
                p.breadcrumb_navigation(stamp, root.clone(), w, cx);
                p.breadcrumb_navigation(stamp, "/".into(), w, cx);
                p.set_workspace_active(false, false, cx);
                p.mouse_navigation(p.navigation_stamp(), NavigationDirection::Back, w, cx);
                p.mouse_navigation(stamp, NavigationDirection::Back, w, cx);
                p.set_workspace_active(true, true, cx);
                p.mouse_navigation(stamp, NavigationDirection::Back, w, cx);
                let stamp = p.navigation_stamp();
                p.open_menu(None, point(px(10.), px(200.)), w, cx);
                p.mouse_navigation(stamp, NavigationDirection::Back, w, cx);
                p.dismiss_context_menu(w, cx);
                p.mouse_navigation(stamp, NavigationDirection::Back, w, cx);
                assert_eq!(p.operation, id);
                assert_eq!(p.session_id(), Some(session));
                assert_eq!(p.filter.entity_id(), filter);
                assert_eq!(p.filter.read(cx).value().as_str(), "日本");
                assert_eq!(p.selected(), selected.as_deref());
                assert_eq!(p.marked(), marks);
                let stamp = p.navigation_stamp();
                p.navigation_control(stamp, Control::Hidden, w, cx);
                let revision = p.navigation_revision;
                p.navigation_control(stamp, Control::Hidden, w, cx);
                assert_eq!(p.navigation_revision, revision);
                assert!(p.show_hidden);
                let stamp = p.navigation_stamp();
                p.refresh(&Refresh, w, cx);
                let busy = p.operation;
                p.mouse_navigation(p.navigation_stamp(), NavigationDirection::Back, w, cx);
                p.navigation_control(stamp, Control::Refresh, w, cx);
                assert_eq!(p.operation, busy);
                p.navigation_stamp()
            })
        })
        .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(w.find("remote-hidden").label(), Some("Hide hidden"));
        pane.update(cx, |p, cx| {
            let operation = p.operation;
            p.mouse_navigation(busy_stamp, NavigationDirection::Back, w, cx);
            assert_eq!(
                p.operation, operation,
                "a disabled loading frame must stay inert after completion"
            );
            let stamp = p.navigation_stamp();
            p.set_context(8, vec![], cx);
            p.mouse_navigation(stamp, NavigationDirection::Back, w, cx);
            p.breadcrumb_navigation(stamp, root.clone(), w, cx);
            assert!(!p.has_session());
        });
    })
    .unwrap();
}
fn mouse(window: &mut Window, cx: &mut App, id: &'static str, direction: NavigationDirection) {
    let position = window.find(id).bounds().center();
    window.dispatch_event(
        gpui_kit::MouseDownEvent {
            button: MouseButton::Navigate(direction),
            position,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

#[gpui_kit::test]
async fn real_sftp_breadcrumbs_icons_mouse_history_and_stale_callbacks(cx: &mut TestAppContext) {
    let server = support::Server::new(false);
    exercise(
        cx,
        &server.dir.path().join("files"),
        server.host(),
        server.options(),
    )
    .await;
}
#[gpui_kit::test]
async fn real_ftp_breadcrumbs_icons_mouse_history_and_stale_callbacks(cx: &mut TestAppContext) {
    let server = support::ftp::Server::new(4);
    exercise(
        cx,
        &server.dir.path().join("files"),
        server.host(),
        server.options(),
    )
    .await;
}

#[test]
fn posix_parent_keeps_exact_unicode_and_symlink_components() {
    assert_eq!(
        parent_path("/root/link/../日本/leaf"),
        Some("/root/link/../日本")
    );
    assert_eq!(parent_path("/"), None);
    assert_eq!(parent_path("/日本"), Some("/"));
}
