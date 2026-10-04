use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, ElementId, Pixels, ScrollDelta, TestAppContext, WindowBounds,
    WindowOptions, point, size,
};
use std::time::Duration;

struct InsetHosts {
    manager: Entity<HostManager>,
    inset: Pixels,
    hidden: bool,
    outside: Entity<InputState>,
}
impl Render for InsetHosts {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .p(self.inset)
            .child(if self.hidden {
                Input::new(&self.outside)
                    .id("outside-host-form")
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(self.manager.clone())
                    .into_any_element()
            })
    }
}

fn fixture(
    store: &Store,
    width: f32,
    height: f32,
    inset: f32,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<InsetHosts>, Entity<HostManager>) {
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
                    let manager = cx.new(|cx| {
                        let service = BrowserService::new().unwrap();
                        HostManager::new(
                            store.clone(),
                            service.clone(),
                            RemoteService::new(service),
                            Some("project".into()),
                            w,
                            cx,
                        )
                    });
                    InsetHosts {
                        manager,
                        inset: px(inset),
                        hidden: false,
                        outside: cx.new(|cx| InputState::new(w, cx)),
                    }
                })
            },
        )
        .unwrap()
    });
    // The headless platform's initial size is not evidence of the actual viewport.
    cx.simulate_window_resize(handle, size(px(width), px(height)));
    frame(handle, cx);
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.viewport_size(), size(px(width), px(height)));
        let viewport = w.find("host-details").bounds();
        assert!(viewport.top() > px(inset));
        assert!(viewport.bottom() < px(height - inset));
        assert!(
            viewport.size.height < px(350.),
            "test needs an actually short viewport"
        );
    })
    .unwrap();
    let manager = root.read_with(cx, |root, _| root.manager.clone());
    (handle, root, manager)
}

async fn idle(handle: AnyWindowHandle, manager: &Entity<HostManager>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        manager.read(cx).cancel.is_none()
    })
    .await;
}

fn frame(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
}
fn press(handle: AnyWindowHandle, key: &str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, w, cx| w.press(key, cx))
        .unwrap();
    // Complete the frame after the deferred measured-geometry request.
    frame(handle, cx);
}
fn tab_to(handle: AnyWindowHandle, id: impl Into<ElementId>, cx: &mut TestAppContext) {
    let id = id.into();
    for _ in 0..160 {
        press(handle, "tab", cx);
        if cx
            .update_window(handle, |_, w, _| w.find(id.clone()).focused() == Some(true))
            .unwrap()
        {
            visible(handle, id, cx);
            return;
        }
    }
    panic!("Native Tab traversal did not reach {id:?}");
}
fn visible(handle: AnyWindowHandle, id: impl Into<ElementId>, cx: &mut TestAppContext) {
    let id = id.into();
    cx.update_window(handle, |_, w, _| {
        let viewport = w.find("host-details").bounds();
        let target = w.find(id.clone());
        let bounds = target.bounds();
        assert!(
            target.visible(),
            "{id:?} is clipped: {bounds:?}, viewport {viewport:?}"
        );
        let tolerance = px(1.);
        assert!(
            bounds.top() >= viewport.top() - tolerance
                && bounds.bottom() <= viewport.bottom() + tolerance,
            "{id:?} outside vertical viewport: {bounds:?}, viewport {viewport:?}"
        );
        assert!(
            bounds.left() >= viewport.left() - tolerance
                && bounds.right() <= viewport.right() + tolerance,
            "{id:?} outside horizontal viewport: {bounds:?}, viewport {viewport:?}"
        );
    })
    .unwrap();
}

fn host() -> Host {
    Host {
        name: "draft".into(),
        hostname: "example.test".into(),
        root_path: "/srv".into(),
        protocol: "sftp".into(),
        mappings: (0..12)
            .map(|i| Mapping {
                local: format!("src/{i}"),
                remote: format!("deploy/{i}"),
            })
            .collect(),
        ..Default::default()
    }
}

#[gpui_kit::test]
async fn host_form_tab_reveals_actual_bounds_in_short_narrow_inset_viewports(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for (width, height, inset) in [(860., 440., 0.), (800., 400., 24.)] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        let mut original = host();
        original.mappings.truncate(4);
        store
            .save_host(Some("project"), None, original.clone())
            .unwrap();
        let (handle, _, manager) = fixture(&store, width, height, inset, cx);
        idle(handle, &manager, cx).await;
        press(handle, "down", cx);
        press(handle, "e", cx);
        visible(handle, "host-name", cx);
        let (entities, operation) = manager.read_with(cx, |m, _| {
            (
                m.form
                    .as_ref()
                    .unwrap()
                    .fields
                    .iter()
                    .map(Entity::entity_id)
                    .collect::<Vec<_>>(),
                m.operation,
            )
        });
        let mut targets: Vec<ElementId> = [
            "host-name",
            "hostname",
            "port",
            "user",
            "keep-alive",
            "key-file",
            "passphrase",
            "root-path",
            "direct",
            "host-choose-link",
            "protocol-",
            "protocol-sftp",
            "protocol-ftp",
            "protocol-ftps",
            "auth-",
            "auth-keyfile",
            "auth-password",
            "auth-agent",
            "map-add",
            "host-save",
            "host-test-draft",
            "host-cancel",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        for index in 0..original.mappings.len() {
            targets.push(("map-local", index).into());
            targets.push(("map-remote", index).into());
        }
        manager.read_with(cx, |m, _| {
            targets.extend(
                m.form
                    .as_ref()
                    .unwrap()
                    .mappings
                    .iter()
                    .map(|(local, _)| ElementId::from(("map-remove", local.entity_id()))),
            );
        });
        for key in ["tab", "shift-tab"] {
            let mut visited = std::collections::HashSet::new();
            for _ in 0..160 {
                press(handle, key, cx);
                for target in &targets {
                    if cx
                        .update_window(handle, |_, w, _| {
                            w.find(target.clone()).focused() == Some(true)
                        })
                        .unwrap()
                    {
                        visible(handle, target.clone(), cx);
                        visited.insert(target.clone());
                    }
                }
                if visited.len() == targets.len() {
                    break;
                }
            }
            assert_eq!(
                visited.len(),
                targets.len(),
                "{key}: did not traverse all rendered native controls"
            );
        }
        manager.read_with(cx, |m, cx| {
            assert_eq!(m.operation, operation);
            let form = m.form.as_ref().unwrap();
            assert_eq!(
                form.fields
                    .iter()
                    .map(Entity::entity_id)
                    .collect::<Vec<_>>(),
                entities
            );
            assert_eq!(form.draft(cx).mappings, original.mappings);
            assert!(form.fields[PASSPHRASE].read(cx).presentation().is_masked());
        });
        assert!(store.project("project").unwrap().hosts == vec![original]);
    }
}

#[gpui_kit::test]
async fn host_form_added_mappings_reveal_once_and_mouse_scroll_does_not_snap_back(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store.save_host(Some("project"), None, host()).unwrap();
    let (handle, _, manager) = fixture(&store, 820., 420., 18., cx);
    idle(handle, &manager, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    for index in 12usize..16 {
        tab_to(handle, "map-add", cx);
        press(handle, if index % 2 == 0 { "enter" } else { "space" }, cx);
        visible(handle, ("map-local", index), cx);
        cx.update_window(handle, |_, w, cx| {
            assert_eq!(w.find(("map-local", index)).focused(), Some(true));
            w.input(&format!("added/{index}"), cx);
        })
        .unwrap();
        press(handle, "tab", cx);
        visible(handle, ("map-remote", index), cx);
        press(handle, "shift-tab", cx);
        visible(handle, ("map-local", index), cx);
    }
    let (focused, scroll) = manager.read_with(cx, |m, cx| {
        let form = m.form.as_ref().unwrap();
        (
            form.mappings.last().unwrap().0.focus_handle(cx),
            form.reveal.scroll_handle().clone(),
        )
    });
    let before = scroll.offset();
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "host-details",
            ScrollDelta::Pixels(point(px(0.), px(180.))),
            cx,
        )
    })
    .unwrap();
    frame(handle, cx);
    let after = scroll.offset();
    assert!(
        after.y > before.y,
        "wheel did not move the external viewport"
    );
    for _ in 0..4 {
        frame(handle, cx);
    }
    assert_eq!(
        scroll.offset(),
        after,
        "unchanged focus snapped back during repaint"
    );
    cx.update_window(handle, |_, w, _| assert!(focused.is_focused(w)))
        .unwrap();
    press(handle, "tab", cx);
    visible(handle, ("map-remote", 15usize), cx);
    assert_eq!(
        store.project("project").unwrap().hosts[0].mappings.len(),
        12
    );
}

#[gpui_kit::test]
async fn host_form_protocol_auth_and_link_changes_preserve_entities_and_reveal_new_focus(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let mut original = host();
    original.mappings.truncate(2);
    store
        .save_host(Some("project"), None, original.clone())
        .unwrap();
    store
        .save_host(
            None,
            None,
            Host {
                name: "shared".into(),
                hostname: "server.test".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let (handle, _, manager) = fixture(&store, 820., 420., 18., cx);
    idle(handle, &manager, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    let fields = manager.read_with(cx, |m, _| m.form.as_ref().unwrap().fields.clone());
    for protocol in ["protocol-ftp", "protocol-ftps"] {
        tab_to(handle, protocol, cx);
        press(handle, "space", cx);
        tab_to(handle, "password", cx);
        if protocol == "protocol-ftp" {
            cx.update_window(handle, |_, w, cx| w.input("secret", cx))
                .unwrap();
        }
    }
    tab_to(handle, "protocol-sftp", cx);
    press(handle, "enter", cx);
    for auth in ["auth-password", "auth-agent", "auth-keyfile"] {
        tab_to(handle, auth, cx);
        press(handle, "space", cx);
        tab_to(
            handle,
            if auth == "auth-password" {
                "password"
            } else if auth == "auth-agent" {
                "root-path"
            } else {
                "passphrase"
            },
            cx,
        );
    }
    tab_to(handle, ("link", 0usize), cx);
    press(handle, "enter", cx);
    frame(handle, cx);
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.find(("link", 0usize)).focused(), Some(true));
        assert!(w.try_find("hostname").is_none());
    })
    .unwrap();
    tab_to(handle, "root-path", cx);
    tab_to(handle, "direct", cx);
    press(handle, "space", cx);
    tab_to(handle, "hostname", cx);
    manager.read_with(cx, |m, cx| {
        let form = m.form.as_ref().unwrap();
        assert_eq!(form.fields, fields);
        assert_eq!(form.fields[PASSWORD].read(cx).value(), "secret");
        assert!(form.fields[PASSWORD].read(cx).presentation().is_masked());
    });
    assert!(store.project("project").unwrap().hosts == vec![original]);
}

#[gpui_kit::test]
async fn host_form_removed_controls_and_hidden_load_completion_do_not_take_focus(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    store.save_host(Some("project"), None, host()).unwrap();
    let (handle, root, manager) = fixture(&store, 820., 420., 18., cx);
    idle(handle, &manager, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    let remove = manager.read_with(cx, |m, _| {
        ElementId::from((
            "map-remove",
            m.form.as_ref().unwrap().mappings[5].0.entity_id(),
        ))
    });
    tab_to(handle, remove.clone(), cx);
    press(handle, "enter", cx);
    cx.update_window(handle, |_, w, cx| {
        assert!(w.try_find(remove).is_none());
        let form = manager.read(cx).form.as_ref().unwrap();
        assert_eq!(form.mappings.len(), 11);
        assert!(
            !form
                .fields
                .iter()
                .any(|field| field.focus_handle(cx).is_focused(w))
        );
        assert!(form.mappings[5].0.focus_handle(cx).is_focused(w));
    })
    .unwrap();
    root.update(cx, |root, cx| {
        root.hidden = true;
        cx.notify();
    });
    frame(handle, cx);
    cx.update_window(handle, |_, w, cx| {
        root.read(cx).outside.focus_handle(cx).focus(w, cx)
    })
    .unwrap();
    frame(handle, cx);
    cx.update_window(handle, |_, w, cx| {
        manager.update(cx, |m, cx| m.request(HostCommand::Load, w, cx));
    })
    .unwrap();
    idle(handle, &manager, cx).await;
    frame(handle, cx);
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.find("outside-host-form").focused(), Some(true));
        assert!(w.try_find("host-details").is_none());
    })
    .unwrap();
    assert!(store.project("project").unwrap().hosts == vec![host()]);
}

#[gpui_kit::test]
async fn host_form_mapping_removal_retains_entities_and_last_removal_focuses_root(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let mut original = host();
    original.mappings.truncate(3);
    store
        .save_host(Some("project"), None, original.clone())
        .unwrap();
    let (handle, _, manager) = fixture(&store, 860., 400., 12., cx);
    idle(handle, &manager, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    let retained = manager.read_with(cx, |m, _| m.form.as_ref().unwrap().mappings[2].1.clone());
    for count in (1..=3).rev() {
        let remove = manager.read_with(cx, |m, _| {
            ElementId::from((
                "map-remove",
                m.form.as_ref().unwrap().mappings[0].0.entity_id(),
            ))
        });
        tab_to(handle, remove, cx);
        press(handle, "space", cx);
        cx.update_window(handle, |_, w, cx| {
            let form = manager.read(cx).form.as_ref().unwrap();
            assert_eq!(form.mappings.len(), count - 1);
            if count == 3 {
                assert_eq!(form.mappings[1].1, retained);
            }
            if let Some((local, _)) = form.mappings.first() {
                assert!(local.focus_handle(cx).is_focused(w));
            } else {
                assert!(form.fields[ROOT].focus_handle(cx).is_focused(w));
            }
        })
        .unwrap();
        visible(
            handle,
            if count > 1 {
                ElementId::from(("map-local", 0usize))
            } else {
                "root-path".into()
            },
            cx,
        );
    }
    tab_to(handle, "host-cancel", cx);
    press(handle, "enter", cx);
    assert!(manager.read_with(cx, |m, _| m.form.is_none()));
    assert!(store.project("project").unwrap().hosts == vec![original]);
}

#[gpui_kit::test]
async fn host_form_resize_reveals_same_focus_and_rejects_older_frame_geometry(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let mut original = host();
    original.mappings.truncate(4);
    store
        .save_host(Some("project"), None, original.clone())
        .unwrap();
    let (handle, root, manager) = fixture(&store, 860., 400., 12., cx);
    idle(handle, &manager, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    tab_to(handle, ("map-remote", 3usize), cx);
    let field = manager.read_with(cx, |m, _| m.form.as_ref().unwrap().mappings[3].1.clone());
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        root.update(cx, |root, cx| {
            root.inset = px(20.);
            cx.notify();
        });
        w.render_frame(cx);
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(860.), px(270.)));
    frame(handle, cx);
    frame(handle, cx);
    visible(handle, ("map-remote", 3usize), cx);
    cx.update_window(handle, |_, w, cx| {
        assert!(field.focus_handle(cx).is_focused(w));
        assert_eq!(w.viewport_size(), size(px(860.), px(270.)));
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(860.), px(400.)));
    frame(handle, cx);
    frame(handle, cx);
    visible(handle, ("map-remote", 3usize), cx);
    manager.read_with(cx, |m, cx| {
        assert_eq!(m.form.as_ref().unwrap().mappings[3].1, field);
        assert_eq!(
            m.form.as_ref().unwrap().draft(cx).mappings,
            original.mappings
        );
        assert!(m.cancel.is_none());
    });
    assert!(store.project("project").unwrap().hosts == vec![original]);
}
