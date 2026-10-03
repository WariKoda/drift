//! Read-only preview state and the lifetime of its own background request.
use drift_app::browser::{BrowserService, Location, OperationId};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    input::{Editor, EditorState},
};
use gpui_kit::{
    AppContext, ClipboardItem, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Window, div,
};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub enum PreviewEvent {
    Loaded(OperationId),
    Failed(OperationId, String),
    Copied(String),
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
        }
    }
    pub fn id(&self) -> OperationId {
        self.id
    }
    pub fn is_loading(&self) -> bool {
        self.cancel.is_some()
    }
    pub fn is_loading_remote(&self) -> bool {
        self.remote && self.is_loading()
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
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .p_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .p_2()
                    .gap_2()
                    .child(
                        Button::new("copy-selection")
                            .label("Copy path")
                            .disabled(self.selection.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(path) = &this.selection {
                                    let path = path.to_string_lossy().into_owned();
                                    cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                                    cx.emit(PreviewEvent::Copied(path));
                                }
                            })),
                    )
                    .child(
                        Button::new("copy-preview")
                            .label("Copy text")
                            .disabled(self.text.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(this.text.clone()));
                            })),
                    ),
            )
            .child(
                div()
                    .id("preview-pane")
                    .flex_1()
                    .min_h_0()
                    .child(Editor::new(&self.editor).readonly(true).size_full()),
            )
    }
}

#[cfg(test)]
mod tests;
