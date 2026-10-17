use super::*;
use crate::diff::{DiffPane, DiffState};
mod modal;

async fn selected_diff(cx: &mut TestAppContext) -> (Fixture, Entity<DiffPane>) {
    let fixture = Fixture::new(cx).await;
    let local = "before\n".repeat(60) + "local\n" + &"tail\n".repeat(60);
    let remote = "before\n".repeat(60) + "remote\n" + &"tail\n".repeat(60);
    fs::write(fixture.local.path().join("keep-00.txt"), local).unwrap();
    fs::write(fixture.server.dir.path().join("files/keep-00.txt"), remote).unwrap();
    cx.update_window(fixture.handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    fixture.idle(cx).await;
    let diff = fixture.shell.read_with(cx, |shell, cx| {
        shell.comparison.read(cx).diff_for_test().clone()
    });
    cx.update_window(fixture.handle, |_, w, cx| {
        w.press("tab", cx);
        assert!(diff.focus_handle(cx).is_focused(w));
        w.press("c", cx);
        w.press("ctrl-a", cx);
        w.press("right", cx);
        w.press("shift-left", cx);
        w.press("ctrl-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("l")
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(fixture.handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    (fixture, diff)
}
fn selected_text(w: &mut Window, cx: &mut gpui_kit::App) -> String {
    w.press("ctrl-c", cx);
    cx.read_from_clipboard().unwrap().text().unwrap_or_default()
}
fn state(diff: &Entity<DiffPane>, cx: &gpui_kit::App) -> DiffState {
    diff.read(cx).snapshot()
}

#[gpui_kit::test]
async fn text_selection_refresh_preserves_exact_path_pair_across_new_sorted_entry(
    cx: &mut TestAppContext,
) {
    let (fixture, diff) = selected_diff(cx).await;
    let before = diff.read_with(cx, |diff, _| diff.snapshot());
    let offset = before.scroll.0.borrow().base_handle.offset();
    fs::write(fixture.local.path().join("aa-added.txt"), "new pair").unwrap();
    cx.update_window(fixture.handle, |_, w, cx| w.press("r", cx))
        .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, w, cx| {
        w.render_frame(cx);
        let after = state(&diff, cx);
        assert_eq!(after.selection, before.selection);
        assert_eq!(after.expanded, before.expanded);
        assert_eq!(after.scroll.0.borrow().base_handle.offset(), offset);
        assert_eq!(selected_text(w, cx), "l");
        assert!(
            fixture
                .shell
                .read(cx)
                .comparison
                .read(cx)
                .sync_result_for_test()
                .is_none()
        );
        assert!(fixture.shell.read(cx).remote.read(cx).has_session());
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(fixture.local.path().join("aa-added.txt")).unwrap(),
        "new pair"
    );
    assert!(
        !fixture
            .server
            .dir
            .path()
            .join("files/aa-added.txt")
            .exists()
    );
}

#[gpui_kit::test]
async fn text_selection_refresh_discards_changed_and_removed_content(cx: &mut TestAppContext) {
    let (fixture, diff) = selected_diff(cx).await;
    fs::write(fixture.local.path().join("keep-00.txt"), "changed α🦀\n").unwrap();
    cx.update_window(fixture.handle, |_, w, cx| w.press("r", cx))
        .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, w, cx| {
        assert!(state(&diff, cx).selection.is_none());
        assert!(state(&diff, cx).expanded.is_empty());
        let formatted = selected_text(w, cx);
        assert!(formatted.contains("changed α🦀"));
        assert!(formatted.contains("@@"));
    })
    .unwrap();
    fs::remove_file(fixture.local.path().join("keep-00.txt")).unwrap();
    fs::remove_file(fixture.server.dir.path().join("files/keep-00.txt")).unwrap();
    cx.update_window(fixture.handle, |_, w, cx| w.press("r", cx))
        .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, w, cx| {
        assert!(state(&diff, cx).selection.is_none());
        let text = selected_text(w, cx);
        assert!(!text.contains("changed α🦀"));
        assert!(text.contains("local 1"));
        assert!(
            fixture
                .shell
                .read(cx)
                .comparison
                .read(cx)
                .sync_result_for_test()
                .is_none()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
async fn text_selection_roundtrip_help_and_resize_preserve_view_without_transfer(
    cx: &mut TestAppContext,
) {
    let (fixture, diff) = selected_diff(cx).await;
    let before = diff.read_with(cx, |diff, _| diff.snapshot());
    cx.update_window(fixture.handle, |_, w, cx| {
        w.press("n", cx);
        w.press("tab", cx);
        assert!(state(&diff, cx).selection.is_none());
        w.press("p", cx);
        w.press("tab", cx);
        assert_eq!(state(&diff, cx).selection, before.selection);
        assert_eq!(selected_text(w, cx), "l");
        w.press("f1", cx);
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("sentinel".into()));
        w.press("ctrl-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("sentinel")
        );
        w.press("escape", cx);
        assert_eq!(selected_text(w, cx), "l");
        drag(w, "comparison-split", 90., cx);
        assert_eq!(state(&diff, cx).selection, before.selection);
        assert_eq!(selected_text(w, cx), "l");
    })
    .unwrap();
    for width in [600., 1500.] {
        cx.simulate_window_resize(fixture.handle, size(px(width), px(440.)));
        cx.update_window(fixture.handle, |_, w, cx| {
            w.render_frame(cx);
            assert_eq!(state(&diff, cx).selection, before.selection);
            assert_eq!(state(&diff, cx).expanded, before.expanded);
            assert_eq!(selected_text(w, cx), "l");
            assert!(
                fixture
                    .shell
                    .read(cx)
                    .comparison
                    .read(cx)
                    .sync_result_for_test()
                    .is_none()
            );
        })
        .unwrap();
    }
    assert_eq!(
        fs::read_to_string(fixture.local.path().join("keep-00.txt")).unwrap(),
        "before\n".repeat(60) + "local\n" + &"tail\n".repeat(60)
    );
}
