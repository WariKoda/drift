use crate::shell::Shell;
use crate::{actions::bind_keys, test_support::settle};
use drift_app::{browser::BrowserService, remote::RemoteService};
use drift_core::store::Store;
use gpui_kit::component::Theme;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AppContext, Bounds, ClipboardItem, TestAppContext, WindowBounds, WindowOptions, point, px, size,
};
use std::time::Duration;

#[gpui_kit::test]
async fn one_rejected_filter_paste_keeps_following_rows_aligned_and_recovers(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file.txt"), "contents").unwrap();
    let service = BrowserService::new().unwrap();
    let remote = RemoteService::new(service.clone());
    let (handle, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(960.), px(1000.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    Shell::new(
                        w,
                        cx,
                        Store::new(config.path().into()),
                        service,
                        remote,
                        root.path().to_path_buf(),
                    )
                })
            },
        )
        .unwrap()
    });
    cx.wait_for(handle, Duration::from_secs(30), |_, cx| {
        shell.read(cx).browser.read(cx).location().is_some()
    })
    .await;
    for font in [16., 24.] {
        for width in [640., 1440.] {
            cx.update_window(handle, |_, w, cx| {
                Theme::update(cx, |theme| theme.font_size = px(font));
                w.resize(size(px(width), px(1000.)));
                w.bounds_changed(cx);
                settle(w, cx);
                let initial = w.find("local-aligned-filter").bounds().size.height;
                w.click("filter", cx);
                cx.write_to_clipboard(ClipboardItem::new_string("rejected-private\nvalue".into()));
                w.press("ctrl-v", cx);
                settle(w, cx);
                let local = w.find("local-aligned-filter").bounds();
                let remote = w.find("remote-aligned-filter").bounds();
                assert!(
                    local.size.height > initial,
                    "one native rejection must actually expand its row"
                );
                assert_eq!(local.top(), remote.top());
                assert_eq!(local.bottom(), remote.bottom());
                assert_eq!(w.find("local-path-header").bounds().top(), local.bottom());
                assert_eq!(w.find("remote-path-header").bounds().top(), remote.bottom());
                assert_eq!(
                    w.find("local-path-header").bounds().top(),
                    w.find("remote-path-header").bounds().top()
                );
                assert_eq!(
                    w.find("local-files-pane").bounds().top(),
                    w.find("remote-files-pane").bounds().top()
                );
                w.input("x", cx);
                settle(w, cx);
                assert_eq!(w.find("local-aligned-filter").bounds().size.height, initial);
                assert_eq!(
                    w.find("local-path-header").bounds().top(),
                    w.find("remote-path-header").bounds().top()
                );
                w.press("ctrl-a", cx);
                w.press("backspace", cx);
            })
            .unwrap();
        }
    }
}
