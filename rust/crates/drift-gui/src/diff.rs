//! Virtualized read-only unified diff. Folding/selection belong to this view.
use crate::actions::*;
use drift_core::diff::{
    Decision, DiffResult, LineKind,
    hunks::{self, Row},
};
use gpui_kit::component::{ActiveTheme, button::Button};
#[cfg(test)]
use gpui_kit::test::TestSupportExt;
use gpui_kit::{
    App, ClipboardItem, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollStrategy, StatefulInteractiveElement, Styled,
    UniformListScrollHandle, Window, div, px, uniform_list,
};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc, sync::Arc};

mod interaction;
mod selection;
mod text;
mod view;
mod viewport;
use selection::{Selection, TextPoint};

const ROW_HEIGHT: f32 = 25.;

#[derive(Clone)]
pub struct DiffState {
    pub expanded: BTreeSet<usize>,
    pub direction: Decision,
    pub scroll: UniformListScrollHandle,
    pub selection: Option<Selection>,
    pub scroll_anchor: Option<usize>,
    pub rendered_direction: Option<Decision>,
}
impl Default for DiffState {
    fn default() -> Self {
        Self {
            expanded: BTreeSet::new(),
            direction: Decision::Skip,
            scroll: UniformListScrollHandle::new(),
            selection: None,
            scroll_anchor: None,
            rendered_direction: None,
        }
    }
}
impl DiffState {
    fn top_row(&self) -> usize {
        let state = self.scroll.0.borrow();
        state.deferred_scroll_to_item.as_ref().map_or_else(
            || {
                // GPUI's last_item_size.item is the viewport, not a row.
                (-state.base_handle.offset().y / px(ROW_HEIGHT))
                    .floor()
                    .max(0.) as usize
            },
            |deferred| deferred.item_index,
        )
    }
}
pub struct DiffPane {
    result: Option<Arc<DiffResult>>,
    pub state: DiffState,
    rows: Vec<Row>,
    message: String,
    focus: FocusHandle,
    geometry: Rc<RefCell<text::Geometry>>,
    revision: u64,
    active: bool,
    drag: Option<interaction::Drag>,
    auto_scroll: gpui_kit::base::AutoScroll,
    blur: Option<gpui_kit::Subscription>,
}
impl Focusable for DiffPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl DiffPane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            result: None,
            state: DiffState::default(),
            rows: vec![],
            message: "Select a comparison file".into(),
            focus: cx.focus_handle().tab_stop(true),
            geometry: Rc::new(RefCell::new(text::Geometry::default())),
            revision: 0,
            active: false,
            drag: None,
            auto_scroll: gpui_kit::base::AutoScroll::default(),
            blur: None,
        }
    }
    pub fn show(
        &mut self,
        result: Option<Arc<DiffResult>>,
        state: DiffState,
        message: String,
        cx: &mut Context<Self>,
    ) {
        self.stop_gesture();
        self.revision += 1;
        self.geometry = Rc::new(RefCell::new(text::Geometry::default()));
        self.result = result;
        self.state = state;
        self.message = message;
        self.rows = self.result.as_ref().map_or_else(Vec::new, |r| {
            hunks::flatten(r, &self.state.expanded, self.state.direction)
        });
        if self
            .state
            .rendered_direction
            .is_some_and(|direction| direction != self.state.direction)
            && let Some(source) = self.state.scroll_anchor
        {
            self.state.scroll.scroll_to_item_strict(
                hunks::source_index(&self.rows, source),
                ScrollStrategy::Top,
            );
        }
        cx.notify();
    }
    pub fn snapshot(&self) -> DiffState {
        let mut state = self.state.clone();
        state.scroll_anchor = self.rows.get(state.top_row()).map(Row::source);
        state.rendered_direction = Some(state.direction);
        state
    }
    pub fn deactivate(&mut self) {
        self.active = false;
        self.stop_gesture();
    }
    pub fn direction(&mut self, direction: Decision, cx: &mut Context<Self>) {
        self.state.direction = direction;
        self.rebuild(cx);
    }
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.stop_gesture();
        self.revision += 1;
        let top = self.state.top_row();
        let anchor = self.rows.get(top).map_or(0, Row::source);
        self.rows = self.result.as_ref().map_or_else(Vec::new, |r| {
            hunks::flatten(r, &self.state.expanded, self.state.direction)
        });
        self.state
            .scroll
            .scroll_to_item_strict(hunks::source_index(&self.rows, anchor), ScrollStrategy::Top);
        cx.notify();
    }
    fn select(&mut self, index: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        self.focus.focus(window, cx);
        if let Some(Row::Fold { gap, .. }) = self.rows.get(index) {
            self.state.expanded.insert(*gap);
            self.rebuild(cx);
            return;
        }
        if let Some(Row::Line(source)) = self.rows.get(index) {
            let point = TextPoint {
                source: *source,
                byte: 0,
            };
            let anchor = if extend {
                self.state
                    .selection
                    .map_or(point, |selection| selection.anchor)
            } else {
                point
            };
            let head = TextPoint {
                source: *source,
                byte: self.result.as_ref().unwrap().lines[*source].text.len(),
            };
            self.state.selection = Some(Selection { anchor, head });
            cx.notify();
        }
    }
    fn hunk(&mut self, next: bool, cx: &mut Context<Self>) {
        let top = self.state.top_row();
        let headers: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| matches!(r, Row::Header { .. }).then_some(i))
            .collect();
        let target = if next {
            headers.iter().copied().find(|i| *i > top)
        } else {
            headers.iter().copied().rev().find(|i| *i < top)
        };
        if let Some(index) = target {
            self.state
                .scroll
                .scroll_to_item_strict(index, ScrollStrategy::Top);
            cx.notify();
        }
    }
    fn page_rows(&self) -> usize {
        self.state
            .scroll
            .0
            .borrow()
            .last_item_size
            .map_or(1, |size| {
                (size.item.height / px(ROW_HEIGHT)).floor().max(1.) as usize
            })
    }
    fn scroll_rows(&mut self, down: bool, rows: usize, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let top = self.state.top_row();
        let target = if down {
            top.saturating_add(rows)
        } else {
            top.saturating_sub(rows)
        }
        .min(self.rows.len().saturating_sub(1));
        self.state
            .scroll
            .scroll_to_item_strict(target, ScrollStrategy::Top);
        cx.notify();
    }
    fn expand_visible(&mut self, cx: &mut Context<Self>) {
        let top = self.state.top_row();
        if let Some(gap) = self
            .rows
            .iter()
            .skip(top)
            .take(self.page_rows())
            .find_map(|row| {
                if let Row::Fold { gap, .. } = row {
                    Some(*gap)
                } else {
                    None
                }
            })
        {
            self.state.expanded.insert(gap);
            self.rebuild(cx);
        }
    }
    fn fold_visible(&mut self, cx: &mut Context<Self>) {
        let Some(result) = &self.result else {
            return;
        };
        let top = self.state.top_row();
        let visible: Vec<_> = self
            .rows
            .iter()
            .skip(top)
            .take(self.page_rows())
            .map(Row::source)
            .collect();
        let folded = hunks::flatten(result, &BTreeSet::new(), self.state.direction);
        if let Some(gap) = folded.iter().find_map(|row| match row {
            Row::Fold { gap, start, end }
                if self.state.expanded.contains(gap)
                    && visible.iter().any(|source| (*start..*end).contains(source)) =>
            {
                Some(*gap)
            }
            _ => None,
        }) {
            self.state.expanded.remove(&gap);
            self.rebuild(cx);
        }
    }
    fn toggle_folds(&mut self, cx: &mut Context<Self>) {
        let Some(result) = &self.result else {
            return;
        };
        let gaps: BTreeSet<_> = hunks::flatten(result, &BTreeSet::new(), self.state.direction)
            .iter()
            .filter_map(|row| {
                if let Row::Fold { gap, .. } = row {
                    Some(*gap)
                } else {
                    None
                }
            })
            .collect();
        if gaps.is_subset(&self.state.expanded) {
            self.state.expanded.clear();
        } else {
            self.state.expanded.extend(gaps);
        }
        self.rebuild(cx);
    }
    fn row_text(&self, row: &Row) -> String {
        match row {
            Row::Header { text, .. } => text.clone(),
            Row::Fold { start, end, .. } => format!("… {} unchanged lines …", end - start),
            Row::Line(index) => {
                let Some(result) = &self.result else {
                    return String::new();
                };
                let l = &result.lines[*index];
                let (old, new, kind) = hunks::line(l, self.state.direction);
                format!(
                    "{:>5} {:>5} {} {}",
                    old.map_or_else(String::new, |n| n.to_string()),
                    new.map_or_else(String::new, |n| n.to_string()),
                    match kind {
                        LineKind::Equal => " ",
                        LineKind::Added => "+",
                        LineKind::Removed => "-",
                    },
                    l.text
                )
            }
        }
    }
    fn copy(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        let text = if let Some(selection) = self
            .state
            .selection
            .filter(|selection| !selection.collapsed())
        {
            self.result.as_ref().map_or_else(String::new, |result| {
                selection.copy_text(result, &self.rows)
            })
        } else {
            self.rows
                .iter()
                .map(|row| self.row_text(row))
                .collect::<Vec<_>>()
                .join("\n")
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod tests;
