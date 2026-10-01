//! Fold IDs and source anchors always refer to immutable comparison indices.
use super::{Decision, DiffLine, DiffResult, LineKind};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Line(usize),
    Header {
        source: usize,
        text: String,
    },
    Fold {
        gap: usize,
        start: usize,
        end: usize,
    },
}
impl Row {
    pub fn source(&self) -> usize {
        match self {
            Self::Line(i) | Self::Header { source: i, .. } => *i,
            Self::Fold { start, .. } => *start,
        }
    }
}
pub fn source_index(rows: &[Row], source: usize) -> usize {
    rows.iter()
        .position(|row| match row {
            Row::Line(i) => *i == source,
            Row::Fold { start, end, .. } => (*start..*end).contains(&source),
            _ => false,
        })
        .unwrap_or(0)
}
/// Direction changes reorder replacements: deletions always precede additions.
pub fn line(line: &DiffLine, direction: Decision) -> (Option<usize>, Option<usize>, LineKind) {
    match direction {
        Decision::Upload => (
            line.remote,
            line.local,
            match line.kind {
                LineKind::Added => LineKind::Removed,
                LineKind::Removed => LineKind::Added,
                kind => kind,
            },
        ),
        Decision::DeleteLocal => (line.local, None, LineKind::Removed),
        Decision::DeleteRemote => (line.remote, None, LineKind::Removed),
        _ => (line.local, line.remote, line.kind),
    }
}
pub fn flatten(result: &DiffResult, expanded: &BTreeSet<usize>, direction: Decision) -> Vec<Row> {
    let lines = &result.lines;
    if lines.is_empty() {
        return vec![];
    }
    if matches!(direction, Decision::DeleteLocal | Decision::DeleteRemote) {
        let count = lines
            .iter()
            .filter(|l| line(l, direction).0.is_some())
            .count();
        return std::iter::once(Row::Header {
            source: 0,
            text: format!("@@ -1,{count} +0,0 @@"),
        })
        .chain(
            lines
                .iter()
                .enumerate()
                .filter(|(_, l)| line(l, direction).0.is_some())
                .map(|(i, _)| Row::Line(i)),
        )
        .collect();
    }
    let mut runs = Vec::new();
    let mut start = 0;
    for i in 1..=lines.len() {
        if i == lines.len()
            || (lines[i].kind == LineKind::Equal) != (lines[start].kind == LineKind::Equal)
        {
            runs.push((start, i, lines[start].kind == LineKind::Equal));
            start = i;
        }
    }
    if runs.iter().all(|(_, _, equal)| *equal) {
        return (0..lines.len()).map(Row::Line).collect();
    }
    let mut out = Vec::new();
    let mut leading = Vec::new();
    for (i, &(start, end, equal)) in runs.iter().enumerate() {
        if equal {
            let prefix = if i > 0 { 3 } else { 0 };
            let suffix = if i + 1 < runs.len() { 3 } else { 0 };
            if end - start > 6 && !expanded.contains(&start) {
                out.extend((start..start + prefix).map(Row::Line));
                out.push(Row::Fold {
                    gap: start,
                    start: start + prefix,
                    end: end - suffix,
                });
                leading = (end - suffix..end).map(Row::Line).collect();
            } else if prefix == 0 && suffix > 0 {
                leading = (start..end).map(Row::Line).collect();
            } else {
                let split = if suffix == 0 {
                    end
                } else {
                    end.saturating_sub(suffix).max(start)
                };
                out.extend((start..split).map(Row::Line));
                leading = (split..end).map(Row::Line).collect();
            }
            continue;
        }
        let hstart = if i > 0 {
            let (s, e, _) = runs[i - 1];
            if e - s <= 6 || expanded.contains(&s) {
                s
            } else {
                e - 3
            }
        } else {
            start
        };
        let hend = if i + 1 < runs.len() {
            let (s, e, _) = runs[i + 1];
            if e - s <= 6 || expanded.contains(&s) {
                e
            } else {
                s + 3
            }
        } else {
            end
        };
        let mut old_start = 0;
        let mut new_start = 0;
        let mut old_count = 0;
        let mut new_count = 0;
        for l in &lines[hstart..hend] {
            let (old, new, _) = line(l, direction);
            if let Some(n) = old {
                if old_start == 0 {
                    old_start = n;
                }
                old_count += 1;
            }
            if let Some(n) = new {
                if new_start == 0 {
                    new_start = n;
                }
                new_count += 1;
            }
        }
        for l in lines[..hstart].iter().rev() {
            let (old, new, _) = line(l, direction);
            if old_count == 0 && old_start == 0 {
                old_start = old.unwrap_or(0);
            }
            if new_count == 0 && new_start == 0 {
                new_start = new.unwrap_or(0);
            }
        }
        out.push(Row::Header {
            source: start,
            text: format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@"),
        });
        out.append(&mut leading);
        for kind in [LineKind::Removed, LineKind::Added] {
            out.extend(
                (start..end)
                    .filter(|idx| line(&lines[*idx], direction).2 == kind)
                    .map(Row::Line),
            );
        }
    }
    out
}
