use super::*;
use gpui_kit::InputEvent as _;
use gpui_kit::base::test_support::snapshots;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, size};
use std::{fs, time::Duration};

struct View(Entity<BrowserPane>);
impl Render for View {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.0.update(cx, |pane, cx| pane.header(cx)))
            .child(self.0.clone())
    }
}
struct Fixture {
    handle: AnyWindowHandle,
    pane: Entity<BrowserPane>,
    root: tempfile::TempDir,
    _config: tempfile::TempDir,
    deep: PathBuf,
}
impl Fixture {
    async fn new(cx: &mut TestAppContext, registered: bool) -> Self {
        cx.executor().allow_parking();
        let root = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let deep = root
            .path()
            .join("日本語🦀-long-directory-".repeat(5))
            .join("leaf");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("file.txt"), "real filesystem").unwrap();
        let store = Store::new(config.path().into());
        if registered {
            let now = drift_core::project::now();
            store
                .save_project(
                    None,
                    drift_core::project::Project {
                        slug: "navigation".into(),
                        name: "Navigation".into(),
                        path: root.path().into(),
                        archived: false,
                        created_at: now,
                        updated_at: now,
                        opened_at: None,
                    },
                )
                .unwrap();
        }
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::actions::bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(480.), px(650.)),
                    })),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        let pane = cx.new(|cx| {
                            BrowserPane::new(
                                store,
                                BrowserService::new().unwrap(),
                                root.path().into(),
                                window,
                                cx,
                            )
                        });
                        pane.update(cx, |p, cx| p.open(deep.clone(), window, cx));
                        View(pane)
                    })
                },
            )
            .unwrap()
        });
        let pane = view.read_with(cx, |v, _| v.0.clone());
        let fixture = Self {
            handle,
            pane,
            root,
            _config: config,
            deep,
        };
        fixture.idle(cx).await;
        fixture
    }
    async fn idle(&self, cx: &mut TestAppContext) {
        cx.wait_for(self.handle, Duration::from_secs(60), |_, cx| {
            !self.pane.read(cx).is_loading()
        })
        .await;
    }
}
fn mouse(window: &mut Window, cx: &mut App, id: &'static str, direction: NavigationDirection) {
    let position = window.find(id).bounds().center();
    window.dispatch_event(
        gpui_kit::MouseDownEvent {
            button: MouseButton::Navigate(direction),
            position,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

#[gpui_kit::test]
async fn real_local_crumbs_boundaries_mouse_history_and_narrow_unicode(cx: &mut TestAppContext) {
    let f = Fixture::new(cx, true).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(
            f.pane.read(cx).breadcrumb_paths(),
            [
                f.root.path().to_path_buf(),
                f.deep.parent().unwrap().into(),
                f.deep.clone()
            ]
        );
        w.render_frame(cx);
        for width in [480., 240., 120.] {
            w.resize(size(px(width), px(650.)));
            w.bounds_changed(cx);
            w.render_frame(cx);
            let viewport = w.find("local-breadcrumbs").bounds();
            assert!(viewport.size.width > px(0.) && viewport.right() <= px(width));
            assert!(
                w.find(("local-crumb", 1usize)).bounds().size.width > viewport.size.width,
                "long segment must scroll, not truncate"
            );
            assert!(w.find("filter").bounds().right() <= px(width));
            w.scroll(
                "local-breadcrumbs",
                gpui_kit::ScrollDelta::Pixels(point(px(-4000.), px(0.))),
                cx,
            );
            w.render_frame(cx);
            assert!(
                w.find(("local-crumb-current", 2usize)).bounds().right()
                    <= viewport.right() + px(1.)
            );
            w.scroll(
                "local-breadcrumbs",
                gpui_kit::ScrollDelta::Pixels(point(px(4000.), px(0.))),
                cx,
            );
            w.render_frame(cx);
        }
        f.pane.update(cx, |p, cx| {
            let id = p.id();
            p.breadcrumb_navigation(p.navigation_stamp(), PathBuf::from("/"), w, cx);
            assert_eq!(p.id(), id);
        });
        let crumb = snapshots(w)
            .into_iter()
            .find(|s| s.path().last() == Some(&gpui_kit::ElementId::from(("local-crumb", 1usize))))
            .unwrap();
        assert_eq!(
            crumb.label(),
            Some(format!("Open {}", f.deep.parent().unwrap().display()).as_str())
        );
        assert!(snapshots(w).iter().all(|s| {
            !s.path()
                .contains(&gpui_kit::ElementId::from(("local-crumb", 2usize)))
        }));
        // The root stays reachable even when the next segment is wider than the viewport.
        w.click(("local-crumb", 0usize), cx);
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(f.pane.read(cx).location().unwrap().directory, f.root.path());
        assert!(!f.pane.read(cx).can_up());
        mouse(w, cx, "local-browser-header", NavigationDirection::Back);
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(f.pane.read(cx).location().unwrap().directory, f.deep);
        mouse(w, cx, "local-files-pane", NavigationDirection::Forward);
    })
    .unwrap();
    f.idle(cx).await;
    assert_eq!(
        f.pane
            .read_with(cx, |p, _| p.location().unwrap().directory.clone()),
        f.root.path()
    );
}

#[gpui_kit::test]
async fn local_noop_busy_stale_menu_and_hidden_frames_preserve_state(cx: &mut TestAppContext) {
    let f = Fixture::new(cx, false).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert_eq!(
            f.pane.read(cx).breadcrumb_paths().first().unwrap(),
            &PathBuf::from("/")
        );
        f.pane.update(cx, |p, cx| {
            p.filter.update(cx, |s, cx| s.set_value("file", w, cx));
            p.files.select(0);
            p.files.toggle_mark();
        });
    })
    .unwrap();
    let busy_stamp = cx
        .update_window(f.handle, |_, w, cx| {
            f.pane.update(cx, |p, cx| {
                let filter = p.filter.entity_id();
                let id = p.id();
                let marks = p.marked();
                let selected = p.selected().map(str::to_owned);
                let stamp = p.navigation_stamp();
                p.breadcrumb_navigation(stamp, p.location().unwrap().directory.clone(), w, cx);
                p.breadcrumb_navigation(stamp, PathBuf::from("/not/an/ancestor"), w, cx);
                p.mouse_navigation(stamp, NavigationDirection::Forward, w, cx);
                p.set_navigation_active(false, cx);
                p.breadcrumb_navigation(p.navigation_stamp(), f.root.path().into(), w, cx);
                p.breadcrumb_navigation(stamp, f.root.path().into(), w, cx);
                p.set_navigation_active(true, cx);
                p.breadcrumb_navigation(stamp, f.root.path().into(), w, cx);
                let stamp = p.navigation_stamp();
                p.open_context_menu(None, point(px(10.), px(200.)), w, cx);
                p.breadcrumb_navigation(stamp, f.root.path().into(), w, cx);
                p.dismiss_context_menu(w, cx);
                p.breadcrumb_navigation(stamp, f.root.path().into(), w, cx);
                assert_eq!(p.id(), id);
                assert_eq!(p.marked(), marks);
                assert_eq!(p.selected(), selected.as_deref());
                assert_eq!(p.filter.entity_id(), filter);
                assert_eq!(p.filter.read(cx).value().as_str(), "file");
                let old_stamp = p.navigation_stamp();
                p.reload(w, cx);
                let busy = p.id();
                p.breadcrumb_navigation(p.navigation_stamp(), f.root.path().into(), w, cx);
                p.mouse_navigation(old_stamp, NavigationDirection::Back, w, cx);
                assert_eq!(p.id(), busy);
                p.navigation_stamp()
            })
        })
        .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        f.pane.update(cx, |p, cx| {
            let id = p.id();
            p.breadcrumb_navigation(busy_stamp, f.root.path().into(), w, cx);
            assert_eq!(
                p.id(),
                id,
                "a disabled loading frame must stay inert after completion"
            );
            let stamp = p.navigation_stamp();
            p.breadcrumb_navigation(stamp, f.root.path().into(), w, cx);
        })
    })
    .unwrap();
    f.idle(cx).await;
    assert_eq!(
        f.pane
            .read_with(cx, |p, _| p.location().unwrap().directory.clone()),
        f.root.path()
    );
}

#[gpui_kit::test]
async fn mouse_back_from_finder_restores_native_filter_marks_and_history(cx: &mut TestAppContext) {
    let f = Fixture::new(cx, true).await;
    cx.update_window(f.handle, |_, w, cx| {
        f.pane.update(cx, |p, cx| {
            p.filter.update(cx, |s, cx| s.set_value("file", w, cx));
            p.files.select(0);
            p.files.toggle_mark();
        })
    })
    .unwrap();
    let (filter, marks, selected) = f.pane.read_with(cx, |p, _| {
        (
            p.filter.entity_id(),
            p.marked(),
            p.selected().map(str::to_owned),
        )
    });
    cx.update_window(f.handle, |_, w, cx| {
        f.pane
            .update(cx, |p, cx| p.command(BrowserCommand::Find, w, cx))
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        assert!(f.pane.read(cx).finder_active());
        w.render_frame(cx);
        mouse(w, cx, "local-browser-header", NavigationDirection::Back);
        let p = f.pane.read(cx);
        assert!(!p.finder_active());
        assert_eq!(p.filter.entity_id(), filter);
        assert_eq!(p.filter.read(cx).value().as_str(), "file");
        assert_eq!(p.marked(), marks);
        assert_eq!(p.selected(), selected.as_deref());
        assert_eq!(p.location().unwrap().directory, f.deep);
        assert!(!p.can_back());
    })
    .unwrap();
}

#[cfg(unix)]
#[gpui_kit::test]
async fn real_non_utf8_ancestor_button_keeps_original_target(cx: &mut TestAppContext) {
    use std::os::unix::ffi::OsStrExt;
    let f = Fixture::new(cx, true).await;
    let ancestor = f.root.path().join(std::ffi::OsStr::from_bytes(b"bad-\xff"));
    let leaf = ancestor.join("leaf");
    fs::create_dir_all(&leaf).unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        f.pane.update(cx, |p, cx| {
            // The existing Git classifier accepts only UTF-8 paths. This test isolates
            // the native crumb's exact filesystem target from that separate policy.
            p.show_ignored = true;
            p.open(leaf.clone(), w, cx);
        })
    })
    .unwrap();
    f.idle(cx).await;
    cx.update_window(f.handle, |_, w, cx| w.click(("local-crumb", 1usize), cx))
        .unwrap();
    f.idle(cx).await;
    assert_eq!(
        f.pane
            .read_with(cx, |p, _| p.location().unwrap().directory.clone()),
        ancestor
    );
}

#[cfg(unix)]
#[test]
fn local_crumb_targets_keep_non_utf8_and_symlink_components() {
    use std::os::unix::ffi::OsStrExt;
    let path = std::path::Path::new(std::ffi::OsStr::from_bytes(b"/root/link/../bad-\xff/leaf"));
    let ancestors: Vec<_> = path.ancestors().collect();
    assert_eq!(
        ancestors[1].as_os_str().as_bytes(),
        b"/root/link/../bad-\xff"
    );
    assert_eq!(ancestors[2].as_os_str().as_bytes(), b"/root/link/..");
}
