use super::*;
pub(super) struct ProjectForm {
    pub expected: Option<Project>,
    pub name: Entity<InputState>,
    pub path: Entity<InputState>,
}
impl ProjectsPanel {
    #[cfg(test)]
    pub(crate) fn set_form_path(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.form
            .as_ref()
            .unwrap()
            .path
            .update(cx, |input, cx| input.set_value(path, window, cx));
    }
    pub(super) fn edit(
        &mut self,
        expected: Option<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = &expected {
            if let Err(error) = form_input::validate_values([
                project.name.as_str(),
                &project.path.to_string_lossy(),
            ]) {
                self.status = error.into();
                cx.notify();
                return;
            }
            self.cursor = Some(project.slug.clone());
        }
        let name = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Project name");
            input.set_value(
                expected
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_default(),
                window,
                cx,
            );
            input
        });
        let path = cx.new(|cx| {
            let mut input =
                InputState::new(window, cx).placeholder("Local path (absolute, relative or ~/…)");
            input.set_value(
                expected
                    .as_ref()
                    .map(|p| p.path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                window,
                cx,
            );
            input
        });
        name.focus_handle(cx).focus(window, cx);
        self.form = Some(ProjectForm {
            expected,
            name,
            path,
        });
        self.status.clear();
        cx.notify();
    }
    pub(super) fn save_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &self.form else {
            return;
        };
        if let Err(error) = form_input::validate_inputs([&form.name, &form.path], window, cx) {
            self.status = error.into();
            cx.notify();
            return;
        }
        let name = form.name.read(cx).value().to_string();
        let path = form.path.read(cx).value().to_string();
        let command = match &form.expected {
            Some(expected) => ProjectCommand::Edit {
                expected: Box::new(expected.clone()),
                name,
                path,
            },
            None => ProjectCommand::Create { name, path },
        };
        self.request(command, window, cx);
    }
}
