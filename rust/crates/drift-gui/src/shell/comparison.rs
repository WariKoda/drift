use super::*;
use drift_app::comparison::{LoadRequest, ScopeOptions};

impl Shell {
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
            ToolbarEvent::CompareLocal => (
                vec![
                    self.browser
                        .read(cx)
                        .selected()
                        .map_or_else(|| location.directory.clone(), PathBuf::from),
                ],
                vec![],
            ),
            ToolbarEvent::CompareRemote => (
                vec![],
                vec![
                    self.remote
                        .read(cx)
                        .selected()
                        .unwrap_or(self.remote.read(cx).directory())
                        .to_owned(),
                ],
            ),
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
