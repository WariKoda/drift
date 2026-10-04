use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, ElementId, Pixels, ScrollDelta, TestAppContext, WindowBounds,
    WindowOptions, point, size,
};
use std::{fs, time::Duration};

struct InsetProjects {
    panel: Entity<ProjectsPanel>,
    inset: Pixels,
    hidden: bool,
    outside: Entity<InputState>,
}
impl Render for InsetProjects {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(self.inset)
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .child(if self.hidden {
                Input::new(&self.outside)
                    .id("outside-projects")
                    .into_any_element()
            } else {
                self.panel.clone().into_any_element()
            })
    }
}
fn fixture(
    store: &Store,
    width: f32,
    height: f32,
    inset: f32,
    cx: &mut TestAppContext,
) -> (
    AnyWindowHandle,
    Entity<InsetProjects>,
    Entity<ProjectsPanel>,
) {
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
                    let panel = cx.new(|cx| {
                        let mut panel = ProjectsPanel::new(
                            store.clone(),
                            BrowserService::new().unwrap(),
                            w,
                            cx,
                        );
                        panel.show(w, cx);
                        panel
                    });
                    InsetProjects {
                        panel,
                        inset: px(inset),
                        hidden: false,
                        outside: cx.new(|cx| InputState::new(w, cx)),
                    }
                })
            },
        )
        .unwrap()
    });
    cx.simulate_window_resize(handle, size(px(width), px(height)));
    frame(handle, cx);
    let panel = root.read_with(cx, |root, _| root.panel.clone());
    (handle, root, panel)
}
async fn idle(handle: AnyWindowHandle, panel: &Entity<ProjectsPanel>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        panel.read(cx).cancel.is_none()
    })
    .await;
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
}
fn visible(handle: AnyWindowHandle, id: impl Into<ElementId>, cx: &mut TestAppContext) {
    let id = id.into();
    cx.update_window(handle, |_, w, _| {
        let viewport = w.find("project-details").bounds();
        let target = w.find(id.clone());
        let bounds = target.bounds();
        assert!(
            target.visible(),
            "{id:?} clipped: {bounds:?}, viewport {viewport:?}"
        );
        assert!(
            bounds.top() >= viewport.top() - px(1.)
                && bounds.bottom() <= viewport.bottom() + px(1.),
            "{id:?} outside vertical viewport: {bounds:?}, viewport {viewport:?}"
        );
        assert!(
            bounds.left() >= viewport.left() - px(1.)
                && bounds.right() <= viewport.right() + px(1.),
            "{id:?} outside horizontal viewport: {bounds:?}, viewport {viewport:?}"
        );
    })
    .unwrap();
}
fn tab_to(handle: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) {
    for _ in 0..16 {
        press(handle, "tab", cx);
        if cx
            .update_window(handle, |_, w, _| w.find(id).focused() == Some(true))
            .unwrap()
        {
            visible(handle, id, cx);
            return;
        }
    }
    panic!("Tab did not reach {id}");
}

#[gpui_kit::test]
async fn project_edit_short_inset_tab_and_reverse_tab_reveal_controls_and_keep_draft(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for (width, height, inset) in [(380., 160., 12.), (320., 120., 8.)] {
        let config = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        let original = store.register("Keep", root.path().into()).unwrap();
        let (handle, _, panel) = fixture(&store, width, height, inset, cx);
        idle(handle, &panel, cx).await;
        press(handle, "down", cx);
        press(handle, "e", cx);
        visible(handle, "project-edit-name", cx);
        let (name, path, operation) = panel.read_with(cx, |p, _| {
            let f = p.form.as_ref().unwrap();
            (f.name.clone(), f.path.clone(), p.operation)
        });
        cx.update_window(handle, |_, w, cx| w.input(" draft", cx))
            .unwrap();
        frame(handle, cx);
        for id in ["project-edit-path", "project-save", "project-close"] {
            tab_to(handle, id, cx);
        }
        for id in ["project-save", "project-edit-path", "project-edit-name"] {
            press(handle, "shift-tab", cx);
            visible(handle, id, cx);
            assert_eq!(
                cx.update_window(handle, |_, w, _| w.find(id).focused())
                    .unwrap(),
                Some(true)
            );
        }
        panel.read_with(cx, |p, cx| {
            let f = p.form.as_ref().unwrap();
            assert_eq!(f.name, name);
            assert_eq!(f.path, path);
            assert!(f.name.read(cx).value().ends_with(" draft"));
            assert_eq!(p.operation, operation);
        });
        tab_to(handle, "project-close", cx);
        press(handle, "enter", cx);
        assert!(panel.read_with(cx, |p, _| p.form.is_none()));
        assert!(store.registry().unwrap().find(&original.slug).unwrap() == &original);
    }
}

#[gpui_kit::test]
async fn project_long_delete_details_scroll_and_native_cancel_never_removes_store(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let saved = store.register("Keep", root.path().into()).unwrap();
    let mut original = saved.clone();
    original.name = "Long project title ".repeat(35);
    store.save_project(Some(&saved), original.clone()).unwrap();
    store
        .save_host(
            Some(&original.slug),
            None,
            drift_core::config::Host {
                name: "prod".into(),
                hostname: "example.test".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let before = fs::read(
        config
            .path()
            .join("projects")
            .join(format!("{}.toml", original.slug)),
    )
    .unwrap();
    let (handle, _, panel) = fixture(&store, 340., 140., 12., cx);
    idle(handle, &panel, cx).await;
    for key in ["enter", "space"] {
        press(handle, "down", cx);
        let list_offset = panel.read_with(cx, |p, _| p.scroll.offset());
        press(handle, "d", cx);
        panel.read_with(cx, |p, _| {
            assert_eq!(
                p.details_reveal.scroll_handle().offset(),
                point(px(0.), px(0.))
            );
            assert_eq!(p.scroll.offset(), list_offset);
        });
        tab_to(handle, "project-delete-confirm", cx);
        tab_to(handle, "project-close", cx);
        assert!(
            panel.read_with(cx, |p, _| p.details_reveal.scroll_handle().offset().y
                < px(0.))
        );
        press(handle, key, cx);
        assert!(panel.read_with(cx, |p, _| p.delete.is_none()
            && p.visible
            && p.cancel.is_none()));
        assert!(store.registry().unwrap().find(&original.slug).unwrap() == &original);
        assert_eq!(
            fs::read(
                config
                    .path()
                    .join("projects")
                    .join(format!("{}.toml", original.slug))
            )
            .unwrap(),
            before
        );
        assert!(root.path().exists());
    }
    cx.simulate_window_resize(handle, size(px(420.), px(430.)));
    frame(handle, cx);
    let delete_id = format!("project-delete-{}", original.slug);
    let mut reached = false;
    for _ in 0..20 {
        press(handle, "tab", cx);
        reached = cx
            .update_window(handle, |_, w, _| {
                w.find(delete_id.clone()).focused() == Some(true)
            })
            .unwrap();
        if reached {
            break;
        }
    }
    assert!(
        reached,
        "native row deletion must remain keyboard-reachable"
    );
    cx.update_window(handle, |_, w, _| {
        let viewport = w.find("project-list").bounds();
        let target = w.find(delete_id).bounds();
        assert!(
            target.top() >= viewport.top() - px(1.)
                && target.bottom() <= viewport.bottom() + px(1.)
        );
    })
    .unwrap();
    let list_offset = panel.read_with(cx, |p, _| p.scroll.offset());
    press(handle, "enter", cx);
    panel.read_with(cx, |p, _| {
        assert!(p.delete.is_some());
        assert_eq!(
            p.details_reveal.scroll_handle().offset(),
            point(px(0.), px(0.))
        );
        assert_eq!(p.scroll.offset(), list_offset);
    });
    tab_to(handle, "project-close", cx);
    press(handle, "space", cx);
    assert!(store.registry().unwrap().find(&original.slug).unwrap() == &original);
}

#[gpui_kit::test]
async fn project_same_focus_resize_reveals_without_snapback_or_hidden_load_focus_theft(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let rootdir = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let original = store.register("Keep", rootdir.path().into()).unwrap();
    let (handle, root, panel) = fixture(&store, 500., 320., 12., cx);
    idle(handle, &panel, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    tab_to(handle, "project-save", cx);
    let fields = panel.read_with(cx, |p, _| {
        let f = p.form.as_ref().unwrap();
        (f.name.clone(), f.path.clone())
    });
    cx.simulate_window_resize(handle, size(px(340.), px(120.)));
    frame(handle, cx);
    visible(handle, "project-save", cx);
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "project-details",
            ScrollDelta::Pixels(point(px(0.), px(1000.))),
            cx,
        );
    })
    .unwrap();
    frame(handle, cx);
    let offset = panel.read_with(cx, |p, _| p.details_reveal.scroll_handle().offset());
    for _ in 0..3 {
        frame(handle, cx);
    }
    assert_eq!(
        panel.read_with(cx, |p, _| p.details_reveal.scroll_handle().offset()),
        offset
    );
    assert_eq!(
        cx.update_window(handle, |_, w, _| w.find("project-save").focused())
            .unwrap(),
        Some(true)
    );
    cx.update_window(handle, |_, w, cx| {
        panel.update(cx, |p, cx| p.request(ProjectCommand::Load, w, cx));
        root.update(cx, |root, cx| {
            root.hidden = true;
            root.outside.focus_handle(cx).focus(w, cx);
            cx.notify();
        });
    })
    .unwrap();
    frame(handle, cx);
    idle(handle, &panel, cx).await;
    cx.update_window(handle, |_, w, cx| {
        assert!(root.read(cx).outside.focus_handle(cx).is_focused(w));
        let f = panel.read(cx).form.as_ref().unwrap();
        assert_eq!(f.name, fields.0);
        assert_eq!(f.path, fields.1);
        root.update(cx, |root, cx| {
            root.hidden = false;
            cx.notify();
        });
        panel
            .read(cx)
            .form
            .as_ref()
            .unwrap()
            .path
            .focus_handle(cx)
            .focus(w, cx);
    })
    .unwrap();
    frame(handle, cx);
    visible(handle, "project-edit-path", cx);
    tab_to(handle, "project-close", cx);
    press(handle, "space", cx);
    assert!(store.registry().unwrap().find(&original.slug).unwrap() == &original);
}

#[gpui_kit::test]
async fn project_validation_error_keeps_native_inputs_and_can_scroll_back_to_cancel(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let (handle, _, panel) = fixture(&store, 340., 140., 10., cx);
    idle(handle, &panel, cx).await;
    // The empty list still supports the native New shortcut.
    press(handle, "down", cx);
    press(handle, "n", cx);
    cx.update_window(handle, |_, w, cx| w.input("Draft", cx))
        .unwrap();
    tab_to(handle, "project-edit-path", cx);
    let missing = config.path().join("a missing project folder ".repeat(12));
    cx.update_window(handle, |_, w, cx| w.input(missing.to_str().unwrap(), cx))
        .unwrap();
    let fields = panel.read_with(cx, |p, _| {
        let f = p.form.as_ref().unwrap();
        (f.name.clone(), f.path.clone())
    });
    fs::write(
        config.path().join("projects.toml"),
        "projects = [invalid TOML",
    )
    .unwrap();
    tab_to(handle, "project-save", cx);
    press(handle, "enter", cx);
    idle(handle, &panel, cx).await;
    assert!(panel.read_with(cx, |p, _| !p.status.is_empty() && p.form.is_some()));
    visible(handle, "project-save", cx);
    panel.read_with(cx, |p, _| {
        let f = p.form.as_ref().unwrap();
        assert_eq!(f.name, fields.0);
        assert_eq!(f.path, fields.1);
    });
    tab_to(handle, "project-edit-path", cx);
    tab_to(handle, "project-close", cx);
    press(handle, "enter", cx);
    fs::remove_file(config.path().join("projects.toml")).unwrap();
    assert!(store.registry().unwrap().projects.is_empty());
}
