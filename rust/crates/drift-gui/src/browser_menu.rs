//! Pane-owned menus outlive virtualized rows, but not their browser context.
use gpui_kit::base::POPUP_PRIORITY;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{
    Anchor, App, DismissEvent, Edges, Entity, EntityId, FocusHandle, Focusable, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, Point, Subscription, Window, anchored,
    deferred, div, px,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuAction {
    Open,
    ToggleTree,
    Mark,
    MarkSiblings,
    InvertMarks,
    ClearMarks,
    ComparePath,
    CompareMarked,
    CompareProject,
    CopyPath,
    Back,
    Forward,
    Up,
    Refresh,
    Hidden,
    Ignored,
    Find,
    Cancel,
    Disconnect,
}

pub(super) struct MenuEntry {
    pub action: MenuAction,
    label: &'static str,
    pub enabled: bool,
    checked: bool,
}
impl MenuEntry {
    pub fn new(action: MenuAction, label: &'static str, enabled: bool) -> Self {
        Self {
            action,
            label,
            enabled,
            checked: false,
        }
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

pub(super) struct BrowserMenu {
    pub popup: Entity<PopupMenu>,
    // Item handlers run while the popup entity is mutably borrowed.
    pub focus: FocusHandle,
    position: Point<Pixels>,
    _subscription: Subscription,
}
impl BrowserMenu {
    pub fn open(
        entries: Vec<Option<MenuEntry>>,
        focus: FocusHandle,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
        handler: impl Fn(MenuAction, &mut Window, &mut App) + 'static,
        dismiss: impl Fn(EntityId, &mut Window, &mut App) + 'static,
    ) -> Self {
        let handler = Rc::new(handler);
        let popup = PopupMenu::build(window, cx, move |menu, _, _| {
            entries.into_iter().fold(
                menu.action_context(focus).scrollable(true),
                |menu, entry| {
                    let Some(entry) = entry else {
                        return menu.separator();
                    };
                    let handler = handler.clone();
                    menu.item(
                        PopupMenuItem::new(entry.label)
                            .disabled(!entry.enabled)
                            .checked(entry.checked)
                            .on_click(move |_, window, cx| {
                                // Kit 0.7 can keyboard-confirm a disabled item.
                                if entry.enabled {
                                    handler(entry.action, window, cx);
                                }
                            }),
                    )
                },
            )
        });
        let id = popup.entity_id();
        let subscription = window.subscribe(&popup, cx, move |_, _: &DismissEvent, window, cx| {
            dismiss(id, window, cx);
        });
        let focus = popup.focus_handle(cx);
        focus.focus(window, cx);
        Self {
            popup,
            focus,
            position,
            _subscription: subscription,
        }
    }
    pub fn render(&self) -> impl IntoElement {
        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .position(self.position)
                .snap_to_window_with_margin(Edges::all(px(8.)))
                .child(
                    div()
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .child(self.popup.clone()),
                ),
        )
        .with_priority(POPUP_PRIORITY)
    }
}
