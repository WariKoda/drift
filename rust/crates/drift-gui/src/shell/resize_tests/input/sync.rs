use super::*;

#[gpui_kit::test]
async fn focused_comparison_preedit_cannot_confirm_sync_but_native_confirm_after_blur_can(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let handle = fixture.handle;
    cx.update_window(handle, |_, w, cx| w.click("compare-project", cx))
        .unwrap();
    fixture.idle(cx).await;
    let comparison = fixture.shell.read_with(cx, |s, _| s.comparison.clone());
    let plan = cx
        .update_window(handle, |_, w, cx| {
            comparison.focus_handle(cx).focus(w, cx);
            w.press("shift-s", cx);
            let plan = w.find("sync-confirm").label().unwrap().to_owned();
            w.click("comparison-filter", cx);
            let input = w.focused_input(cx).unwrap().as_input().unwrap().clone();
            input.update(cx, |state, cx| {
                state.replace_and_mark_text_in_range(Some(0..0), "keep", Some(4..4), w, cx)
            });
            (plan, input)
        })
        .unwrap();
    cx.run_until_parked();
    let local_before = fs::read(fixture.local.path().join("keep-00.txt")).unwrap();
    let remote_before = fs::read(fixture.server.dir.path().join("files/keep-00.txt")).unwrap();
    for key in ["ctrl-enter", "cmd-enter"] {
        cx.update_window(handle, |_, w, cx| {
            plan.1.update(cx, |state, cx| {
                state.replace_and_mark_text_in_range(Some(0..4), "keep", Some(4..4), w, cx)
            });
            w.render_frame(cx);
            w.press(key, cx);
            assert!(plan.1.focus_handle(cx).is_focused(w));
            assert_eq!(plan.1.read(cx).value(), "keep");
            assert_eq!(w.find("sync-confirm").label(), Some(plan.0.as_str()));
            assert!(!comparison.read(cx).is_loading());
            assert!(comparison.read(cx).sync_result_for_test().is_none());
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(
            fs::read(fixture.local.path().join("keep-00.txt")).unwrap(),
            local_before
        );
        assert_eq!(
            fs::read(fixture.server.dir.path().join("files/keep-00.txt")).unwrap(),
            remote_before
        );
    }
    cx.update_window(handle, |_, w, cx| {
        plan.1.update(cx, |state, cx| {
            state.replace_and_mark_text_in_range(Some(0..4), "keep", Some(4..4), w, cx)
        });
        for _ in 0..32 {
            w.press("shift-tab", cx);
            if w.find("sync-confirm").focused() == Some(true) {
                break;
            }
        }
        assert_eq!(w.find("sync-confirm").focused(), Some(true));
        assert_eq!(
            plan.1
                .update(cx, |state, cx| state.marked_text_range(w, cx)),
            Some(0..4)
        );
        w.press("enter", cx);
    })
    .unwrap();
    fixture.idle(cx).await;
    assert!(comparison.read_with(cx, |c, _| c.sync_result_for_test().is_some()));
    assert_eq!(
        fs::read(fixture.local.path().join("keep-00.txt")).unwrap(),
        fs::read(fixture.server.dir.path().join("files/keep-00.txt")).unwrap()
    );
}
