//! Character selection over immutable source lines in the current diff preview.
use drift_core::diff::{DiffResult, hunks::Row};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextPoint {
    pub source: usize,
    pub byte: usize,
}

impl TextPoint {
    pub(super) fn clipped(self, result: &DiffResult) -> Option<Self> {
        let text = &result.lines.get(self.source)?.text;
        Some(Self {
            byte: clip_grapheme(text, self.byte),
            ..self
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub anchor: TextPoint,
    pub head: TextPoint,
}

impl Selection {
    /// Tests the stored endpoints, not their visibility or clipped positions.
    pub(super) fn collapsed(self) -> bool {
        self.anchor == self.head
    }

    /// Empty ranges still participate in newline selection between visible lines.
    #[cfg(test)]
    pub(super) fn range_for(
        self,
        source: usize,
        result: &DiffResult,
        rows: &[Row],
    ) -> Option<Range<usize>> {
        let span = self.span(result, rows)?;
        let text = &result.lines.get(source)?.text;
        let row = rows.iter().position(|row| *row == Row::Line(source))?;
        span.range_for(row, source, text.len())
    }

    /// Copies only visible file content, with LF separators even for empty slices.
    pub(super) fn copy_text(self, result: &DiffResult, rows: &[Row]) -> String {
        let Some(span) = self.span(result, rows) else {
            return String::new();
        };
        let mut text = String::new();
        let mut first = true;
        for (row, entry) in rows.iter().enumerate() {
            let Row::Line(source) = entry else {
                continue;
            };
            let Some(line) = result.lines.get(*source) else {
                continue;
            };
            let Some(range) = span.range_for(row, *source, line.text.len()) else {
                continue;
            };
            if !first {
                text.push('\n');
            }
            text.push_str(&line.text[range]);
            first = false;
        }
        text
    }

    pub(super) fn span(self, result: &DiffResult, rows: &[Row]) -> Option<SelectedSpan> {
        let mut endpoints = [
            (0, self.anchor.clipped(result)?),
            (0, self.head.clipped(result)?),
        ];
        for (rank, point) in &mut endpoints {
            // A hidden endpoint retains its rank at its fold; an omitted endpoint
            // has no rank and must not turn into a selection of unrelated lines.
            *rank = rows.iter().position(|row| match row {
                Row::Line(source) => *source == point.source,
                Row::Fold { start, end, .. } => (*start..*end).contains(&point.source),
                Row::Header { .. } => false,
            })?;
        }
        let [a, b] = endpoints;
        if a == b {
            return None;
        }
        // Source indices only break ties within a fold, never order visible rows:
        // Upload reverses replacement runs without changing source identities.
        if (a.0, a.1.source, a.1.byte) > (b.0, b.1.source, b.1.byte) {
            endpoints.swap(0, 1);
        }
        Some(SelectedSpan {
            start: endpoints[0],
            end: endpoints[1],
        })
    }
}

pub(super) struct SelectedSpan {
    pub(super) start: (usize, TextPoint),
    pub(super) end: (usize, TextPoint),
}

impl SelectedSpan {
    pub(super) fn range_for(&self, row: usize, source: usize, len: usize) -> Option<Range<usize>> {
        if !(self.start.0..=self.end.0).contains(&row) {
            return None;
        }
        let start = if source == self.start.1.source {
            self.start.1.byte
        } else {
            0
        };
        let end = if source == self.end.1.source {
            self.end.1.byte
        } else {
            len
        };
        Some(start..end)
    }
}

/// Returns the greatest extended-grapheme boundary at or below `byte`.
/// Positions beyond the text clamp to its end, including for empty text.
pub fn clip_grapheme(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    text.grapheme_indices(true)
        .map(|(offset, _)| offset)
        .take_while(|offset| *offset <= byte)
        .last()
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "selection/tests.rs"]
mod tests;
