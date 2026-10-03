use super::*;
use drift_core::{config::RuntimeConfig, local::ProjectRoot};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{TestAppContext, WindowOptions};
use std::{fs, sync::Arc, time::Duration};

#[gpui_kit::test]
async fn only_successfully_loaded_preview_text_can_replace_the_clipboard(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("empty.txt"), "").unwrap();
    fs::write(dir.path().join("binary.dat"), [0_u8, 1, 2]).unwrap();
    let location = Location {
        root: Arc::new(ProjectRoot::open(dir.path()).unwrap()),
        directory: dir.path().into(),
        slug: None,
        config: RuntimeConfig::resolve(&Default::default(), None).unwrap(),
    };
    let (handle, preview) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
            cx.new(|cx| PreviewPane::new(BrowserService::new().unwrap(), w, cx))
        })
        .unwrap()
    });
    for (path, loaded) in [
        ("empty.txt", true),
        ("binary.dat", false),
        ("missing.txt", false),
    ] {
        cx.update_window(handle, |_, w, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("sentinel".into()));
            preview.update(cx, |pane, cx| {
                pane.show(location.clone(), path.into(), 1, w, cx);
                assert!(!pane.copy_text(cx));
            });
        })
        .unwrap();
        cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
            !preview.read(cx).is_loading()
        })
        .await;
        cx.update_window(handle, |_, w, cx| {
            preview.update(cx, |pane, cx| {
                assert_eq!(pane.copy_text(cx), loaded);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    if loaded { None } else { Some("sentinel") }
                );
                pane.select(None, 2, w, cx);
                assert!(!pane.copy_text(cx));
                assert!(!pane.requested_for(1, Path::new(path), false));
            });
        })
        .unwrap();
    }
}

#[gpui_kit::test]
async fn replacing_a_preview_cancels_old_work_and_keeps_only_the_current_file(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("old.txt"), "old content").unwrap();
    fs::write(dir.path().join("current.txt"), "current content").unwrap();
    let location = Location {
        root: Arc::new(ProjectRoot::open(dir.path()).unwrap()),
        directory: dir.path().into(),
        slug: None,
        config: RuntimeConfig::resolve(&Default::default(), None).unwrap(),
    };
    let (handle, preview) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| PreviewPane::new(BrowserService::new().unwrap(), window, cx))
        })
        .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        preview.update(cx, |pane, cx| {
            pane.show(location.clone(), "old.txt".into(), 1, window, cx);
            let old_id = pane.id();
            let old_cancel = pane.cancel.as_ref().unwrap().clone();
            pane.select(None, 2, window, cx);
            assert!(old_cancel.is_cancelled());
            assert!(pane.text.is_empty());
            assert_eq!(pane.selection, None);
            pane.show(location, "current.txt".into(), 2, window, cx);
            assert_ne!(old_id, pane.id());
        });
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(preview.read(cx).text, "current content");
        assert_eq!(preview.read(cx).title, "current.txt");
        window.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("current content")
        );
        window.click("copy-selection", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("current.txt")
        );
    })
    .unwrap();
}
