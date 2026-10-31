use super::*;
use crate::test_support::enter_without_repaint;
use gpui_kit::AnyWindowHandle;

struct Fixture {
    server: support::Server,
    handle: AnyWindowHandle,
    pane: Entity<RemotePane>,
    hosts: Vec<Host>,
}
impl Fixture {
    fn new(cx: &mut TestAppContext) -> Self {
        cx.executor().allow_parking();
        let server = support::Server::new(false);
        let root = server.dir.path().join("files");
        fs::create_dir(root.join("second")).unwrap();
        fs::write(root.join("first.txt"), "first unchanged\n").unwrap();
        fs::write(root.join("second/second.txt"), "second unchanged\n").unwrap();
        let mut first = server.host();
        first.name = "Z first configured host".into();
        let mut second = first.clone();
        second.name = "A second configured host".into();
        second.root_path = root.join("second").to_string_lossy().into_owned();
        let hosts = vec![first, second];
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::actions::bind_keys(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let pane = cx.new(|cx| {
                        RemotePane::new(
                            RemoteService::with_options(
                                drift_app::browser::BrowserService::new().unwrap(),
                                server.options(),
                            ),
                            window,
                            cx,
                        )
                    });
                    pane.update(cx, |pane, cx| pane.set_context(1, hosts.clone(), cx));
                    RemoteWindow { pane }
                })
            })
            .unwrap()
        });
        let pane = view.read_with(cx, |view, _| view.pane.clone());
        Self {
            server,
            handle,
            pane,
            hosts,
        }
    }
    async fn idle(&self, cx: &mut TestAppContext) {
        cx.wait_for(self.handle, Duration::from_secs(30), |_, cx| {
            !self.pane.read(cx).is_loading()
        })
        .await;
    }
    fn unchanged(&self) {
        let root = self.server.dir.path().join("files");
        assert_eq!(
            fs::read_to_string(root.join("first.txt")).unwrap(),
            "first unchanged\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("second/second.txt")).unwrap(),
            "second unchanged\n"
        );
    }
}

#[gpui_kit::test]
async fn native_host_picker_uses_catalog_order_connects_second_and_same_host_keeps_session(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx);
    let initial = f.pane.read_with(cx, |pane, _| pane.connection());
    cx.update_window(f.handle, |_, w, cx| {
        for _ in 0..3 {
            w.render_frame(cx);
        }
        let picker = w.find("remote-host-select");
        assert_eq!(picker.role(), Some(gpui_kit::Role::ComboBox));
        assert_eq!(picker.label(), Some("Remote host"));
        assert_ne!(picker.disabled(), Some(true));
        assert_eq!(picker.value(), Some("Connect to host…"));
        assert_eq!(picker.expanded(), Some(false));
        w.click("remote-host-select", cx);
        w.press("down", cx);
        w.press("down", cx);
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(w.find("remote-host-select").expanded(), Some(false));
        assert_eq!(
            w.find("remote-host-select").value(),
            Some("Connect to host…")
        );
        assert_eq!(f.pane.read(cx).connection(), initial);
        assert!(!f.pane.read(cx).has_session());
        assert!(!f.pane.read(cx).is_loading());
        crate::test_support::select(w, "remote-host-select", None, 1, cx);
    })
    .unwrap();
    f.idle(cx).await;
    let session = f
        .pane
        .read_with(cx, |pane, _| pane.session.clone().unwrap());
    let connection = f.pane.read_with(cx, |pane, _| pane.connection());
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(f.pane.read(cx).host_name(), Some(f.hosts[1].name.as_str()));
        assert_eq!(f.pane.read(cx).directory(), f.hosts[1].root_path);
        assert_eq!(
            w.find("remote-host-select").value(),
            Some(f.hosts[1].name.as_str())
        );
        f.pane.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("space", cx);
        crate::test_support::select(w, "remote-host-select", Some(1), 1, cx);
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        let pane = f.pane.read(cx);
        assert_eq!(pane.session_id(), Some(session.id));
        assert_eq!(pane.connection(), connection);
        assert_eq!(
            pane.marked(),
            [format!("{}/second.txt", f.hosts[1].root_path)]
        );
        assert!(!pane.is_loading());
        assert_eq!(session.state(), ConnectionState::Connected);
        assert_eq!(w.find("remote-host-select").expanded(), Some(false));
        crate::test_support::select(w, "remote-host-select", Some(1), 0, cx);
    })
    .unwrap();
    f.idle(cx).await;
    cx.wait_for(f.handle, Duration::from_secs(30), |_, _| {
        session.state() == ConnectionState::Closed
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(f.pane.read(cx).host_name(), Some(f.hosts[0].name.as_str()));
        assert_ne!(f.pane.read(cx).session_id(), Some(session.id));
        assert!(f.pane.read(cx).connection() > connection);
        assert_eq!(
            w.find("remote-host-select").value(),
            Some(f.hosts[0].name.as_str())
        );
        w.click("disconnect", cx);
        assert!(!f.pane.read(cx).has_session());
    })
    .unwrap();
    f.unchanged();
}

#[gpui_kit::test]
async fn stale_native_host_popup_cannot_connect_after_project_catalog_or_visibility_change(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx);
    for route in 0..3 {
        cx.update_window(f.handle, |_, _, cx| {
            f.pane.update(cx, |pane, cx| {
                pane.set_workspace_active(true, true, cx);
                pane.set_context(1, f.hosts.clone(), cx);
            });
        })
        .unwrap();
        let expected = cx
            .update_window(f.handle, |_, w, cx| {
                w.render_frame(cx);
                w.click("remote-host-select", cx);
                w.press("down", cx);
                w.press("down", cx);
                f.pane.update(cx, |pane, cx| match route {
                    0 => pane.set_context(2, f.hosts.clone(), cx),
                    1 => {
                        let mut changed = f.hosts.clone();
                        changed.reverse();
                        changed[0].auth.password = "catalog changed credentials".into();
                        pane.set_context(1, changed, cx);
                    }
                    _ => pane.set_workspace_active(false, false, cx),
                });
                let connection = f.pane.read(cx).connection();
                enter_without_repaint(w, cx);
                connection
            })
            .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            for _ in 0..3 {
                w.render_frame(cx);
            }
            assert_eq!(f.pane.read(cx).connection(), expected, "route {route}");
            assert!(!f.pane.read(cx).has_session(), "route {route}");
            assert!(!f.pane.read(cx).is_loading(), "route {route}");
            assert_eq!(w.find("remote-host-select").expanded(), Some(false));
        })
        .unwrap();
    }
    f.unchanged();
}

#[gpui_kit::test]
async fn native_host_popup_rejects_confirm_when_real_listing_makes_the_picker_busy(
    cx: &mut TestAppContext,
) {
    let f = Fixture::new(cx);
    cx.update_window(f.handle, |_, w, cx| {
        crate::test_support::connect_first(w, cx)
    })
    .unwrap();
    f.idle(cx).await;
    let (session, connection) = f
        .pane
        .read_with(cx, |pane, _| (pane.session_id(), pane.connection()));
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote-host-select", cx);
        w.press("down", cx);
        f.pane.update(cx, |pane, cx| pane.refresh(&Refresh, w, cx));
        assert!(f.pane.read(cx).is_loading());
        enter_without_repaint(w, cx);
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(f.pane.read(cx).session_id(), session);
        assert_eq!(f.pane.read(cx).connection(), connection);
        assert_eq!(f.pane.read(cx).host_name(), Some(f.hosts[0].name.as_str()));
        assert_eq!(w.find("remote-host-select").expanded(), Some(false));
    })
    .unwrap();
    f.unchanged();
}
