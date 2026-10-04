use super::*;

#[gpui_kit::test]
fn rejected_commit_or_preedit_preserves_mark_through_empty_preedit_and_unmark_after_repaint(
    cx: &mut TestAppContext,
) {
    let (handle, fields) = fixture(cx);
    let (input, boundary) = entities(&fields, 0, cx);
    for preedit in [false, true] {
        for unmark in [false, true] {
            cx.update_window(handle, |_, w, cx| japanese_preedit(&input, w, cx))
                .unwrap();
            cx.run_until_parked();
            let revision = boundary.read_with(cx, |s, _| s.revision);
            cx.update_window(handle, |_, w, cx| {
                let before = Snapshot::read(&input, w, cx);
                let mut proxy = handler(&input, &boundary, cx);
                if preedit {
                    proxy.replace_and_mark_text_in_range(None, "bad\npreedit", None, w, cx);
                } else {
                    proxy.replace_text_in_range(None, "bad\rcommit", w, cx);
                }
                drop(proxy);
                w.render_frame(cx);
                let mut proxy = handler(&input, &boundary, cx);
                if unmark {
                    proxy.unmark_text(w, cx);
                } else {
                    proxy.replace_and_mark_text_in_range(None, "", None, w, cx);
                }
                assert_eq!(Snapshot::read(&input, w, cx), before);
                assert_eq!(boundary.read(cx).warning.unwrap().message(), WARNING);
                assert_eq!(boundary.read(cx).revision, revision);
                // No permanent blanket rejection: a later explicit cancellation works.
                proxy.replace_and_mark_text_in_range(None, "", None, w, cx);
                assert_eq!(input.read(cx).value(), "AZ");
                assert_eq!(proxy.marked_text_range(w, cx), None);
            })
            .unwrap();
            cx.run_until_parked();
        }
    }
}
