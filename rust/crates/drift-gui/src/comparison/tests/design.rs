use super::*;
use crate::design::{metric, size as token};
use gpui_kit::component::{Theme, ThemeMode};

#[gpui_kit::test]
async fn comparison_chrome_scales_preserving_paths_decisions_and_pending_consent(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let long_name = format!("long-{}-ä-Ω.txt", "filename".repeat(20));
    for name in [&long_name, "other.txt"] {
        fs::write(local.path().join(name), "local contents").unwrap();
        fs::write(
            server.dir.path().join("files").join(name),
            "remote contents",
        )
        .unwrap();
    }
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
                    size: size(px(1200.), px(900.)),
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
        w.render_frame(cx);
        assert_eq!(pane.read(cx).files.len(), 2);
        assert!(
            w.find(("comparison-file", 0usize))
                .label()
                .unwrap()
                .contains(&long_name)
        );
        let filter = pane.read(cx).filter.clone();
        let diff = pane.read(cx).diff.clone();
        let id = pane.read(cx).id;
        let decisions = pane.read(cx).decisions.clone();
        w.click("sync-all", cx);
        let consent = pane.read(cx).confirm_sync.clone();
        assert!(consent.is_some());
        pane.read(cx).focus_handle(cx).focus(w, cx);
        let focus = w.focused(cx);
        for (width, font, mode) in [
            (1200., 16., ThemeMode::Dark),
            (320., 16., ThemeMode::Light),
            (420., 24., ThemeMode::Dark),
        ] {
            w.resize(size(px(width), px(900.)));
            w.bounds_changed(cx);
            Theme::change(mode, Some(w), cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            w.render_frame(cx);
            for id in [
                "comparison-back",
                "comparison-refresh",
                "comparison-ignored",
                "comparison-progress",
                "comparison-action",
                "sync-selected",
                "sync-all",
            ] {
                let item = w.find(id);
                let bounds = item.bounds();
                assert!(item.visible());
                assert!(
                    bounds.left() >= px(0.) && bounds.right() <= px(width),
                    "{id}: {bounds:?}"
                );
                assert_eq!(bounds.size.height, metric(token::CONTROL, cx));
            }
            for id in ["sync-confirm", "sync-dismiss"] {
                let bounds = w.find(id).bounds();
                assert!(
                    bounds.left() >= px(0.) && bounds.right() <= px(width),
                    "{id}: {bounds:?}"
                );
                assert!(bounds.top() >= px(0.) && bounds.bottom() <= px(900.));
            }
            let row = w.find(("comparison-file", 0usize)).bounds();
            let files = w.find("comparison-files").bounds();
            if files.size.width < metric(240., cx) {
                assert!(row.size.height >= metric(80., cx));
            } else {
                assert_eq!(row.size.height, metric(token::ROW * 2., cx));
            }
            assert!(
                w.find(("comparison-file", 0usize))
                    .label()
                    .unwrap()
                    .contains(&long_name)
            );
            assert_eq!(pane.read(cx).id, id);
            assert_eq!(pane.read(cx).filter, filter);
            assert_eq!(pane.read(cx).diff, diff);
            assert_eq!(pane.read(cx).decisions, decisions);
            assert_eq!(pane.read(cx).confirm_sync, consent);
            assert!(!pane.read(cx).is_loading());
            assert_eq!(w.focused(cx), focus);
        }
        w.click("sync-dismiss", cx);
        assert!(pane.read(cx).confirm_sync.is_none());
        assert!(!pane.read(cx).is_loading());
    })
    .unwrap();
    for name in [&long_name, "other.txt"] {
        assert_eq!(
            fs::read_to_string(local.path().join(name)).unwrap(),
            "local contents"
        );
        assert_eq!(
            fs::read_to_string(server.dir.path().join("files").join(name)).unwrap(),
            "remote contents"
        );
    }
    connection.shutdown().await;
}
