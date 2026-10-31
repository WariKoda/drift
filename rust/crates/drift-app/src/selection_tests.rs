use super::*;

fn list() -> FileList {
    FileList::new(
        ["folder", "a", "hidden/b", "c", "d"]
            .map(str::to_owned)
            .into(),
    )
}

#[test]
fn modifier_mapping_and_single_toggle_keep_marks_separate() {
    assert_eq!(
        SelectGesture::from_modifiers(false, false),
        SelectGesture::Single
    );
    assert_eq!(
        SelectGesture::from_modifiers(false, true),
        SelectGesture::Toggle
    );
    assert_eq!(
        SelectGesture::from_modifiers(true, false),
        SelectGesture::Extend { additive: false }
    );
    assert_eq!(
        SelectGesture::from_modifiers(true, true),
        SelectGesture::Extend { additive: true }
    );
    let mut list = list();
    list.select(3);
    list.toggle_mark();
    let marks = list.marked();
    list.click(0, SelectGesture::Single);
    assert_eq!(list.selection(), ["folder"]);
    list.click(1, SelectGesture::Toggle);
    assert_eq!(list.selection(), ["folder", "a"]);
    list.click(1, SelectGesture::Toggle);
    assert_eq!(list.selected(), Some("a"));
    assert!(!list.is_selected("a"));
    assert_eq!(list.selection(), ["folder"]);
    list.click(0, SelectGesture::Toggle);
    assert!(list.selection().is_empty());
    assert_eq!(list.selected(), Some("folder"));
    assert_eq!(list.marked(), marks);
    list.select(4);
    assert_eq!(list.selection(), ["d"]);
    list.select(usize::MAX);
    assert!(list.selection().is_empty());
    assert_eq!(list.selected(), None);
}

#[test]
fn shift_ranges_keep_the_anchor_and_additive_ranges_keep_other_rows() {
    let mut list = list();
    list.select(3);
    list.toggle_mark();
    let marks = list.marked();
    list.click(1, SelectGesture::Extend { additive: false });
    assert_eq!(list.selection(), ["a", "hidden/b", "c"]);
    assert_eq!(list.selected(), Some("a"));
    list.click(4, SelectGesture::Extend { additive: false });
    assert_eq!(list.selection(), ["c", "d"]);
    list.click(0, SelectGesture::Toggle);
    list.click(1, SelectGesture::Extend { additive: true });
    assert_eq!(list.selection(), ["folder", "a", "c", "d"]);
    list.click(0, SelectGesture::Extend { additive: false });
    assert_eq!(list.selection(), ["folder"]);
    assert_eq!(list.marked(), marks);
}

#[test]
fn ranges_only_cover_visible_rows_and_hidden_anchors_fall_back_to_single() {
    let mut list = list();
    list.click(4, SelectGesture::Extend { additive: true });
    assert_eq!(list.selection(), ["d"]);
    list.filter("hidden");
    assert!(list.selection().is_empty());
    list.click(0, SelectGesture::Extend { additive: true });
    assert_eq!(list.selection(), ["hidden/b"]);
    list.filter("");
    list.select(1);
    list.click(4, SelectGesture::Toggle);
    list.filter_finder("d");
    assert_eq!(list.selection(), ["d"]);
    list.filter("d");
    list.click(0, SelectGesture::Extend { additive: false });
    assert_eq!(list.selection(), ["folder", "hidden/b", "d"]);
    assert_eq!(list.selected(), Some("folder"));
    assert!(!list.is_selected("a"));
    assert!(!list.is_selected("c"));
}

#[test]
fn replacement_prunes_paths_preserves_cursor_and_visible_order_and_restores_saved_selection() {
    let mut list = list();
    list.select(1);
    list.click(3, SelectGesture::Toggle);
    list.replace_entries(["d", "c", "a"].map(str::to_owned).into());
    assert_eq!(list.selected(), Some("c"));
    assert_eq!(list.selection(), ["c", "a"]);
    let saved = list.clone();
    list.select(0);
    list.toggle_mark();
    list.restore_view(saved);
    assert_eq!(list.selection(), ["c", "a"]);
    assert_eq!(list.selected(), Some("c"));
    assert_eq!(list.marked(), ["d"]);
    list.click(0, SelectGesture::Extend { additive: false });
    assert_eq!(list.selection(), ["d", "c"]);
    list.replace_entries(vec!["a".into()]);
    assert!(list.selection().is_empty());
    assert_eq!(list.selected(), None);
    list.click(0, SelectGesture::Extend { additive: false });
    assert_eq!(list.selection(), ["a"]);
}

#[test]
fn remote_replacement_and_context_clicks_preserve_only_visible_selection() {
    let mut list = FileList::new_remote(
        "/root",
        ["/root/c", "/root/a", "/root/b"].map(str::to_owned).into(),
    );
    list.select(0);
    list.click(2, SelectGesture::Toggle);
    list.context_select_path("/root/c");
    assert_eq!(list.selection(), ["/root/c", "/root/b"]);
    assert_eq!(list.selected(), Some("/root/c"));
    list.replace_remote_entries(
        "/root",
        ["/root/b", "/root/c", "/outside"].map(str::to_owned).into(),
    );
    assert_eq!(list.selection(), ["/root/b", "/root/c"]);
    list.replace_remote_entries("/root", vec!["/root/b".into(), "/root/a".into()]);
    assert_eq!(list.selection(), ["/root/b"]);
    assert_eq!(list.selected(), None);
    list.context_select_path("/root/a");
    assert_eq!(list.selection(), ["/root/a"]);
}

#[test]
fn multi_selection_marks_all_markable_rows_then_unmarks_all_without_changing_selection() {
    let mut list = list();
    list.restrict_marks(["a".into(), "c".into()].into());
    list.select(1);
    list.toggle_mark();
    list.click(4, SelectGesture::Extend { additive: false });
    let selection = list.selection();
    assert!(!list.mark_will_clear());
    list.toggle_mark();
    assert_eq!(list.marked(), ["a", "c"]);
    assert!(list.mark_will_clear());
    assert_eq!(list.selection(), selection);
    list.toggle_mark();
    assert!(list.marked().is_empty());
    assert!(!list.mark_will_clear());
    assert_eq!(list.selection(), selection);
    list.select(0);
    list.click(2, SelectGesture::Toggle);
    list.toggle_mark();
    assert!(list.marked().is_empty());
}
