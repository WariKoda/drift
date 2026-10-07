use super::scroll_tests::{fixture, frame, idle, press, tab_to, visible};
use super::*;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Bounds, EntityInputHandler as _, Pixels, ScrollDelta, TestAppContext, point,
    size,
};
use std::fs;

fn chrome(handle: AnyWindowHandle, cx: &mut TestAppContext) -> [Bounds<Pixels>; 3] {
    cx.update_window(handle, |_, w, _| {
        [
            "project-dialog",
            "project-form-header",
            "project-form-footer",
        ]
        .map(|id| w.find(id).bounds())
    })
    .unwrap()
}

#[gpui_kit::test]
async fn project_dialog_measures_actual_parent_centers_and_keeps_only_body_scrollable(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for (width, height, inset, expected_width) in [
        (1000., 680., 56., 560.),
        (380., 320., 24., 316.),
        (320., 120., 8., 304.),
        // A large window whose ancestor leaves only a short usable body.
        (640., 480., 180., 280.),
    ] {
        let config = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        let original = store.register("Keep", root.path().into()).unwrap();
        let before = fs::read(config.path().join("projects.toml")).unwrap();
        let (handle, _, panel) = fixture(&store, width, height, inset, cx);
        idle(handle, &panel, cx).await;
        press(handle, "down", cx);
        press(handle, "e", cx);
        let stable = chrome(handle, cx);
        let [dialog, header, footer] = stable;
        assert_eq!(dialog.size.width, px(expected_width));
        assert!((dialog.center().x - px(width / 2.)).abs() < px(1.));
        assert!((dialog.center().y - px(height / 2.)).abs() < px(1.));
        assert!(dialog.top() >= px(inset) && dialog.bottom() <= px(height - inset));
        assert!(dialog.left() >= px(inset) && dialog.right() <= px(width - inset));
        assert_eq!(header.top(), dialog.top());
        assert_eq!(footer.bottom(), dialog.bottom());
        cx.update_window(handle, |_, w, _| {
            let body = w.find("project-details").bounds();
            assert!(body.top() >= header.bottom() && body.bottom() <= footer.top());
            assert!(
                body.size.height >= px(28.),
                "one native field must fit: {body:?}"
            );
            assert_eq!(w.find("project-edit-name").bounds().size.height, px(28.));
            assert_eq!(w.find("project-edit-path").bounds().size.height, px(28.));
            assert_eq!(w.find("project-save").label(), Some("Save project"));
            assert_eq!(w.find("project-close").label(), Some("Cancel"));
        })
        .unwrap();
        visible(handle, "project-edit-name", cx);
        for id in ["project-edit-path", "project-close", "project-save"] {
            press(handle, "tab", cx);
            cx.update_window(handle, |_, w, _| {
                assert_eq!(w.find(id).focused(), Some(true))
            })
            .unwrap();
            visible(handle, id, cx);
            assert_eq!(chrome(handle, cx), stable);
        }
        cx.update_window(handle, |_, w, cx| {
            w.scroll(
                "project-details",
                ScrollDelta::Pixels(point(px(0.), px(-1000.))),
                cx,
            );
        })
        .unwrap();
        frame(handle, cx);
        assert_eq!(chrome(handle, cx), stable);
        visible(handle, "project-save", cx);
        visible(handle, "project-close", cx);
        press(handle, "shift-tab", cx);
        press(handle, "enter", cx);
        assert!(panel.read_with(cx, |p, _| p.form.is_none() && p.cancel.is_none()));
        assert_eq!(
            fs::read(config.path().join("projects.toml")).unwrap(),
            before
        );
        assert!(store.registry().unwrap().find(&original.slug) == Some(&original));
    }
}

#[gpui_kit::test]
async fn project_dialog_resize_font_theme_preserve_native_preedit_selection_history_and_operation(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.register("Keep", root.path().into()).unwrap();
    let before = fs::read(config.path().join("projects.toml")).unwrap();
    let (handle, _, panel) = fixture(&store, 720., 520., 32., cx);
    idle(handle, &panel, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    tab_to(handle, "project-edit-path", cx);
    let (name, path, operation) = panel.read_with(cx, |p, _| {
        let f = p.form.as_ref().unwrap();
        (f.name.clone(), f.path.clone(), p.operation)
    });
    cx.update_window(handle, |_, w, cx| {
        path.update(cx, |s, cx| {
            s.set_value("A🦀Z", w, cx);
            s.replace_and_mark_text_in_range(Some(1..3), "に🦀", Some(1..3), w, cx);
        });
    })
    .unwrap();
    frame(handle, cx);
    for (font, mode, width, height) in [
        (24., ThemeMode::Light, 560., 320.),
        (20., ThemeMode::Dark, 460., 270.),
        (16., ThemeMode::Light, 400., 184.),
    ] {
        cx.update(|cx| {
            Theme::change(mode, None, cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
        });
        cx.simulate_window_resize(handle, size(px(width), px(height)));
        frame(handle, cx);
        visible(handle, "project-edit-path", cx);
        visible(handle, "project-save", cx);
        visible(handle, "project-close", cx);
        cx.update_window(handle, |_, w, cx| {
            let p = panel.read(cx);
            let f = p.form.as_ref().unwrap();
            assert_eq!((&f.name, &f.path), (&name, &path));
            assert_eq!(p.operation, operation);
            assert!(p.cancel.is_none() && !p.writing);
            assert!(path.focus_handle(cx).is_focused(w));
            assert_eq!(path.read(cx).value(), "Aに🦀Z");
            assert_eq!(path.read(cx).selected_range(), 4..8);
            assert_eq!(
                path.update(cx, |s, cx| s.marked_text_range(w, cx)),
                Some(1..4)
            );
        })
        .unwrap();
        let offset = panel.read_with(cx, |p, _| p.details_reveal.scroll_handle().offset());
        for _ in 0..3 {
            frame(handle, cx);
        }
        assert_eq!(
            panel.read_with(cx, |p, _| p.details_reveal.scroll_handle().offset()),
            offset
        );
    }
    // Ordinary typing/redraw doesn't re-run focus reveal even after manual scrolling.
    cx.update_window(handle, |_, w, cx| {
        path.update(cx, |s, cx| s.unmark_text(w, cx));
        w.scroll(
            "project-details",
            ScrollDelta::Pixels(point(px(0.), px(1000.))),
            cx,
        );
    })
    .unwrap();
    frame(handle, cx);
    let offset = panel.read_with(cx, |p, _| p.details_reveal.scroll_handle().offset());
    cx.update_window(handle, |_, w, cx| w.input("Q", cx))
        .unwrap();
    frame(handle, cx);
    assert_eq!(
        panel.read_with(cx, |p, _| p.details_reveal.scroll_handle().offset()),
        offset
    );
    press(
        handle,
        if cfg!(target_os = "macos") {
            "cmd-z"
        } else {
            "ctrl-z"
        },
        cx,
    );
    assert_eq!(path.read_with(cx, |s, _| s.value()), "Aに🦀Z");
    assert_eq!(
        fs::read(config.path().join("projects.toml")).unwrap(),
        before
    );
    tab_to(handle, "project-close", cx);
    press(handle, "space", cx);
    assert_eq!(
        fs::read(config.path().join("projects.toml")).unwrap(),
        before
    );
}

#[gpui_kit::test]
async fn project_dialog_native_save_persists_only_after_intentional_action(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for action in ["enter", "ctrl-s", "cmd-s", "click"] {
        let config = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        let (handle, _, panel) = fixture(&store, 440., 300., 20., cx);
        idle(handle, &panel, cx).await;
        press(handle, "down", cx);
        press(handle, "n", cx);
        cx.update_window(handle, |_, w, cx| w.input("日本🦀", cx))
            .unwrap();
        press(handle, "enter", cx);
        assert!(store.registry().unwrap().projects.is_empty());
        assert!(panel.read_with(cx, |p, _| p.form.is_some()));
        tab_to(handle, "project-edit-path", cx);
        cx.update_window(handle, |_, w, cx| {
            w.input(root.path().to_str().unwrap(), cx)
        })
        .unwrap();
        press(handle, "enter", cx);
        let operation = panel.read_with(cx, |p, _| p.operation);
        tab_to(handle, "project-save", cx);
        assert_eq!(panel.read_with(cx, |p, _| p.operation), operation);
        assert!(store.registry().unwrap().projects.is_empty());
        if action == "click" {
            cx.update_window(handle, |_, w, cx| w.click("project-save", cx))
                .unwrap();
        } else {
            press(handle, action, cx);
        }
        idle(handle, &panel, cx).await;
        assert!(panel.read_with(cx, |p, _| p.form.is_none()));
        let registry = store.registry().unwrap();
        assert_eq!(registry.projects.len(), 1);
        assert_eq!(registry.projects[0].name, "日本🦀");
        assert_eq!(registry.projects[0].path, root.path());
        assert!(config.path().join("projects.toml").exists());
    }
}

#[gpui_kit::test]
async fn project_dialog_narrow_scaled_footer_wraps_native_actions_without_clipping(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.register("Keep", root.path().into()).unwrap();
    let (handle, _, panel) = fixture(&store, 300., 560., 24., cx);
    idle(handle, &panel, cx).await;
    cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(24.)));
    press(handle, "down", cx);
    press(handle, "e", cx);
    visible(handle, "project-edit-name", cx);
    tab_to(handle, "project-edit-path", cx);
    let stable = chrome(handle, cx);
    for id in ["project-close", "project-save"] {
        press(handle, "tab", cx);
        visible(handle, id, cx);
        cx.update_window(handle, |_, w, _| {
            assert_eq!(w.find(id).focused(), Some(true))
        })
        .unwrap();
    }
    cx.update_window(handle, |_, w, _| {
        let cancel = w.find("project-close").bounds();
        let save = w.find("project-save").bounds();
        assert!(
            save.top() >= cancel.bottom(),
            "narrow footer must wrap, not overlap"
        );
        assert_eq!(cancel.size.height, px(42.));
        assert_eq!(save.size.height, px(42.));
        let dialog = w.find("project-dialog").bounds();
        assert_eq!(dialog.size.width, px(228.));
        assert!(dialog.top() >= px(24.) && dialog.bottom() <= px(536.));
    })
    .unwrap();
    assert_eq!(chrome(handle, cx), stable);
    press(handle, "shift-tab", cx);
    press(handle, "space", cx);
    assert_eq!(store.registry().unwrap().projects.len(), 1);
}

#[gpui_kit::test]
async fn project_dialog_busy_native_inputs_actions_and_shortcuts_do_not_write_or_dismiss(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let original = store.register("Keep", root.path().into()).unwrap();
    let before = fs::read(config.path().join("projects.toml")).unwrap();
    let (handle, _, panel) = fixture(&store, 440., 300., 20., cx);
    idle(handle, &panel, cx).await;
    press(handle, "down", cx);
    press(handle, "e", cx);
    let (name, path, operation) = panel.read_with(cx, |p, _| {
        let f = p.form.as_ref().unwrap();
        (f.name.clone(), f.path.clone(), p.operation)
    });
    panel.update(cx, |p, cx| p.set_busy(true, cx));
    frame(handle, cx);
    let values = cx.update(|cx| (name.read(cx).value(), path.read(cx).value()));
    cx.update_window(handle, |_, w, cx| {
        assert!(!name.read(cx).is_editable() && !path.read(cx).is_editable());
        w.input("must not enter the draft", cx);
        w.click("project-close", cx);
        w.click("project-save", cx);
        for key in ["ctrl-s", "cmd-s", "escape"] {
            w.press(key, cx);
        }
    })
    .unwrap();
    panel.read_with(cx, |p, cx| {
        let f = p.form.as_ref().unwrap();
        assert_eq!((&f.name, &f.path), (&name, &path));
        assert_eq!((name.read(cx).value(), path.read(cx).value()), values);
        assert_eq!(p.operation, operation);
        assert!(p.cancel.is_none() && p.visible);
    });
    assert_eq!(
        fs::read(config.path().join("projects.toml")).unwrap(),
        before
    );
    assert!(store.registry().unwrap().find(&original.slug) == Some(&original));
    panel.update(cx, |p, cx| p.set_busy(false, cx));
    frame(handle, cx);
    tab_to(handle, "project-close", cx);
    press(handle, "enter", cx);
    assert!(panel.read_with(cx, |p, _| p.form.is_none()));
}

#[gpui_kit::test]
async fn project_dialog_below_scaled_native_geometry_gate_keeps_footer_and_reveals_input_top(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    store.register("Keep", root.path().into()).unwrap();
    let (handle, _, panel) = fixture(&store, 320., 120., 8., cx);
    idle(handle, &panel, cx).await;
    cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(24.)));
    press(handle, "down", cx);
    press(handle, "e", cx);
    for id in ["project-edit-name", "project-edit-path"] {
        if id == "project-edit-path" {
            press(handle, "tab", cx);
        }
        cx.update_window(handle, |_, w, _| {
            let body = w.find("project-details").bounds();
            let field = w.find(id).bounds();
            let header = w.find("project-form-header").bounds();
            let footer = w.find("project-form-footer").bounds();
            // Even zero padding cannot fit title + native action + input in
            // 104 logical px at this font size. Do not claim full-input visibility.
            assert!(body.size.height > px(0.) && body.size.height < field.size.height);
            assert!((field.top() - body.top()).abs() <= px(1.));
            assert!(header.top() >= px(8.) && footer.bottom() <= px(112.));
            eprintln!(
                "project form geometry gate: usable=104, header={}, footer={}, body={}, input={}",
                header.size.height, footer.size.height, body.size.height, field.size.height
            );
        })
        .unwrap();
    }
    for id in ["project-close", "project-save"] {
        tab_to(handle, id, cx);
        visible(handle, id, cx);
    }
    press(handle, "shift-tab", cx);
    press(handle, "enter", cx);
    assert_eq!(store.registry().unwrap().projects.len(), 1);
}
