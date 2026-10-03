use super::*;
use drift_app::comparison::{LoadRequest, ScopeOptions};

impl Shell {
    pub(super) fn compare_marked(
        &mut self,
        _: &CompareMarked,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.compare(&ToolbarEvent::CompareMarked, window, cx);
    }

    pub(super) fn compare(
        &mut self,
        event: &ToolbarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(location) = self.browser.read(cx).location().cloned() else {
            return;
        };
        let Some((host, connection)) = self.remote.read(cx).comparison_target() else {
            return;
        };
        let project = self.browser.read(cx).id().project;
        let (local, remote) = match event {
            ToolbarEvent::CompareLocal => {
                let mut paths = self.browser.read(cx).marked();
                if paths.is_empty() {
                    paths.push(self.browser.read(cx).selected().map_or_else(
                        || location.directory.to_string_lossy().into_owned(),
                        str::to_owned,
                    ));
                }
                (paths.into_iter().map(PathBuf::from).collect(), vec![])
            }
            ToolbarEvent::CompareRemote => {
                let mut paths = self.remote.read(cx).marked();
                if paths.is_empty() {
                    paths.push(
                        self.remote
                            .read(cx)
                            .selected()
                            .unwrap_or(self.remote.read(cx).directory())
                            .to_owned(),
                    );
                }
                (vec![], paths)
            }
            ToolbarEvent::CompareMarked => {
                let local: Vec<_> = self
                    .browser
                    .read(cx)
                    .marked()
                    .into_iter()
                    .map(PathBuf::from)
                    .collect();
                let remote = self.remote.read(cx).marked();
                if local.is_empty() && remote.is_empty() {
                    self.status = "No files marked — use Space to mark files first".into();
                    cx.notify();
                    return;
                }
                (local, remote)
            }
            _ => (vec![], vec![]),
        };
        self.preview
            .update(cx, |pane, cx| pane.select(None, project, window, cx));
        self.comparison.update(cx, |pane, cx| {
            pane.open(
                LoadRequest {
                    location,
                    host,
                    connection,
                    local,
                    remote,
                    scope: ScopeOptions::default(),
                },
                project,
                window,
                cx,
            )
        });
    }
}
