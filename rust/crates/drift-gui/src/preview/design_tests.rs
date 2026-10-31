use super::*;
use crate::design::{metric, size as token};
use drift_core::{config::RuntimeConfig, local::ProjectRoot};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::{fs, sync::Arc, time::Duration};

struct PreviewWindow(Entity<PreviewPane>);
impl Render for PreviewWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().child(self.0.clone())
    }
}

#[gpui_kit::test]
async fn preview_chrome_wraps_preserving_native_editor_path_and_copy(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = format!("long-{}-ä-Ω.txt", "filename".repeat(20));
    let text = "Unicode 👩🏽‍💻\n  whitespace\n\n";
    fs::write(dir.path().join(&path), text).unwrap();
    let location = Location {
        root: Arc::new(ProjectRoot::open(dir.path()).unwrap()),
        directory: dir.path().into(),
        slug: None,
        config: RuntimeConfig::resolve(&Default::default(), None).unwrap(),
    };
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(900.), px(600.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    PreviewWindow(
                        cx.new(|cx| PreviewPane::new(BrowserService::new().unwrap(), w, cx)),
                    )
                })
            },
        )
        .unwrap()
    });
    let preview = view.read_with(cx, |view, _| view.0.clone());
    cx.update_window(handle, |_, w, cx| {
        preview.update(cx, |pane, cx| {
            pane.show(location, path.clone().into(), 1, w, cx)
        });
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !preview.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        let editor = preview.read(cx).editor.clone();
        let id = preview.read(cx).id;
        editor.focus_handle(cx).focus(w, cx);
        let focus = w.focused(cx);
        for (width, height, font, mode) in [
            (900., 600., 16., ThemeMode::Dark),
            (320., 500., 16., ThemeMode::Light),
            (420., 800., 24., ThemeMode::Dark),
        ] {
            w.resize(size(px(width), px(height)));
            w.bounds_changed(cx);
            Theme::change(mode, Some(w), cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            w.render_frame(cx);
            for button in ["copy-selection", "copy-preview"] {
                let bounds = w.find(button).bounds();
                assert!(bounds.left() >= px(0.) && bounds.right() <= px(width));
                assert_eq!(bounds.size.height, metric(token::CONTROL, cx));
                assert!(bounds.bottom() <= w.find("preview-header").bounds().bottom());
            }
            let title = w.find("preview-location");
            assert_eq!(title.label(), Some(path.as_str()));
            assert!(title.bounds().right() <= px(width));
            assert_eq!(
                title.bounds().top(),
                w.within("preview").find("preview-header").bounds().bottom()
            );
            assert_eq!(title.bounds().size.height, metric(token::HEADER, cx));
            let editors = gpui_kit::base::test_support::snapshots(w)
                .into_iter()
                .filter(|s| s.role() == Some(gpui_kit::Role::MultilineTextInput))
                .collect::<Vec<_>>();
            assert_eq!(editors.len(), 1);
            assert!(
                editors[0]
                    .path()
                    .contains(&gpui_kit::ElementId::from("preview-pane"))
            );
            assert_eq!(title.bounds().bottom(), editors[0].bounds().top());
            assert!(w.within("preview").find("preview-location").visible());
            assert_eq!(editors[0].bounds().bottom(), px(height));
            assert_eq!(preview.read(cx).editor, editor);
            assert_eq!(preview.read(cx).id, id);
            assert_eq!(preview.read(cx).text, text);
            assert!(preview.read(cx).ready);
            assert_eq!(w.focused(cx), focus);
        }
        w.click("copy-selection", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(path.as_str())
        );
        w.click("copy-preview", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(text)
        );
    })
    .unwrap();
}
