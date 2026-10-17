use super::*;

#[gpui_kit::test]
async fn stored_project_controls_block_editor_without_rewriting_name_or_path(
    cx: &mut TestAppContext,
) {
    for unsafe_path in [false, true] {
        let config = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let path = if unsafe_path {
            root.path().join("root\nsegment")
        } else {
            root.path().join("root")
        };
        fs::create_dir(&path).unwrap();
        let store = Store::new(config.path().into());
        let project = store
            .register(if unsafe_path { "Keep" } else { "Keep\rProject" }, path)
            .unwrap();
        let before = fs::read(config.path().join("projects.toml")).unwrap();
        let (handle, panel) = fixture(&store, cx).await;
        press(handle, "down", cx);
        let operation = panel.read_with(cx, |p, _| p.operation);
        press(handle, "e", cx);
        panel.read_with(cx, |p, _| {
            assert!(p.form.is_none() && p.cancel.is_none() && !p.writing);
            assert_eq!(p.operation, operation);
            assert!(p.status.starts_with("Cannot open form:"));
            assert!(!p.status.contains("Keep") && !p.status.contains("segment"));
        });
        cx.update_window(handle, |_, w, _| {
            assert!(w.try_find("project-edit-name").is_none())
        })
        .unwrap();
        assert_eq!(
            fs::read(config.path().join("projects.toml")).unwrap(),
            before
        );
        assert!(store.registry().unwrap().find(&project.slug).unwrap() == &project);
    }
}
