use drift_core::diff::{
    Decision, DiffResult, FileMetadata, LineKind,
    hunks::{self, Row},
};
use std::{
    collections::BTreeSet,
    time::{Duration, UNIX_EPOCH},
};

fn text(local: &str, remote: &str) -> DiffResult {
    let mut result = DiffResult {
        local: Some(FileMetadata {
            size: local.len() as u64,
            modified: None,
        }),
        remote: Some(FileMetadata {
            size: remote.len() as u64,
            modified: None,
        }),
        ..Default::default()
    };
    result.text(local.as_bytes(), remote.as_bytes());
    result
}
#[test]
fn direction_reorders_replacements_and_preserves_source_numbers() {
    let result = text("a\nL1\nL2\nz\n", "a\nR\nz\n");
    let original = result.lines.clone();
    let download = hunks::flatten(&result, &BTreeSet::new(), Decision::Download);
    let upload = hunks::flatten(&result, &BTreeSet::new(), Decision::Upload);
    assert!(matches!(&download[0], Row::Header {text,..} if text=="@@ -1,4 +1,3 @@"));
    assert!(matches!(&upload[0], Row::Header {text,..} if text=="@@ -1,3 +1,4 @@"));
    let rows: Vec<_> = upload
        .iter()
        .filter_map(|r| {
            if let Row::Line(i) = r {
                let l = &result.lines[*i];
                Some((l.text.as_str(), hunks::line(l, Decision::Upload)))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("a", (Some(1), Some(1), LineKind::Equal)),
            ("R", (Some(2), None, LineKind::Removed)),
            ("L1", (None, Some(2), LineKind::Added)),
            ("L2", (None, Some(3), LineKind::Added)),
            ("z", (Some(3), Some(4), LineKind::Equal))
        ]
    );
    assert_eq!(result.lines, original);
}
#[test]
fn empty_ranges_deletion_and_newlines_have_correct_coordinates() {
    for (local, remote, direction, header) in [
        ("", "R\n", Decision::Download, "@@ -0,0 +1,1 @@"),
        ("", "R\n", Decision::Upload, "@@ -1,1 +0,0 @@"),
        ("L\n", "", Decision::Upload, "@@ -0,0 +1,1 @@"),
    ] {
        let result = text(local, remote);
        assert!(
            matches!(&hunks::flatten(&result,&BTreeSet::new(),direction)[0],Row::Header {text,..} if text==header)
        );
    }
    let newline = text("same\n", "same");
    assert!(newline.differs());
    assert!(
        text("a\r\nb\r", "a\nb\n")
            .lines
            .iter()
            .all(|l| !l.text.contains('\r'))
    );
    let deletion = hunks::flatten(
        &text("a\nb\n", "a\nb\n"),
        &BTreeSet::new(),
        Decision::DeleteLocal,
    );
    assert!(matches!(&deletion[0],Row::Header {text,..} if text=="@@ -1,2 +0,0 @@"));
}
#[test]
fn folded_context_and_direction_changes_preserve_source_anchors() {
    let result = text(
        &("before\n".repeat(12) + "L1\nL2\n" + &"after\n".repeat(12)),
        &("before\n".repeat(12) + "R\n" + &"after\n".repeat(12)),
    );
    let closed = hunks::flatten(&result, &BTreeSet::new(), Decision::Download);
    let Row::Fold { gap, start, end } = closed[0] else {
        panic!("expected leading fold")
    };
    assert_eq!(end - start, 9);
    let expanded = hunks::flatten(&result, &BTreeSet::from([gap]), Decision::Upload);
    let index = hunks::source_index(&expanded, start);
    assert_eq!(expanded[index], Row::Line(start));
    for (i, l) in result
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind != LineKind::Equal)
    {
        let index = hunks::source_index(&expanded, i);
        assert_eq!(expanded[index], Row::Line(i));
        assert!(!l.text.is_empty());
    }
}
#[test]
fn policy_tolerates_two_seconds_and_handles_missing_or_unknown_times() {
    let mut result = text("local", "remote");
    assert_eq!(result.suggestion(), Decision::Upload);
    result.local.as_mut().unwrap().modified = Some(UNIX_EPOCH + Duration::from_secs(100));
    result.remote.as_mut().unwrap().modified = Some(UNIX_EPOCH + Duration::from_secs(102));
    assert_eq!(result.suggestion(), Decision::Upload);
    result.remote.as_mut().unwrap().modified = Some(UNIX_EPOCH + Duration::from_secs(103));
    assert_eq!(result.suggestion(), Decision::Download);
    result.remote = None;
    assert_eq!(result.suggestion(), Decision::Upload);
    assert_eq!(
        result.next_decision(Decision::Upload),
        Decision::DeleteLocal
    );
    result.local = None;
    result.remote = Some(FileMetadata {
        size: 4,
        modified: None,
    });
    assert_eq!(result.suggestion(), Decision::Download);
    assert_eq!(
        result.next_decision(Decision::Download),
        Decision::DeleteRemote
    );
}
