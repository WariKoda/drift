use super::{selection::clip_grapheme, *};
use gpui_kit::{
    App, Bounds, Element, ElementId, GlobalElementId, Hsla, InspectorElementId, LayoutId, Pixels,
    Point, SharedString, StyledText, TextLayout, fill,
};
use std::{cell::RefCell, ops::Range, rc::Rc};

#[derive(Default)]
pub(super) struct Geometry {
    pub viewport: Option<Bounds<Pixels>>,
    pub lines: Vec<LineGeometry>,
}
pub(super) struct LineGeometry {
    pub source: usize,
    pub bounds: Bounds<Pixels>,
    pub layout: TextLayout,
    pub text: SharedString,
}
impl Geometry {
    pub fn point(&self, position: Point<Pixels>, exact: bool) -> Option<TextPoint> {
        let line = self
            .lines
            .iter()
            .filter(|line| {
                !exact
                    || (position.y >= line.bounds.top()
                        && position.y < line.bounds.top() + px(ROW_HEIGHT))
            })
            .min_by(|a, b| {
                let distance = |line: &LineGeometry| {
                    (f32::from(position.y - line.bounds.top()) - ROW_HEIGHT / 2.).abs()
                };
                distance(a).total_cmp(&distance(b))
            })?;
        let probe = gpui_kit::point(
            position.x,
            line.bounds.top() + line.layout.line_height() / 2.,
        );
        let byte = line
            .layout
            .index_for_position(probe)
            .unwrap_or_else(|index| index);
        Some(TextPoint {
            source: line.source,
            byte: clip_grapheme(&line.text, byte),
        })
    }
    pub fn gutter(&self, position: Point<Pixels>, source: usize) -> bool {
        self.lines
            .iter()
            .find(|line| line.source == source)
            .is_some_and(|line| position.x < line.bounds.left())
    }
}

pub(super) struct TextRun {
    source: usize,
    text: SharedString,
    styled: StyledText,
    selected: Option<Range<usize>>,
    caret: Option<usize>,
    color: Hsla,
    geometry: Rc<RefCell<Geometry>>,
}
impl TextRun {
    pub fn new(
        source: usize,
        text: SharedString,
        selected: Option<Range<usize>>,
        caret: Option<usize>,
        color: Hsla,
        geometry: Rc<RefCell<Geometry>>,
    ) -> Self {
        Self {
            source,
            styled: StyledText::new(text.clone()),
            text,
            selected,
            caret,
            color,
            geometry,
        }
    }
}
impl IntoElement for TextRun {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for TextRun {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        Some(("diff-text", self.source).into())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.styled.request_layout(id, inspector, w, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) {
        self.styled.prepaint(id, inspector, bounds, &mut (), w, cx);
        self.geometry.borrow_mut().lines.push(LineGeometry {
            source: self.source,
            bounds,
            layout: self.styled.layout().clone(),
            text: self.text.clone(),
        });
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.styled.layout();
        if let Some(range) = &self.selected
            && let (Some(start), Some(end)) = (
                layout.position_for_index(range.start),
                layout.position_for_index(range.end),
            )
        {
            w.paint_quad(fill(
                Bounds::from_corners(
                    start,
                    gpui_kit::point(end.x.max(start.x + px(2.)), end.y + layout.line_height()),
                ),
                self.color,
            ));
        }
        if let Some(byte) = self.caret
            && let Some(position) = layout.position_for_index(byte)
        {
            w.paint_quad(fill(
                Bounds::new(position, gpui_kit::size(px(1.), layout.line_height())),
                self.color,
            ));
        }
        self.styled
            .paint(id, inspector, bounds, &mut (), &mut (), w, cx);
    }
}
