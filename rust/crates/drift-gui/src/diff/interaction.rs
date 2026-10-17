use super::*;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point};
use unicode_segmentation::UnicodeSegmentation;

pub(super) struct Drag {
    pub revision: u64,
    pub pointer: Point<Pixels>,
    pub press: Point<Pixels>,
    pub gutter: bool,
}
impl DiffPane {
    pub fn stop_gesture(&mut self) {
        self.drag = None;
        self.auto_scroll.stop();
    }
    pub(super) fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        self.stop_gesture();
        let geometry = self.geometry.borrow();
        if window.default_prevented()
            || cx.has_active_drag()
            || !self.active
            || !geometry
                .viewport
                .is_some_and(|bounds| bounds.contains(&event.position))
        {
            return;
        }
        let Some(point) = geometry.point(event.position, true) else {
            return;
        };
        let gutter = geometry.gutter(event.position, point.source);
        drop(geometry);
        self.focus.focus(window, cx);
        let anchor = if event.modifiers.shift {
            self.state
                .selection
                .map_or(point, |selection| selection.anchor)
        } else if gutter {
            TextPoint { byte: 0, ..point }
        } else {
            point
        };
        let head = if gutter {
            TextPoint {
                byte: self.result.as_ref().unwrap().lines[point.source].text.len(),
                ..point
            }
        } else {
            point
        };
        self.state.selection = Some(Selection { anchor, head });
        self.drag = Some(Drag {
            revision: self.revision,
            pointer: event.position,
            press: event.position,
            gutter,
        });
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_none() {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left)
            || cx.has_active_drag()
            || !self.focus.is_focused(window)
        {
            self.stop_gesture();
            return;
        }
        self.update_drag(event.position, cx);
        if let Some(bounds) = self.geometry.borrow().viewport {
            self.auto_scroll.set(
                gpui_kit::base::AutoScroll::compute_delta(event.position.y, bounds),
                cx,
                |delta, this, cx| {
                    if !this.active
                        || this
                            .drag
                            .as_ref()
                            .is_none_or(|drag| drag.revision != this.revision)
                    {
                        this.stop_gesture();
                        return;
                    }
                    let scroll = this.state.scroll.0.borrow().base_handle.clone();
                    let offset = scroll.offset();
                    scroll.set_offset(gpui_kit::point(offset.x, offset.y - delta));
                    cx.notify();
                },
            );
        }
        cx.stop_propagation();
    }
    pub(super) fn mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left && self.drag.is_some() {
            self.update_drag(event.position, cx);
            self.stop_gesture();
            cx.stop_propagation();
        }
    }
    pub(super) fn update_drag(&mut self, pointer: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        if drag.revision != self.revision {
            self.stop_gesture();
            return;
        }
        drag.pointer = pointer;
        if drag.gutter
            && (pointer.x - drag.press.x).abs() < px(2.)
            && (pointer.y - drag.press.y).abs() < px(2.)
        {
            return;
        }
        let Some(head) = self.geometry.borrow().point(pointer, false) else {
            return;
        };
        if let Some(selection) = &mut self.state.selection
            && selection.head != head
        {
            selection.head = head;
            cx.notify();
        }
    }
    pub(super) fn move_caret(
        &mut self,
        horizontal: i8,
        vertical: i8,
        edge: Option<bool>,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.active {
            return;
        }
        self.stop_gesture();
        let Some(result) = &self.result else {
            return;
        };
        if !extend
            && horizontal != 0
            && let Some(span) = self
                .state
                .selection
                .and_then(|selection| selection.span(result, &self.rows))
        {
            let point = if horizontal < 0 {
                span.start.1
            } else {
                span.end.1
            };
            self.state.selection = Some(Selection {
                anchor: point,
                head: point,
            });
            self.state.scroll.scroll_to_item(
                hunks::source_index(&self.rows, point.source),
                ScrollStrategy::Nearest,
            );
            cx.notify();
            return;
        }
        let lines: Vec<_> = self
            .rows
            .iter()
            .filter_map(|row| {
                if let Row::Line(source) = row {
                    Some(*source)
                } else {
                    None
                }
            })
            .collect();
        if lines.is_empty() {
            return;
        }
        let initial = self
            .rows
            .iter()
            .skip(self.state.top_row())
            .find_map(|row| {
                if let Row::Line(source) = row {
                    Some(*source)
                } else {
                    None
                }
            })
            .unwrap_or(lines[0]);
        let current = self.state.selection.map_or(
            TextPoint {
                source: initial,
                byte: 0,
            },
            |selection| selection.head,
        );
        let rank = lines
            .iter()
            .position(|source| *source == current.source)
            .unwrap_or(0);
        let mut target = TextPoint {
            source: lines[rank],
            byte: selection::clip_grapheme(&result.lines[lines[rank]].text, current.byte),
        };
        let text = &result.lines[target.source].text;
        if let Some(end) = edge {
            target.byte = if end { text.len() } else { 0 };
        } else if vertical != 0 {
            let column = text[..target.byte].graphemes(true).count();
            let next = if vertical > 0 {
                (rank + 1).min(lines.len() - 1)
            } else {
                rank.saturating_sub(1)
            };
            target.source = lines[next];
            let text = &result.lines[target.source].text;
            target.byte = text
                .grapheme_indices(true)
                .nth(column)
                .map_or(text.len(), |(byte, _)| byte);
        } else if horizontal < 0 {
            if let Some((byte, _)) = text
                .grapheme_indices(true)
                .take_while(|(byte, _)| *byte < target.byte)
                .last()
            {
                target.byte = byte;
            } else if rank > 0 {
                target.source = lines[rank - 1];
                target.byte = result.lines[target.source].text.len();
            }
        } else if horizontal > 0 {
            if let Some((byte, _)) = text
                .grapheme_indices(true)
                .find(|(byte, _)| *byte > target.byte)
            {
                target.byte = byte;
            } else if target.byte < text.len() {
                target.byte = text.len();
            } else if rank + 1 < lines.len() {
                target.source = lines[rank + 1];
                target.byte = 0;
            }
        }
        let anchor = if extend {
            self.state
                .selection
                .map_or(current, |selection| selection.anchor)
        } else {
            target
        };
        self.state.selection = Some(Selection {
            anchor,
            head: target,
        });
        self.state.scroll.scroll_to_item(
            hunks::source_index(&self.rows, target.source),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    pub(super) fn select_all(&mut self, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        self.stop_gesture();
        let Some(result) = &self.result else {
            return;
        };
        let mut sources = self.rows.iter().filter_map(|row| {
            if let Row::Line(source) = row {
                Some(*source)
            } else {
                None
            }
        });
        let Some(first) = sources.next() else {
            return;
        };
        let last = sources.next_back().unwrap_or(first);
        self.state.selection = Some(Selection {
            anchor: TextPoint {
                source: first,
                byte: 0,
            },
            head: TextPoint {
                source: last,
                byte: result.lines[last].text.len(),
            },
        });
        cx.notify();
    }
}
