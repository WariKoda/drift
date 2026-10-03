use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, point, size};
use std::{fs, time::Duration};
async fn idle(handle: AnyWindowHandle, panel: &Entity<ProjectsPanel>, cx: &mut TestAppContext) {
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        panel.read(cx).cancel.is_none()
    })
    .await;
}
fn open(store: &Store, cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<ProjectsPanel>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1200.), px(900.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut panel = ProjectsPanel::new(
                        store.clone(),
                        BrowserService::new().unwrap(),
                        window,
                        cx,
                    );
                    panel.show(window, cx);
                    panel
                })
            },
        )
        .unwrap()
    })
}
#[gpui_kit::test]
async fn create_edit_archive_and_confirmed_remove_keep_local_files(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("keep"), "data").unwrap();
    let store = Store::new(config.path().into());
    let (handle, panel) = open(&store, cx);
    idle(handle, &panel, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.click("project-new", cx);
        w.click("project-edit-name", cx);
        w.input("Shop", cx);
        w.click("project-edit-path", cx);
        w.input(root.path().to_str().unwrap(), cx);
        w.press("ctrl-s", cx);
    })
    .unwrap();
    idle(handle, &panel, cx).await;
    let project = store.registry().unwrap().find("shop").unwrap().clone();
    store
        .save_host(
            Some("shop"),
            None,
            drift_core::config::Host {
                name: "prod".into(),
                hostname: "prod.example".into(),
                ..Default::default()
            },
        )
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("project-edit-shop", cx);
        panel.update(cx, |p, cx| {
            p.form
                .as_ref()
                .unwrap()
                .name
                .update(cx, |input, cx| input.set_value("Renamed", w, cx))
        });
        w.press("ctrl-s", cx);
    })
    .unwrap();
    idle(handle, &panel, cx).await;
    assert_eq!(
        store.registry().unwrap().find("shop").unwrap().name,
        "Renamed"
    );
    assert!(store.registry().unwrap().find("shop").unwrap().created_at == project.created_at);
    cx.update_window(handle, |_, w, cx| w.click("project-archive-shop", cx))
        .unwrap();
    idle(handle, &panel, cx).await;
    assert!(store.registry().unwrap().find("shop").unwrap().archived);
    cx.update_window(handle, |_, w, cx| {
        w.click("project-archived", cx);
        w.click("project-archive-shop", cx);
    })
    .unwrap();
    idle(handle, &panel, cx).await;
    assert!(!store.registry().unwrap().find("shop").unwrap().archived);
    cx.update_window(handle, |_, w, cx| {
        w.click("project-delete-shop", cx);
        w.press("escape", cx);
    })
    .unwrap();
    assert!(store.registry().unwrap().find("shop").is_some());
    cx.update_window(handle, |_, w, cx| {
        w.click("project-delete-shop", cx);
        w.press("enter", cx);
    })
    .unwrap();
    idle(handle, &panel, cx).await;
    assert!(store.registry().unwrap().find("shop").is_none());
    assert!(!config.path().join("projects/shop.toml").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("keep")).unwrap(),
        "data"
    );
}
#[gpui_kit::test]
async fn stale_edit_and_delete_preserve_inputs_and_confirmation(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let project = store.register("Shop", root.path().into()).unwrap();
    let (handle, panel) = open(&store, cx);
    idle(handle, &panel, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.click("project-edit-shop", cx);
        panel.update(cx, |p, cx| {
            p.form
                .as_ref()
                .unwrap()
                .name
                .update(cx, |input, cx| input.set_value("Unsaved", w, cx))
        });
    })
    .unwrap();
    store
        .edit_project(&project, "Concurrent", root.path().to_str().unwrap())
        .unwrap();
    cx.update_window(handle, |_, w, cx| w.press("ctrl-s", cx))
        .unwrap();
    idle(handle, &panel, cx).await;
    cx.update_window(handle, |_, w, cx| {
        assert!(panel.read(cx).status.contains("changed in another"));
        assert_eq!(
            panel
                .read(cx)
                .form
                .as_ref()
                .unwrap()
                .name
                .read(cx)
                .value()
                .as_ref(),
            "Unsaved"
        );
        w.press("escape", cx);
        w.click("project-reload", cx);
    })
    .unwrap();
    idle(handle, &panel, cx).await;
    cx.update_window(handle, |_, w, cx| w.click("project-delete-shop", cx))
        .unwrap();
    store.mark_opened("shop").unwrap();
    cx.update_window(handle, |_, w, cx| w.press("enter", cx))
        .unwrap();
    idle(handle, &panel, cx).await;
    assert!(panel.read_with(cx, |p, _| p.delete.is_some()
        && p.status.contains("changed in another")));
    assert_eq!(
        store.registry().unwrap().find("shop").unwrap().name,
        "Concurrent"
    );
}

#[gpui_kit::test]
async fn keyboard_projects_keep_cursor_filter_forms_and_confirmation_separate(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let config = tempfile::tempdir().unwrap();
    let roots = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    for i in 0..12 {
        let path = roots.path().join(format!("project-{i:02}"));
        fs::create_dir(&path).unwrap();
        store.register(&format!("Project {i:02}"), path).unwrap();
    }
    let (handle, panel) = open(&store, cx);
    idle(handle, &panel, cx).await;
    cx.update_window(handle, |_, w, cx| {
        w.press("down", cx);
        assert!(panel.read(cx).list_focus.is_focused(w));
        for (key, expected) in [
            ("end", "project-11"),
            ("home", "project-00"),
            ("j", "project-01"),
            ("k", "project-00"),
            ("shift-g", "project-11"),
            ("g", "project-00"),
        ] {
            w.press(key, cx);
            assert_eq!(panel.read(cx).selected_project(cx).unwrap().slug, expected);
        }
        w.press("end", cx);
        assert!(panel.read(cx).scroll.offset().y < px(0.));
        w.press("e", cx);
        assert_eq!(
            panel
                .read(cx)
                .form
                .as_ref()
                .unwrap()
                .expected
                .as_ref()
                .unwrap()
                .slug,
            "project-11"
        );
        w.press("ctrl-f", cx); // Search must not steal focus from the active form.
        assert!(
            panel
                .read(cx)
                .form
                .as_ref()
                .unwrap()
                .name
                .focus_handle(cx)
                .is_focused(w)
        );
        w.input("nde", cx);
        w.press("escape", cx);
        assert!(panel.read(cx).form.is_none());
        assert!(panel.read(cx).list_focus.is_focused(w));
        assert_eq!(
            panel.read(cx).selected_project(cx).unwrap().slug,
            "project-11"
        );
        w.press("n", cx);
        w.press("escape", cx);
        assert_eq!(
            panel.read(cx).selected_project(cx).unwrap().slug,
            "project-11"
        );
        w.press("/", cx);
        w.input("Project 05", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("enter", cx); // Filter -> list, without opening a project.
        assert!(panel.read(cx).list_focus.is_focused(w));
        assert_eq!(
            panel.read(cx).selected_project(cx).unwrap().slug,
            "project-05"
        );
        w.press("a", cx);
    })
    .unwrap();
    idle(handle, &panel, cx).await;
    assert!(
        store
            .registry()
            .unwrap()
            .find("project-05")
            .unwrap()
            .archived
    );
    cx.update_window(handle, |_, w, cx| {
        assert!(panel.read(cx).visible_projects(cx).is_empty());
        w.press("home", cx);
        w.press("end", cx);
        w.press("delete", cx);
        assert!(panel.read(cx).delete.is_none());
        w.press(".", cx);
        assert_eq!(
            panel.read(cx).selected_project(cx).unwrap().slug,
            "project-05"
        );
        w.press("a", cx);
    })
    .unwrap();
    idle(handle, &panel, cx).await;
    assert!(
        !store
            .registry()
            .unwrap()
            .find("project-05")
            .unwrap()
            .archived
    );
    cx.update_window(handle, |_, w, cx| {
        w.press("d", cx);
        assert!(panel.read(cx).delete.is_some());
        w.press("escape", cx);
        assert!(panel.read(cx).list_focus.is_focused(w));
        w.press("delete", cx);
        w.press("enter", cx);
    })
    .unwrap();
    idle(handle, &panel, cx).await;
    assert!(store.registry().unwrap().find("project-05").is_none());
    assert!(roots.path().join("project-05").exists());

    let new_path = roots.path().join("new");
    fs::create_dir(&new_path).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("n", cx); // Creating works even with no matching rows.
        w.input("New", cx);
        w.click("project-edit-path", cx);
        w.input(new_path.to_str().unwrap(), cx);
        w.press("cmd-s", cx);
    })
    .unwrap();
    idle(handle, &panel, cx).await;
    assert!(store.registry().unwrap().find("new").is_some());
    cx.update_window(handle, |_, w, cx| {
        assert!(panel.read(cx).list_focus.is_focused(w));
        w.press("ctrl-f", cx);
        w.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        w.input("ndeajkgGy", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        assert!(panel.read(cx).form.is_none());
        assert!(panel.read(cx).delete.is_none());
        assert_eq!(panel.read(cx).query.read(cx).value().as_str(), "ndeajkgGy");
    })
    .unwrap();
}
