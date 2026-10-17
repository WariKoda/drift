use super::*;
use drift_core::diff::{Decision, LineKind, hunks};
use std::collections::BTreeSet;

fn equal_result(text: &str) -> DiffResult {
    let mut result = DiffResult::default();
    result.text(text.as_bytes(), text.as_bytes());
    result
}

fn selection(anchor: (usize, usize), head: (usize, usize)) -> Selection {
    Selection {
        anchor: TextPoint {
            source: anchor.0,
            byte: anchor.1,
        },
        head: TextPoint {
            source: head.0,
            byte: head.1,
        },
    }
}

fn assert_both_directions(
    selected: Selection,
    result: &DiffResult,
    rows: &[Row],
    copied: &str,
    ranges: &[Option<Range<usize>>],
) {
    assert_eq!(ranges.len(), result.lines.len());
    for selected in [
        selected,
        Selection {
            anchor: selected.head,
            head: selected.anchor,
        },
    ] {
        assert_eq!(selected.copy_text(result, rows), copied);
        for (source, range) in ranges.iter().enumerate() {
            assert_eq!(
                selected.range_for(source, result, rows),
                *range,
                "source {source}",
            );
        }
    }
}

#[test]
fn clipping_floors_extended_graphemes_not_just_utf8_characters() {
    let text = "e\u{301}👩‍💻日本🇯🇵";
    let graphemes = ["e\u{301}", "👩‍💻", "日", "本", "🇯🇵"];
    let mut start = 0;
    for grapheme in graphemes {
        for byte in start..start + grapheme.len() {
            assert_eq!(clip_grapheme(text, byte), start, "byte {byte}");
        }
        start += grapheme.len();
        assert_eq!(clip_grapheme(text, start), start);
    }
    assert_eq!(start, text.len());
    assert_eq!(clip_grapheme(text, usize::MAX), text.len());
    assert_eq!(clip_grapheme("", 0), 0);
    assert_eq!(clip_grapheme("", usize::MAX), 0);
}

#[test]
fn unicode_ranges_and_copy_use_the_same_grapheme_boundaries() {
    let result = equal_result("Ae\u{301}👩‍💻日本\n");
    let rows = hunks::flatten(&result, &BTreeSet::new(), Decision::Skip);
    let start = "A".len();
    let end = "Ae\u{301}👩‍💻日".len();
    assert_both_directions(
        selection((0, start + 1), (0, end + 1)),
        &result,
        &rows,
        "e\u{301}👩‍💻日",
        &[Some(start..end)],
    );
}

#[test]
fn reversed_multiline_selection_clips_only_endpoint_lines() {
    let result = equal_result("alpha\nbeta\ngamma\n");
    let rows = hunks::flatten(&result, &BTreeSet::new(), Decision::Download);
    assert_both_directions(
        selection((2, 3), (0, 2)),
        &result,
        &rows,
        "pha\nbeta\ngam",
        &[Some(2..5), Some(0..4), Some(0..3)],
    );
}

#[test]
fn upload_selection_follows_replacement_row_order_not_source_order() {
    let mut result = DiffResult::default();
    result.text(b"L0\nL1\n", b"R0\nR1\n");
    assert_eq!(result.lines.len(), 4);
    assert_eq!(result.lines[0].kind, LineKind::Removed);
    assert_eq!(result.lines[2].kind, LineKind::Added);
    let rows = hunks::flatten(&result, &BTreeSet::new(), Decision::Upload);
    let sources: Vec<_> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Line(source) => Some(*source),
            _ => None,
        })
        .collect();
    assert_eq!(sources, [2, 3, 0, 1]);
    assert_both_directions(
        selection((3, 1), (0, 2)),
        &result,
        &rows,
        "1\nL0",
        &[Some(0..2), None, None, Some(1..2)],
    );
    assert_both_directions(
        selection((2, 0), (1, 2)),
        &result,
        &rows,
        "R0\nR1\nL0\nL1",
        &[Some(0..2), Some(0..2), Some(0..2), Some(0..2)],
    );
}

#[test]
fn end_to_next_start_selects_a_newline_including_across_a_header() {
    let result = equal_result("one\ntwo\n");
    let rows = [
        Row::Line(0),
        Row::Header {
            source: 1,
            text: "@@ not file content @@".into(),
        },
        Row::Line(1),
    ];
    assert_both_directions(
        selection((0, 3), (1, 0)),
        &result,
        &rows,
        "\n",
        &[Some(3..3), Some(0..0)],
    );
}

#[test]
fn full_empty_lines_and_empty_endpoint_slices_are_preserved() {
    let result = equal_result("one\n\n\ntwo\n");
    assert_eq!(result.lines.len(), 4);
    let rows = hunks::flatten(&result, &BTreeSet::new(), Decision::Skip);
    assert_both_directions(
        selection((0, 3), (3, 0)),
        &result,
        &rows,
        "\n\n\n",
        &[Some(3..3), Some(0..0), Some(0..0), Some(0..0)],
    );
    assert_both_directions(
        selection((0, 0), (3, 3)),
        &result,
        &rows,
        "one\n\n\ntwo",
        &[Some(0..3), Some(0..0), Some(0..0), Some(0..3)],
    );
    assert_both_directions(
        selection((1, 0), (2, 0)),
        &result,
        &rows,
        "\n",
        &[None, Some(0..0), Some(0..0), None],
    );
}

#[test]
fn folds_exclude_content_but_keep_hidden_endpoints_ranked() {
    let result =
        equal_result("zero\nhidden-a\nhidden-b\nhidden-c\nfour\n\nhidden-d\nhidden-e\neight\n");
    let rows = [
        Row::Line(0),
        Row::Fold {
            gap: 1,
            start: 1,
            end: 4,
        },
        Row::Header {
            source: 4,
            text: "@@ header @@".into(),
        },
        Row::Line(4),
        Row::Line(5),
        Row::Fold {
            gap: 6,
            start: 6,
            end: 8,
        },
        Row::Line(8),
    ];
    assert_both_directions(
        selection((0, 4), (8, 0)),
        &result,
        &rows,
        "\nfour\n\n",
        &[
            Some(4..4),
            None,
            None,
            None,
            Some(0..4),
            Some(0..0),
            None,
            None,
            Some(0..0),
        ],
    );
    assert_both_directions(
        selection((2, 2), (8, 2)),
        &result,
        &rows,
        "four\n\nei",
        &[
            None,
            None,
            None,
            None,
            Some(0..4),
            Some(0..0),
            None,
            None,
            Some(0..2),
        ],
    );
    assert_both_directions(
        selection((0, 1), (7, 3)),
        &result,
        &rows,
        "ero\nfour\n",
        &[
            Some(1..4),
            None,
            None,
            None,
            Some(0..4),
            Some(0..0),
            None,
            None,
            None,
        ],
    );
    assert_both_directions(
        selection((2, 1), (6, 2)),
        &result,
        &rows,
        "four\n",
        &[
            None,
            None,
            None,
            None,
            Some(0..4),
            Some(0..0),
            None,
            None,
            None,
        ],
    );
    for selected in [selection((1, 1), (3, 2)), selection((2, 1), (2, 4))] {
        assert!(!selected.collapsed());
        assert_both_directions(selected, &result, &rows, "", &vec![None; 9]);
    }
}

#[test]
fn real_flattening_preserves_endpoints_when_context_is_folded_again() {
    let mut result = DiffResult::default();
    let context = "context\n".repeat(10);
    result.text(
        format!("{context}local\n").as_bytes(),
        format!("{context}remote\n").as_bytes(),
    );
    let selected = selection((1, 2), (9, 3));
    let expanded = hunks::flatten(&result, &BTreeSet::from([0]), Decision::Download);
    assert_eq!(
        selected.copy_text(&result, &expanded),
        "ntext\ncontext\ncontext\ncontext\ncontext\ncontext\ncontext\ncontext\ncon",
    );
    let folded = hunks::flatten(&result, &BTreeSet::new(), Decision::Download);
    assert!(matches!(
        folded.first(),
        Some(Row::Fold {
            start: 0,
            end: 7,
            ..
        }),
    ));
    let mut ranges = vec![None; result.lines.len()];
    ranges[7] = Some(0..7);
    ranges[8] = Some(0..7);
    ranges[9] = Some(0..3);
    assert_both_directions(selected, &result, &folded, "context\ncontext\ncon", &ranges);
    assert_eq!(selected, selection((1, 2), (9, 3)));
}

#[test]
fn omitted_preview_endpoints_never_select_unrelated_content() {
    let mut result = DiffResult::default();
    result.text(b"local\n", b"remote\n");
    let selected = selection((0, 1), (1, 3));
    for direction in [Decision::DeleteLocal, Decision::DeleteRemote] {
        let rows = hunks::flatten(&result, &BTreeSet::new(), direction);
        assert_both_directions(selected, &result, &rows, "", &[None, None]);
    }
    let rows = hunks::flatten(&result, &BTreeSet::new(), Decision::DeleteLocal);
    assert_both_directions(
        selection((0, 1), (0, 4)),
        &result,
        &rows,
        "oca",
        &[Some(1..4), None],
    );
    let header_only = [Row::Header {
        source: 0,
        text: "@@ header @@".into(),
    }];
    assert_both_directions(
        selection((0, 0), (0, 4)),
        &result,
        &header_only,
        "",
        &[None, None],
    );
}

#[test]
fn collapsed_carets_and_invalid_sources_have_no_range_or_copy() {
    let result = equal_result("abc\n");
    let rows = [Row::Line(0)];
    let caret = selection((0, 2), (0, 2));
    assert!(caret.collapsed());
    assert_both_directions(caret, &result, &rows, "", &[None]);
    let invalid = selection((0, 0), (usize::MAX, 1));
    assert!(!invalid.collapsed());
    assert_both_directions(invalid, &result, &rows, "", &[None]);
    assert_eq!(invalid.head.clipped(&result), None);
    assert_eq!(caret.range_for(usize::MAX, &result, &rows), None);
    assert_both_directions(selection((0, 0), (0, 3)), &result, &[], "", &[None]);
    let empty = DiffResult::default();
    assert_eq!(caret.copy_text(&empty, &rows), "");
    assert_eq!(caret.range_for(0, &empty, &rows), None);
}

#[test]
fn out_of_bounds_and_mid_utf8_positions_are_safe_and_clamped() {
    let result = equal_result("e\u{301}👩‍💻日\n");
    let rows = [Row::Line(0)];
    let text = &result.lines[0].text;
    assert_both_directions(
        selection((0, 2), (0, usize::MAX)),
        &result,
        &rows,
        text,
        &[Some(0..text.len())],
    );
    assert_eq!(
        TextPoint { source: 0, byte: 2 }.clipped(&result),
        Some(TextPoint { source: 0, byte: 0 }),
    );
    for selected in [
        selection((0, 1), (0, 2)),
        selection((0, text.len()), (0, usize::MAX)),
    ] {
        assert!(!selected.collapsed());
        assert_both_directions(selected, &result, &rows, "", &[None]);
    }
}
