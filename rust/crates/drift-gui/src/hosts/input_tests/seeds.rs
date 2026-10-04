use super::*;

#[gpui_kit::test]
async fn stored_host_controls_block_editor_and_duplicate_before_native_normalization(
    cx: &mut TestAppContext,
) {
    for field in 0..5 {
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        let mut host = original();
        match field {
            0 => host.auth.password = "secret-first\nsecret-second".into(),
            1 => host.auth.passphrase = "phrase-first\rphrase-second".into(),
            2 => host.auth.key_file = "key-first\nkey-second".into(),
            3 => host.root_path = "/srv\rhidden".into(),
            _ => host.mappings[0].local = "src\nother".into(),
        }
        store
            .save_host(Some("project"), None, host.clone())
            .unwrap();
        let before = fs::read(config.path().join("projects/project.toml")).unwrap();
        let (handle, manager) = fixture(&store, cx).await;
        press(handle, "down", cx);
        let operation = manager.read_with(cx, |m, _| m.operation);
        for key in ["e", "c"] {
            press(handle, key, cx);
            manager.read_with(cx, |m, _| {
                assert!(m.form.is_none() && m.cancel.is_none() && m.tools.is_none());
                assert_eq!(m.operation, operation);
                assert!(m.status.starts_with("Cannot open form:"));
                assert!(!m.status.contains("secret") && !m.status.contains("phrase-first"));
            });
            cx.update_window(handle, |_, w, _| assert!(w.try_find("host-name").is_none()))
                .unwrap();
            assert_eq!(
                fs::read(config.path().join("projects/project.toml")).unwrap(),
                before
            );
            assert!(store.project("project").unwrap().hosts == vec![host.clone()]);
        }
    }
}
