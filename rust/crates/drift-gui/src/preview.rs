//! Read-only preview state and the lifetime of its own background request.
use crate::actions::Cancel;
use crate::design::{Palette, Text, control, metric, size, space};
use drift_app::browser::{BrowserService, Location, OperationId};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    button::Button,
    input::{Editor, EditorState},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, ClipboardItem, Context, Entity, EventEmitter, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    div,
};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub enum PreviewEvent {
    Loaded(OperationId),
    Failed(OperationId, String),
    Copied(String),
    TextCopied(OperationId),
    Close(OperationId),
}
impl EventEmitter<PreviewEvent> for PreviewPane {}
pub struct PreviewPane {
    editor: Entity<EditorState>,
    service: BrowserService,
    title: String,
    text: String,
    selection: Option<PathBuf>,
    id: OperationId,
    cancel: Option<CancellationToken>,
    remote: bool,
    requested: bool,
    ready: bool,
    external_chrome: bool,
}
impl Drop for PreviewPane {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
    }
}
impl PreviewPane {
    pub fn new(service: BrowserService, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            editor: cx.new(|cx| {
                EditorState::new(window, cx)
                    .line_number(true)
                    .soft_wrap(true)
            }),
            service,
            title: "Preview".into(),
            text: String::new(),
            selection: None,
            id: OperationId {
                project: 0,
                operation: 0,
            },
            cancel: None,
            remote: false,
            requested: false,
            ready: false,
            external_chrome: false,
        }
    }
    pub fn with_external_chrome(mut self) -> Self {
        self.external_chrome = true;
        self
    }

    pub fn controls_header(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let palette = Palette::current(cx);
        div()
            .id("preview-header")
            .test_support()
            .key_context("DriftPreview")
            .flex()
            .flex_wrap()
            .flex_shrink_0()
            .items_center()
            .min_w_0()
            .min_h(metric(size::HEADER, cx))
            .px(metric(space::PANEL, cx))
            .py(metric(space::TIGHT, cx))
            .gap(metric(space::CONTROL, cx))
            .text_size(Text::Body.rems())
            .bg(palette.chrome)
            .border_b_1()
            .border_color(palette.border)
            .child("Preview")
            .child(
                div()
                    .text_size(Text::Metadata.rems())
                    .text_color(palette.muted)
                    .child("Read-only"),
            )
            .child(
                control(Button::new("copy-selection"), cx)
                    .label("Copy path")
                    .disabled(self.selection.is_none())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.copy_path(cx);
                    })),
            )
            .child(
                control(Button::new("copy-preview"), cx)
                    .label("Copy text")
                    .disabled(!self.ready)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.copy_text(cx);
                    })),
            )
            .into_any_element()
    }

    pub fn path_header(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let palette = Palette::current(cx);
        div()
            .id("preview-location")
            .test_support()
            .aria_label(self.title.clone())
            .tooltip({
                let title = self.title.clone();
                move |w, cx| gpui_kit::component::tooltip::Tooltip::new(title.clone()).build(w, cx)
            })
            .flex_shrink_0()
            .min_w_0()
            .h(metric(size::HEADER, cx))
            .min_h(metric(size::HEADER, cx))
            .max_h(metric(size::HEADER, cx))
            .px(metric(space::PANEL, cx))
            .py(metric(space::TIGHT, cx))
            .text_size(Text::Metadata.rems())
            .text_color(palette.muted)
            .bg(palette.chrome)
            .border_b_1()
            .border_color(palette.border)
            .truncate()
            .child(self.title.clone())
            .into_any_element()
    }

    pub fn id(&self) -> OperationId {
        self.id
    }
    pub fn requested_for(&self, project: u64, path: &Path, remote: bool) -> bool {
        self.requested
            && self.id.project == project
            && self.remote == remote
            && self.selection.as_deref() == Some(path)
    }
    pub fn can_show(&self, project: u64) -> bool {
        self.requested && self.id.project == project && (self.ready || self.is_loading())
    }
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.editor.focus_handle(cx).focus(window, cx);
    }
    pub fn copy_path(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(path) = &self.selection else {
            return false;
        };
        let path = path.to_string_lossy().into_owned();
        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
        cx.emit(PreviewEvent::Copied(path));
        true
    }
    pub fn copy_text(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.ready {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(self.text.clone()));
        cx.emit(PreviewEvent::TextCopied(self.id));
        true
    }
    pub fn is_loading(&self) -> bool {
        self.cancel.is_some()
    }
    pub fn select(
        &mut self,
        selection: Option<PathBuf>,
        project: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.id = OperationId {
            project,
            operation: self.id.operation + 1,
        };
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.title = "Preview".into();
        self.remote = false;
        self.requested = false;
        self.ready = false;
        self.text.clear();
        self.selection = selection;
        self.editor
            .update(cx, |editor, cx| editor.set_value("", window, cx));
        cx.notify();
    }
    pub fn show_remote(
        &mut self,
        session: drift_app::remote::RemoteSession,
        path: String,
        project: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select(Some(path.clone().into()), project, window, cx);
        self.remote = true;
        self.requested = true;
        self.title = format!("{}: {path}", session.host_name);
        let id = self.id;
        let operation =
            drift_app::remote::RemoteService::new(self.service.clone()).preview(session, path, id);
        self.cancel = Some(operation.cancel);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.id != id {
                    return;
                }
                this.cancel = None;
                match result {
                    Ok(Ok(text)) => {
                        this.editor
                            .update(cx, |editor, cx| editor.set_value(text.clone(), window, cx));
                        this.text = text;
                        this.ready = true;
                        cx.emit(PreviewEvent::Loaded(id));
                    }
                    Ok(Err(error)) => cx.emit(PreviewEvent::Failed(id, error.to_string())),
                    Err(error) => cx.emit(PreviewEvent::Failed(id, error.to_string())),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub fn show(
        &mut self,
        location: Location,
        path: PathBuf,
        project: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select(Some(path.clone()), project, window, cx);
        self.requested = true;
        self.title = path.display().to_string();
        let id = self.id;
        let operation = self.service.preview(location, path, id);
        self.cancel = Some(operation.cancel);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.id != id {
                    return;
                }
                this.cancel = None;
                match result {
                    Ok(Ok(text)) => {
                        this.editor
                            .update(cx, |editor, cx| editor.set_value(text.clone(), window, cx));
                        this.text = text;
                        this.ready = true;
                        cx.emit(PreviewEvent::Loaded(id));
                    }
                    Ok(Err(error)) => cx.emit(PreviewEvent::Failed(id, error.to_string())),
                    Err(error) => cx.emit(PreviewEvent::Failed(id, error.to_string())),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl Render for PreviewPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::current(cx);
        let controls = self.controls_header(cx);
        let path = self.path_header(cx);
        div()
            .id("preview")
            .test_support()
            .key_context("DriftPreview")
            .on_action(cx.listener(|this, _: &Cancel, _, cx| {
                cx.emit(PreviewEvent::Close(this.id));
            }))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(palette.canvas)
            .text_color(palette.text)
            .when(!self.external_chrome, |view| {
                view.child(controls).child(path)
            })
            .child(
                div()
                    .id("preview-pane")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .child(Editor::new(&self.editor).readonly(true).size_full()),
            )
    }
}

#[cfg(test)]
mod design_tests;
#[cfg(test)]
mod tests;
