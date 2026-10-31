use super::*;
use crate::design::{Palette, Text, metric};
use gpui_kit::ElementId;
use gpui_kit::component::{Theme, ThemeMode};

#[gpui_kit::test]
async fn measured_narrow_file_panes_keep_counts_and_all_action_text_visible_and_anchor_scroll(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    for name in
        std::iter::once("a-changed.txt".to_owned()).chain((0..60).map(|i| format!("z-{i:03}.txt")))
    {
        fs::write(local.path().join(&name), "local contents").unwrap();
        fs::write(
            server.dir.path().join("files").join(name),
            "remote contents",
        )
        .unwrap();
    }
    fs::write(local.path().join("b-local.txt"), "local only").unwrap();
    fs::write(server.dir.path().join("files/c-remote.txt"), "remote only").unwrap();
    let service = BrowserService::new().unwrap();
    let remote = RemoteService::with_options(service.clone(), server.options());
    let host = server.host();
    let connection = remote
        .connect(
            host.clone(),
            OperationId {
                project: 1,
                operation: 1,
            },
        )
        .task
        .await
        .unwrap()
        .unwrap()
        .session;
    let request = LoadRequest {
        location: Location {
            root: Arc::new(ProjectRoot::open(local.path()).unwrap()),
            directory: local.path().into(),
            slug: None,
            config: RuntimeConfig {
                hosts: vec![host.clone()],
                mappings: vec![],
                ui: Default::default(),
            },
        },
        host,
        connection: connection.clone(),
        local: vec![],
        remote: vec![],
        scope: ScopeOptions::default(),
    };
    let (handle, pane) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1200.), px(2000.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    let mut pane = ComparisonPane::new(service, w, cx);
                    pane.open(request, 1, w, cx);
                    pane
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        !pane.read(cx).is_loading()
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        pane.update(cx, |pane, cx| {
            assert_eq!(pane.files.len(), 63);
            pane.decisions[pane.files[0]] = Decision::DeleteRemote;
            pane.decisions[pane.files[1]] = Decision::DeleteLocal;
            pane.decisions[pane.files[2]] = Decision::Download;
            pane.choose(Some(pane.files[0]), w, cx);
        });
        let decisions = pane.read(cx).decisions.clone();
        let selected = pane.read(cx).selected;
        let scroll = pane.read(cx).scroll.clone();
        let filter = pane.read(cx).filter.clone();
        let diff = pane.read(cx).diff.clone();
        let id = pane.read(cx).id;
        let focus = w.focused(cx);
        for (width, font, mode) in [
            (1200., 16., ThemeMode::Dark),
            (420., 16., ThemeMode::Light),
            (320., 24., ThemeMode::Dark),
            (420., 32., ThemeMode::Light),
            (1200., 16., ThemeMode::Dark),
        ] {
            w.resize(size(px(width), px(2000.)));
            w.bounds_changed(cx);
            Theme::change(mode, Some(w), cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            w.render_frame(cx);
            let files = w.find("comparison-files").bounds();
            let header = w.find("comparison-files-header").bounds();
            let viewport = w.find("comparison-file-list").bounds();
            assert!(viewport.size.height > px(0.));
            let compact = files.size.width < metric(240., cx);
            let mut labels: Vec<(ElementId, String, Bounds<gpui_kit::Pixels>)> = vec![
                ("comparison-shown-count".into(), "63 shown".into(), header),
                ("comparison-total-count".into(), "63 total".into(), header),
            ];
            for (row, status, action) in [
                (0usize, "Changed", "Delete remote"),
                (1usize, "Local only", "Delete local"),
                (2usize, "Remote only", "Download"),
            ] {
                let bounds = w.find(("comparison-file", row)).bounds();
                assert!(
                    bounds.bottom() <= viewport.bottom(),
                    "{bounds:?}, {viewport:?}"
                );
                let quad = w
                    .painted_quads()
                    .into_iter()
                    .find(|quad| {
                        quad.bounds == bounds.scale(w.scale_factor())
                            && quad.background
                                == (if row == 0 {
                                    Palette::current(cx).selected
                                } else {
                                    Palette::current(cx).canvas
                                })
                                .into()
                    })
                    .expect("the actual row must be painted, not merely observed");
                let mask = quad.content_mask.bounds;
                let status_bounds = w.find(("comparison-file-status", row)).bounds();
                let action_bounds = w.find(("comparison-file-action", row)).bounds();
                for text_bounds in [status_bounds, action_bounds] {
                    let text_bounds = text_bounds.scale(w.scale_factor());
                    assert!(
                        mask.left() <= text_bounds.left() && mask.right() >= text_bounds.right()
                    );
                    assert!(
                        mask.top() <= text_bounds.top() && mask.bottom() >= text_bounds.bottom()
                    );
                }
                if compact {
                    assert!(status_bounds.bottom() <= action_bounds.top());
                    assert!(bounds.size.height >= metric(80., cx));
                } else {
                    assert_eq!(status_bounds.top(), action_bounds.top());
                    assert_eq!(bounds.size.height, metric(48., cx));
                }
                labels.push((
                    ("comparison-file-status", row).into(),
                    status.into(),
                    bounds,
                ));
                labels.push((
                    ("comparison-file-action", row).into(),
                    if compact {
                        action.into()
                    } else {
                        format!(" · {action}")
                    },
                    bounds,
                ));
            }
            // Validate actual native text metrics, not only full accessibility strings.
            for (id, text, owner) in labels {
                let item = w.find(id);
                if text.ends_with("shown") || text.ends_with("total") {
                    assert_eq!(item.label(), Some(text.as_str()));
                }
                let bounds = item.bounds();
                assert!(bounds.left() >= owner.left() && bounds.right() <= owner.right());
                assert!(bounds.top() >= owner.top() && bounds.bottom() <= owner.bottom());
                let mut style = w.text_style();
                style.font_family = cx.theme().font_family.clone();
                let font_size = Text::Metadata.rems().to_pixels(w.rem_size());
                let shaped = w
                    .text_system()
                    .shape_text(
                        text.clone().into(),
                        font_size,
                        &[style.to_run(text.len())],
                        Some(bounds.size.width),
                        None,
                    )
                    .unwrap();
                let height: gpui_kit::Pixels = shaped
                    .iter()
                    .map(|line| line.size(metric(16., cx)).height)
                    .sum();
                assert!(
                    height <= bounds.size.height + px(0.1),
                    "{text:?}: needs {height:?}, has {bounds:?}"
                );
                for line in shaped {
                    let ends = line
                        .wrap_boundaries()
                        .iter()
                        .map(|boundary| {
                            line.runs()[boundary.run_ix].glyphs[boundary.glyph_ix]
                                .position
                                .x
                        })
                        .chain(std::iter::once(line.unwrapped_layout.width));
                    let mut start = px(0.);
                    for end in ends {
                        assert!(
                            end - start <= bounds.size.width + px(0.1),
                            "{text:?}: wrapped advance {:?}, {bounds:?}",
                            end - start
                        );
                        start = end;
                    }
                }
            }
            let navigation = w.find("comparison-toolbar").bounds();
            let plan = w.find("comparison-plan-toolbar").bounds();
            assert!(navigation.bottom() <= plan.top());
            for id in ["comparison-action", "sync-selected", "sync-all"] {
                let bounds = w.find(id).bounds();
                assert!(bounds.top() >= plan.top() && bounds.bottom() <= plan.bottom());
            }
            assert_eq!(pane.read(cx).decisions, decisions);
            assert_eq!(pane.read(cx).selected, selected);
            assert_eq!(pane.read(cx).id, id);
            assert_eq!(pane.read(cx).filter, filter);
            assert_eq!(pane.read(cx).diff, diff);
            assert!(std::rc::Rc::ptr_eq(&pane.read(cx).scroll.0, &scroll.0));
            assert_eq!(w.focused(cx), focus);
            assert!(pane.read(cx).confirm_sync.is_none());
            assert!(!pane.read(cx).is_loading());
        }
        // A wide window can still contain a narrow pane after a deliberate resize.
        for _ in 0..15 {
            w.press("ctrl-alt-left", cx);
        }
        w.render_frame(cx);
        assert_eq!(w.find("comparison-files").bounds().size.width, px(120.));
        assert!(w.find(("comparison-file", 0usize)).bounds().size.height >= metric(80., cx));
        pane.read(cx)
            .scroll
            .scroll_to_item_strict(20, ScrollStrategy::Top);
        w.render_frame(cx);
        assert_eq!(
            w.find(("comparison-file", 20usize)).bounds().top(),
            w.find("comparison-file-list").bounds().top()
        );
        for (width, font) in [(420., 24.), (1200., 16.), (420., 32.)] {
            pane.read(cx)
                .scroll
                .scroll_to_item(21, ScrollStrategy::Nearest);
            w.resize(size(px(width), px(2000.)));
            w.bounds_changed(cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            w.render_frame(cx);
            assert_eq!(
                w.find(("comparison-file", 20usize)).bounds().top(),
                w.find("comparison-file-list").bounds().top()
            );
            assert_eq!(pane.read(cx).selected, selected);
            assert_eq!(pane.read(cx).decisions, decisions);
        }
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("ctrl-f", cx);
        assert!(pane.read(cx).filter.focus_handle(cx).is_focused(w));
        w.input("b-local", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
        pane.read(cx).files.len() == 1
    })
    .await;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(w.find("comparison-shown-count").label(), Some("1 shown"));
        assert_eq!(w.find("comparison-total-count").label(), Some("63 total"));
        assert!(
            w.find(("comparison-file", 0usize))
                .label()
                .unwrap()
                .contains("b-local.txt")
        );
        assert!(pane.read(cx).confirm_sync.is_none());
        assert!(!pane.read(cx).is_loading());
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(local.path().join("a-changed.txt")).unwrap(),
        "local contents"
    );
    assert_eq!(
        fs::read_to_string(server.dir.path().join("files/a-changed.txt")).unwrap(),
        "remote contents"
    );
    assert_eq!(
        fs::read_to_string(local.path().join("b-local.txt")).unwrap(),
        "local only"
    );
    assert_eq!(
        fs::read_to_string(server.dir.path().join("files/c-remote.txt")).unwrap(),
        "remote only"
    );
    connection.shutdown().await;
}
