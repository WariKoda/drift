use super::*;

#[gpui_kit::test]
fn restored_ratio_survives_clamps_without_writes_and_only_user_choices_publish(
    cx: &mut TestAppContext,
) {
    let (handle, harness) = open(cx, None);
    let changes = Rc::new(std::cell::RefCell::new(Vec::new()));
    cx.update(|cx| {
        harness.update(cx, |h, _| {
            let changes = changes.clone();
            h.split
                .configure_persistence(Some(0.8), move |value| changes.borrow_mut().push(value));
        })
    });
    for width in [200., 1000., 1., 1000.] {
        cx.simulate_window_resize(handle, size(px(width), px(500.)));
        cx.update_window(handle, |_, w, cx| {
            let minimum = 120_f32.min(width / 2.);
            assert_sizes(w, cx, px((width * 0.8).clamp(minimum, width - minimum)));
        })
        .unwrap();
    }
    assert!(changes.borrow().is_empty());
    cx.update_window(handle, |_, w, cx| {
        w.press("ctrl-alt-left", cx);
        assert_sizes(w, cx, px(776.));
        w.press("ctrl-alt-0", cx);
        assert_sizes(w, cx, px(500.));
        w.render_frame(cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(changes.borrow().as_slice(), &[Some(0.776), None]);
}

#[gpui_kit::test]
fn pointer_release_and_owned_escape_publish_actual_choice_not_viewport_clamp(
    cx: &mut TestAppContext,
) {
    let (handle, harness) = open(cx, Some(px(350.)));
    let changes = Rc::new(std::cell::RefCell::new(Vec::new()));
    cx.update(|cx| {
        harness.update(cx, |h, _| {
            let changes = changes.clone();
            h.split
                .configure_persistence(None, move |value| changes.borrow_mut().push(value));
        })
    });
    cx.update_window(handle, |_, w, cx| {
        assert_sizes(w, cx, px(350.));
        let from = w.find("pane-divider").bounds().center();
        w.drag(from, point(px(700.), from.y), cx);
        assert_sizes(w, cx, px(700.));
        let _ = start_drag(w, cx, px(600.));
        w.press("escape", cx);
        assert_sizes(w, cx, px(600.));
        assert!(!cx.has_active_drag());
    })
    .unwrap();
    assert_eq!(changes.borrow().last().copied(), Some(Some(0.6)));
    let count = changes.borrow().len();
    cx.simulate_window_resize(handle, size(px(100.), px(500.)));
    cx.update_window(handle, |_, w, cx| assert_sizes(w, cx, px(50.)))
        .unwrap();
    assert_eq!(changes.borrow().len(), count);
    cx.simulate_window_resize(handle, size(px(1000.), px(500.)));
    cx.update_window(handle, |_, w, cx| assert_sizes(w, cx, px(600.)))
        .unwrap();
    assert_eq!(changes.borrow().len(), count);
}

#[gpui_kit::test]
fn unmodified_default_and_blocked_minimum_keyboard_do_not_create_preferences(
    cx: &mut TestAppContext,
) {
    let (handle, harness) = open(cx, None);
    let changes = Rc::new(std::cell::RefCell::new(Vec::new()));
    cx.update(|cx| {
        harness.update(cx, |h, _| {
            let changes = changes.clone();
            h.split
                .configure_persistence(None, move |value| changes.borrow_mut().push(value));
        })
    });
    cx.simulate_window_resize(handle, size(px(200.), px(500.)));
    cx.update_window(handle, |_, w, cx| {
        assert_sizes(w, cx, px(100.));
        w.press("ctrl-alt-left", cx);
        w.press("ctrl-alt-right", cx);
        w.press("ctrl-alt-0", cx);
        assert_sizes(w, cx, px(100.));
    })
    .unwrap();
    assert!(changes.borrow().is_empty());
}
