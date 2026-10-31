use super::*;
use gpui_kit::NavigationDirection;
use gpui_kit::base::Selectable;

pub(super) type Stamp = (u64, u64, u64, u64, u64, u64, bool);

#[derive(Clone, Copy)]
enum Control {
    Back,
    Forward,
    Up,
    Refresh,
    Ignored,
    Hidden,
}

// Remote paths are POSIX strings. Slice the original string, not a host-platform
// PathBuf or a normalized reconstruction (which could change a symlink target).
pub(super) fn parent_path(path: &str) -> Option<&str> {
    if path == "/" || path.is_empty() {
        return None;
    }
    let end = path.trim_end_matches('/').rfind('/')?;
    Some(&path[..end.max(1)])
}

impl RemotePane {
    pub fn set_workspace_active(&mut self, workspace: bool, files: bool, cx: &mut Context<Self>) {
        let files = workspace && files;
        if self.workspace_active != workspace || self.navigation_active != files {
            self.workspace_active = workspace;
            self.navigation_active = files;
            self.navigation_revision += 1;
            cx.notify();
        }
    }

    pub fn navigation_stamp(&self) -> Stamp {
        (
            self.project,
            self.connection,
            self.operation,
            self.menu_revision,
            self.visibility_revision,
            self.navigation_revision,
            self.is_loading() || self.tree_pending.is_some(),
        )
    }

    pub(super) fn session_controls_active(&self, stamp: Stamp) -> bool {
        self.workspace_active && stamp == self.navigation_stamp() && self.menu.is_none()
    }

    fn navigation_ready(&self, stamp: Stamp) -> bool {
        self.navigation_active
            && stamp == self.navigation_stamp()
            && !self.is_loading()
            && self.tree_pending.is_none()
            && self.menu.is_none()
            && self
                .session
                .as_ref()
                .is_some_and(|s| s.state() == ConnectionState::Connected && s.contains(&self.path))
    }

    fn breadcrumb_paths(&self) -> Vec<String> {
        let Some(session) = &self.session else {
            return vec![];
        };
        let mut paths = vec![];
        let mut path = self.path.as_str();
        while session.contains(path) {
            paths.push(path.to_owned());
            if path == session.root {
                break;
            }
            let Some(parent) = parent_path(path) else {
                break;
            };
            path = parent;
        }
        paths.reverse();
        paths
    }

    fn breadcrumb_navigation(
        &mut self,
        stamp: Stamp,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.navigation_ready(stamp)
            || path == self.path
            || !self.breadcrumb_paths().contains(&path)
        {
            return;
        }
        self.navigate(path, Navigation::Push, window, cx);
    }

    pub fn mouse_navigation(
        &mut self,
        stamp: Stamp,
        direction: NavigationDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let control = match direction {
            NavigationDirection::Back => Control::Back,
            NavigationDirection::Forward => Control::Forward,
        };
        self.navigation_control(stamp, control, window, cx);
    }

    fn control_enabled(&self, stamp: Stamp, control: Control) -> bool {
        if !self.navigation_ready(stamp) {
            return false;
        }
        match control {
            Control::Back => self.history.previous().is_some(),
            Control::Forward => self.history.next().is_some(),
            Control::Up => parent_path(&self.path)
                .is_some_and(|p| self.session.as_ref().is_some_and(|s| s.contains(p))),
            Control::Refresh | Control::Hidden => true,
            Control::Ignored => self.selection_location.is_some() && self.visibility.is_some(),
        }
    }

    fn navigation_control(
        &mut self,
        stamp: Stamp,
        control: Control,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.control_enabled(stamp, control) {
            return;
        }
        match control {
            Control::Back => self.back(&GoBack, window, cx),
            Control::Forward => self.forward(&GoForward, window, cx),
            Control::Up => self.up(&GoUp, window, cx),
            Control::Refresh => self.refresh(&Refresh, window, cx),
            Control::Ignored => self.toggle_ignored(window, cx),
            Control::Hidden => self.toggle_hidden(cx),
        }
    }

    pub(super) fn toggle_hidden(&mut self, cx: &mut Context<Self>) {
        if self.is_loading() {
            return;
        }
        self.navigation_revision += 1;
        self.show_hidden = !self.show_hidden;
        self.set_entries(cx);
        cx.notify();
    }

    pub fn navigation_controls(
        &self,
        trailing: Option<gpui_kit::AnyElement>,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let stamp = self.navigation_stamp();
        let controls = [
            ("remote-back", "Back", IconName::ArrowLeft, Control::Back),
            (
                "remote-forward",
                "Forward",
                IconName::ArrowRight,
                Control::Forward,
            ),
            ("remote-up", "Up", IconName::ArrowUp, Control::Up),
            (
                "remote-refresh",
                "Refresh",
                IconName::RefreshCw,
                Control::Refresh,
            ),
            (
                "remote-ignored",
                if self.show_ignored {
                    "Hide ignored"
                } else {
                    "Show ignored"
                },
                IconName::EyeOff,
                Control::Ignored,
            ),
            (
                "remote-hidden",
                if self.show_hidden {
                    "Hide hidden"
                } else {
                    "Show hidden"
                },
                IconName::Eye,
                Control::Hidden,
            ),
        ];
        div()
            .id("remote-navigation-toolbar")
            .test_support()
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(move |this, _, w, cx| {
                    cx.stop_propagation();
                    this.mouse_navigation(stamp, NavigationDirection::Back, w, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(move |this, _, w, cx| {
                    cx.stop_propagation();
                    this.mouse_navigation(stamp, NavigationDirection::Forward, w, cx);
                }),
            )
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(metric(size::HEADER, cx))
            .overflow_x_scroll()
            .min_w_0()
            .gap(metric(space::TIGHT, cx))
            .px(metric(space::PANEL, cx))
            .py(metric(space::TIGHT, cx))
            .bg(Palette::current(cx).chrome)
            .border_b_1()
            .border_color(Palette::current(cx).border)
            .children(controls.into_iter().map(|(id, label, icon, control_id)| {
                control(Button::new(id), cx)
                    .w(metric(size::CONTROL, cx))
                    .flex_shrink_0()
                    .selected(match control_id {
                        Control::Hidden => self.show_hidden,
                        Control::Ignored => self.show_ignored,
                        _ => false,
                    })
                    .icon(icon)
                    .accessibility_label(label)
                    .tooltip(label)
                    .disabled(!self.control_enabled(stamp, control_id))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigation_control(stamp, control_id, window, cx)
                    }))
            }))
            .children(trailing)
            .into_any_element()
    }

    pub(super) fn breadcrumbs(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let paths = self.breadcrumb_paths();
        let stamp = self.navigation_stamp();
        let last = paths.len().saturating_sub(1);
        div()
            .id("remote-breadcrumbs")
            .test_support()
            .min_w_0()
            .w_full()
            .flex_1()
            .overflow_x_scroll()
            .px(metric(space::PANEL, cx))
            .bg(Palette::current(cx).chrome)
            .flex()
            .items_center()
            .children(paths.into_iter().enumerate().map(|(index, path)| {
                let label = path
                    .rsplit('/')
                    .find(|part| !part.is_empty())
                    .unwrap_or("/")
                    .to_owned();
                let crumb = if index == last {
                    div()
                        .id(("remote-crumb-current", index))
                        .test_support()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .px(metric(space::CONTROL, cx))
                        .child(label)
                        .into_any_element()
                } else {
                    control(Button::new(("remote-crumb", index)), cx)
                        .label(label)
                        .accessibility_label(format!("Open {path}"))
                        .tooltip(path.clone())
                        .flex_shrink_0()
                        .max_w_auto()
                        .disabled(!self.navigation_ready(stamp))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.breadcrumb_navigation(stamp, path.clone(), window, cx)
                        }))
                        .into_any_element()
                };
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .children((index > 0).then(|| {
                        div()
                            .text_size(Text::Metadata.rems())
                            .text_color(Palette::current(cx).muted)
                            .child("›")
                    }))
                    .child(crumb)
                    .into_any_element()
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
