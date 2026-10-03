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
use std::{collections::BTreeSet, sync::Arc};

const ROW_HEIGHT: f32 = 25.;

#[derive(Clone)]
pub struct DiffState {
    pub expanded: BTreeSet<usize>,
    pub direction: Decision,
    pub scroll: UniformListScrollHandle,
    pub selection: Option<(usize, usize)>,
}
impl Default for DiffState {
    fn default() -> Self {
        Self {
            expanded: BTreeSet::new(),
            direction: Decision::Skip,
            scroll: UniformListScrollHandle::new(),
            selection: None,
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
        }
    }
    pub fn show(
        &mut self,
        result: Option<Arc<DiffResult>>,
        state: DiffState,
        message: String,
        cx: &mut Context<Self>,
    ) {
        self.result = result;
        self.state = state;
        self.message = message;
        self.rows = self.result.as_ref().map_or_else(Vec::new, |r| {
            hunks::flatten(r, &self.state.expanded, self.state.direction)
        });
        cx.notify();
    }
    pub fn direction(&mut self, direction: Decision, cx: &mut Context<Self>) {
        self.state.direction = direction;
        self.rebuild(cx);
    }
    fn rebuild(&mut self, cx: &mut Context<Self>) {
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
        self.focus.focus(window, cx);
        if let Some(Row::Fold { gap, .. }) = self.rows.get(index) {
            self.state.expanded.insert(*gap);
            self.rebuild(cx);
            return;
        }
        if let Some(Row::Line(source)) = self.rows.get(index) {
            let start = if extend {
                self.state.selection.map_or(*source, |(start, _)| start)
            } else {
                *source
            };
            self.state.selection = Some((start, *source));
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
        let text = self
            .rows
            .iter()
            .filter(|row| {
                self.state.selection.is_none_or(
                    |(a, b)| matches!(row,Row::Line(i) if (a.min(b)..=a.max(b)).contains(i)),
                )
            })
            .map(|r| self.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}
impl Render for DiffPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let selected = cx.theme().selection;
        div().id("unified-diff").flex().flex_col().flex_1().min_w_0().min_h_0()
            .key_context("DriftDiff").track_focus(&self.focus)
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(|this, _: &ScrollUp, _, cx| this.scroll_rows(false, 1, cx)))
            .on_action(cx.listener(|this, _: &ScrollDown, _, cx| this.scroll_rows(true, 1, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.scroll_rows(false, this.page_rows(), cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.scroll_rows(true, this.page_rows(), cx)))
            .on_action(cx.listener(|this, _: &HalfPageUp, _, cx| this.scroll_rows(false, (this.page_rows()/2).max(1), cx)))
            .on_action(cx.listener(|this, _: &HalfPageDown, _, cx| this.scroll_rows(true, (this.page_rows()/2).max(1), cx)))
            .on_action(cx.listener(|this, _: &CursorFirst, _, cx| this.scroll_rows(false, usize::MAX, cx)))
            .on_action(cx.listener(|this, _: &CursorLast, _, cx| this.scroll_rows(true, usize::MAX, cx)))
            .on_action(cx.listener(|this, _: &ExpandContext, _, cx| this.expand_visible(cx)))
            .on_action(cx.listener(|this, _: &FoldContext, _, cx| this.fold_visible(cx)))
            .on_action(cx.listener(|this, _: &ToggleFolds, _, cx| this.toggle_folds(cx)))
            .on_action(cx.listener(|this, _: &NextHunk, _, cx| this.hunk(true,cx)))
            .on_action(cx.listener(|this, _: &PreviousHunk, _, cx| this.hunk(false,cx)))
            .child(div().flex().gap_2().p_2()
                .child(Button::new("previous-hunk").label("Previous hunk").on_click(cx.listener(|this,_,_,cx| this.hunk(false,cx))))
                .child(Button::new("next-hunk").label("Next hunk").on_click(cx.listener(|this,_,_,cx| this.hunk(true,cx))))
                .child(Button::new("collapse-context").label("Fold context").on_click(cx.listener(|this,_,_,cx| { this.state.expanded.clear(); this.rebuild(cx); })))
                .child(Button::new("copy-diff").label("Copy diff / selection").on_click(cx.listener(|this,_,w,cx| this.copy(&CopySelection,w,cx)))))
            .child(div().p_2().child(match self.state.direction {
                Decision::Upload => "Remote → Local (Upload preview)", Decision::Download | Decision::Skip => "Local → Remote (Download preview)", Decision::DeleteLocal => "Local → removed", Decision::DeleteRemote => "Remote → removed",
            }))
            .child(if self.rows.is_empty() {
                div().p_3().child(if self.result.as_ref().is_some_and(|r| r.binary) { "Binary or large file: content comparison only".into() } else { self.message.clone() }).into_any_element()
            } else {
                uniform_list("diff-lines", self.rows.len(), cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range.map(|i| {
                        let row = &this.rows[i];
                        let chosen = this.state.selection.is_some_and(|(a,b)| matches!(row,Row::Line(source) if (a.min(b)..=a.max(b)).contains(source)));
                        let color = match row { Row::Line(index) => match hunks::line(&this.result.as_ref().unwrap().lines[*index],this.state.direction).2 { LineKind::Added => cx.theme().success, LineKind::Removed => cx.theme().danger, LineKind::Equal => cx.theme().foreground }, _ => cx.theme().muted_foreground };
                        let element = div().id(("diff-row",i)).h(px(ROW_HEIGHT)).px_2().font_family("monospace").whitespace_nowrap().bg(if chosen { selected } else { background }).text_color(color)
                            .child(this.row_text(row))
                            .on_click(cx.listener(move |this,event: &gpui_kit::ClickEvent,w,cx| this.select(i,event.modifiers().shift,w,cx)));
                        #[cfg(test)]
                        let element = element.test_support();
                        element
                    }).collect()
                })).track_scroll(&self.state.scroll).flex_1().min_h_0().into_any_element()
            })
    }
}

#[cfg(test)]
mod tests;
