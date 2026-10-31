use super::*;
use gpui_kit::NavigationDirection;
use gpui_kit::base::Disableable;

type Stamp = (OperationId, u64, u64, u64, bool);

impl BrowserPane {
    pub fn set_navigation_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.navigation_active != active {
            self.navigation_active = active;
            self.navigation_revision += 1;
            cx.notify();
        }
    }

    pub fn navigation_stamp(&self) -> Stamp {
        (
            self.id(),
            self.navigation_revision,
            self.menu_revision,
            self.menu_serial,
            self.is_loading() || self.tree_pending.is_some(),
        )
    }

    fn navigation_ready(&self, stamp: Stamp) -> bool {
        self.navigation_active
            && stamp == self.navigation_stamp()
            && !self.is_loading()
            && self.tree_pending.is_none()
            && self.menu.is_none()
    }

    fn breadcrumb_paths(&self) -> Vec<PathBuf> {
        let Some(location) = &self.location else {
            return vec![self.start.clone()];
        };
        let mut paths: Vec<_> = location
            .directory
            .ancestors()
            .take_while(|path| location.slug.is_none() || path.starts_with(location.root.base()))
            .map(std::path::Path::to_path_buf)
            .collect();
        paths.reverse();
        paths
    }

    fn breadcrumb_navigation(
        &mut self,
        stamp: Stamp,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.navigation_ready(stamp)
            || self
                .location
                .as_ref()
                .is_none_or(|location| path == location.directory)
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
        if !self.navigation_ready(stamp) {
            return;
        }
        match direction {
            NavigationDirection::Back if self.can_back() => self.go_back(&GoBack, window, cx),
            NavigationDirection::Forward if self.can_forward() => {
                self.go_forward(&GoForward, window, cx)
            }
            _ => {}
        }
    }

    pub(super) fn breadcrumbs(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let paths = self.breadcrumb_paths();
        let stamp = self.navigation_stamp();
        let last = paths.len().saturating_sub(1);
        div()
            .id("local-breadcrumbs")
            .test_support()
            .flex_1()
            .flex_basis(metric(size::FILTER / 2., cx))
            .min_w_0()
            .overflow_x_scroll()
            .flex()
            .items_center()
            .children(paths.into_iter().enumerate().map(|(index, path)| {
                let label = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned();
                let crumb = if index == last {
                    div()
                        .id(("local-crumb-current", index))
                        .test_support()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .px(metric(space::CONTROL, cx))
                        .child(label)
                        .into_any_element()
                } else {
                    control(Button::new(("local-crumb", index)), cx)
                        .label(label)
                        .accessibility_label(format!("Open {}", path.display()))
                        .tooltip(path.display().to_string())
                        .flex_shrink_0()
                        .max_w_auto()
                        .disabled(!self.navigation_ready(stamp))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.breadcrumb_navigation(stamp, path.clone(), window, cx);
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
