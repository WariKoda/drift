use super::*;
use crate::{
    actions::bind_keys,
    design::{metric, size as token},
    sftp_test_support as support,
};
use gpui_kit::base::test_support::snapshots;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
};
use std::{fs, path::Path, time::Duration};

mod fixed_panes;
mod header_history;
mod pane_cache;
mod pane_path;
mod persistent;
mod relocated_controls;
mod remote_hints;
mod remote_preview_cancel;
mod selection_comparison;
mod selection_status;
mod toolbar_keyboard;
pub(super) mod toolbar_scopes;
mod workspace_filters;
mod workspace_headers;

pub(super) const CONTROLS: [(&str, &str); 5] = [
    ("home", "Home"),
    ("projects", "Projects"),
    ("hosts", "Hosts"),
    ("open-folder", "Open folder"),
    ("settings", "Settings"),
];

pub(super) const LOCAL_CONTROLS: [(&str, &str); 8] = [
    ("back", "Back"),
    ("forward", "Forward"),
    ("up", "Up"),
    ("refresh", "Refresh"),
    ("find", "Find files"),
    ("hidden", "Show hidden"),
    ("ignored", "Show ignored"),
    ("cancel", "Cancel"),
];

pub(super) const COMPARE_CONTROLS: [(&str, &str); 4] = [
    ("compare-project", "Compare project"),
    ("compare-local", "Compare local selection"),
    ("compare-remote", "Compare remote selection"),
    ("compare-marked", "Compare marked paths"),
];

#[derive(Clone, Copy, PartialEq)]
enum ToolbarScope {
    Global,
    Local,
    Compare,
}
impl ToolbarScope {
    const ALL: [Self; 3] = [Self::Global, Self::Local, Self::Compare];
    fn id(self) -> &'static str {
        match self {
            Self::Global => "workspace-toolbar",
            Self::Local => "local-navigation-toolbar",
            Self::Compare => "workspace-compare",
        }
    }
    fn controls(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Global => &CONTROLS,
            Self::Local => &LOCAL_CONTROLS,
            Self::Compare => &COMPARE_CONTROLS,
        }
    }
    /// Controls that show a text label next to their icon.
    fn labelled(self, id: &str) -> bool {
        match self {
            Self::Global => ["projects", "hosts", "open-folder"].contains(&id),
            Self::Local => false,
            Self::Compare => true,
        }
    }
}

// Render fresh production native components for each scope and state, without I/O.
struct RecordedToolbar {
    enabled: bool,
    scope: ToolbarScope,
    covered: bool,
    events: Vec<&'static str>,
}
impl Render for RecordedToolbar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = self.enabled;
        let state = ToolbarState {
            management_enabled: !self.covered,
            browser_enabled: !self.covered,
            projects_open: false,
            hosts_open: false,
            settings_open: false,
            can_compare: enabled,
            has_location: enabled,
            listing: false,
            cancellable: enabled,
            remote_cancellable: enabled,
            folder_prompt: !enabled,
            can_back: enabled,
            can_forward: enabled,
            can_up: enabled,
            show_hidden: false,
            show_ignored: false,
        };
        let listener = cx.listener(|this, event, _, _| {
            this.events.push(match event {
                ToolbarEvent::Home => "home",
                ToolbarEvent::Settings => "settings",
                ToolbarEvent::Projects => "projects",
                ToolbarEvent::Hosts => "hosts",
                ToolbarEvent::OpenFolder => "open-folder",
                ToolbarEvent::Cancel => "cancel",
                ToolbarEvent::CancelRemote => "remote-cancel",
                ToolbarEvent::CompareProject => "compare-project",
                ToolbarEvent::CompareLocal => "compare-local",
                ToolbarEvent::CompareRemote => "compare-remote",
                ToolbarEvent::CompareMarked => "compare-marked",
                ToolbarEvent::Browser(command) => match command {
                    BrowserCommand::Back => "back",
                    BrowserCommand::Forward => "forward",
                    BrowserCommand::Up => "up",
                    BrowserCommand::Refresh => "refresh",
                    BrowserCommand::Find => "find",
                    BrowserCommand::Hidden => "hidden",
                    BrowserCommand::Ignored => "ignored",
                    _ => panic!("unexpected toolbar navigation"),
                },
            });
        });
        div().size_full().child(match self.scope {
            ToolbarScope::Global => Toolbar::new(state, listener),
            ToolbarScope::Local => Toolbar::local(state, listener),
            ToolbarScope::Compare => Toolbar::compare(state, listener),
        })
    }
}

fn check_native_toolbar(
    cx: &mut TestAppContext,
    scope: ToolbarScope,
    enabled: bool,
    covered: bool,
) {
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(420.), px(900.)),
                })),
                ..Default::default()
            },
            cx,
            |_, cx| {
                cx.new(|_| RecordedToolbar {
                    enabled,
                    scope,
                    covered,
                    events: vec![],
                })
            },
        )
        .unwrap()
    });
    cx.update_window(handle, |_, w, cx| {
        let controls = scope.controls();
        let scope_id = scope.id();
        let active: Vec<_> = controls
            .iter()
            .copied()
            .filter(|(id, _)| {
                !covered
                    && (enabled
                        || (scope != ToolbarScope::Compare
                            && [
                                "home", "projects", "refresh", "hidden", "ignored", "settings",
                            ]
                            .contains(id)))
            })
            .collect();
        for (width, font) in [(420., 16.), (420., 24.), (1200., 24.), (180., 24.)] {
            w.resize(size(px(width), px(900.)));
            w.bounds_changed(cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            w.render_frame(cx);
            assert_eq!(
                snapshots(w)
                    .into_iter()
                    .filter(|item| item.role() == Some(gpui_kit::Role::Button))
                    .count(),
                controls.len()
            );
            for other in ToolbarScope::ALL {
                if other != scope {
                    for &(id, _) in other.controls() {
                        assert!(w.try_find(id).is_none(), "unexpected {id} in {scope_id}");
                    }
                }
            }
            let toolbar = w.find(scope_id).bounds();
            let mut origins = vec![];
            for &(id, label) in controls {
                let item = w.within(scope_id).find(id);
                let bounds = item.bounds();
                assert_eq!(item.role(), Some(gpui_kit::Role::Button));
                assert_eq!(item.label(), Some(label));
                if scope != ToolbarScope::Local {
                    assert!(item.visible(), "{id}");
                    assert!(
                        bounds.left() >= toolbar.left() && bounds.right() <= toolbar.right(),
                        "{id}: {bounds:?}"
                    );
                }
                assert!(
                    bounds.top() >= toolbar.top() && bounds.bottom() <= toolbar.bottom(),
                    "{id}: {bounds:?}"
                );
                assert!(
                    (scope == ToolbarScope::Local || bounds.right() <= px(width))
                        && bounds.bottom() <= px(900.),
                    "{id}: {bounds:?}"
                );
                assert_eq!(bounds.size.height, metric(token::CONTROL, cx), "{id}");
                if scope.labelled(id) {
                    assert!(
                        bounds.size.width > metric(token::CONTROL, cx),
                        "labelled {id}: {bounds:?}"
                    );
                } else {
                    assert_eq!(
                        bounds.size.width,
                        metric(token::CONTROL, cx),
                        "icon-only {id}"
                    );
                }
                origins.push(bounds.top());
                // Offscreen scroll content is mounted, but its center is not a pointer target.
                if item.visible()
                    && bounds.left() >= toolbar.left()
                    && bounds.right() <= toolbar.right()
                {
                    let before = view.read(cx).events.len();
                    w.click(id, cx);
                    let enabled = active.iter().any(|(active_id, _)| *active_id == id);
                    assert_eq!(
                        view.read(cx).events.len(),
                        before + usize::from(enabled),
                        "{id}"
                    );
                    if enabled {
                        assert_eq!(view.read(cx).events.last(), Some(&id));
                    }
                }
            }
            if scope == ToolbarScope::Local {
                assert!(
                    origins.iter().all(|y| *y == origins[0]),
                    "navigation must not wrap"
                );
                assert_eq!(toolbar.size.height, metric(token::HEADER, cx));
            }
            // Disabled native controls must not join traversal or deliver keyboard events.
            if active.is_empty() {
                let before = view.read(cx).events.len();
                for _ in 0..controls.len() + 1 {
                    w.focus_next(cx);
                    w.press("enter", cx);
                    w.press("space", cx);
                }
                assert_eq!(view.read(cx).events.len(), before);
            } else {
                w.focus_next(cx);
                w.render_frame(cx);
                let focused = snapshots(w)
                    .into_iter()
                    .find(|item| item.focused() == Some(true))
                    .unwrap();
                let start = active
                    .iter()
                    .position(|(_, label)| Some(*label) == focused.label())
                    .unwrap();
                for offset in 1..=active.len() {
                    w.focus_next(cx);
                    w.render_frame(cx);
                    let (id, label) = active[(start + offset) % active.len()];
                    assert_eq!(w.find(id).focused(), Some(true), "expected {label}");
                    for key in ["enter", "space"] {
                        let before = view.read(cx).events.len();
                        w.press(key, cx);
                        assert_eq!(view.read(cx).events.len(), before + 1, "{id}: {key}");
                        assert_eq!(view.read(cx).events.last(), Some(&id));
                    }
                }
            }
        }
        let focus = w.focused(cx);
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            Theme::change(mode, Some(w), cx);
            w.render_frame(cx);
            assert_eq!(w.focused(cx), focus);
            for &(id, label) in controls {
                assert_eq!(w.within(scope_id).find(id).label(), Some(label));
            }
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn native_global_toolbar_names_bounds_keyboard_order_and_disabled_inertness(
    cx: &mut TestAppContext,
) {
    check_native_toolbar(cx, ToolbarScope::Global, true, false);
    check_native_toolbar(cx, ToolbarScope::Global, false, false);
    check_native_toolbar(cx, ToolbarScope::Global, true, true);
}

#[gpui_kit::test]
fn native_local_toolbar_names_bounds_keyboard_order_and_disabled_inertness(
    cx: &mut TestAppContext,
) {
    check_native_toolbar(cx, ToolbarScope::Local, true, false);
    check_native_toolbar(cx, ToolbarScope::Local, false, false);
    check_native_toolbar(cx, ToolbarScope::Local, true, true);
}

#[gpui_kit::test]
fn native_compare_toolbar_names_bounds_keyboard_order_and_disabled_inertness(
    cx: &mut TestAppContext,
) {
    check_native_toolbar(cx, ToolbarScope::Compare, true, false);
    check_native_toolbar(cx, ToolbarScope::Compare, false, false);
    check_native_toolbar(cx, ToolbarScope::Compare, true, true);
}

struct Fixture {
    handle: AnyWindowHandle,
    shell: Entity<Shell>,
}
impl Fixture {
    async fn new(
        width: f32,
        height: f32,
        store: Store,
        root: &Path,
        options: Option<drift_core::remote::ConnectOptions>,
        cx: &mut TestAppContext,
    ) -> Self {
        cx.executor().allow_parking();
        let (handle, shell) = cx.update(|cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(width), px(height)),
                    })),
                    ..Default::default()
                },
                cx,
                |w, cx| {
                    cx.new(|cx| {
                        let service = BrowserService::new().unwrap();
                        let remote = if let Some(options) = options {
                            RemoteService::with_options(service.clone(), options)
                        } else {
                            RemoteService::new(service.clone())
                        };
                        Shell::new(w, cx, store, service, remote, root.to_path_buf())
                    })
                },
            )
            .unwrap()
        });
        cx.wait_for(handle, Duration::from_secs(20), |_, cx| {
            shell.read(cx).browser.read(cx).location().is_some()
                && !shell.read(cx).browser.read(cx).is_loading()
        })
        .await;
        Self { handle, shell }
    }
}

#[gpui_kit::test]
async fn local_rows_scale_indent_marks_range_disclosure_and_context_without_transfers(
    cx: &mut TestAppContext,
) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir(local.path().join("child")).unwrap();
    fs::write(local.path().join("child/a.txt"), "nested local").unwrap();
    fs::write(local.path().join("z.txt"), "local only").unwrap();
    let f = Fixture::new(
        1200.,
        900.,
        Store::new(config.path().into()),
        local.path(),
        None,
        cx,
    )
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(
            w.within("local-files-pane")
                .find(0usize)
                .bounds()
                .size
                .height,
            px(24.)
        );
        assert_eq!(
            w.find(("local-tree-toggle", 0usize)).bounds().size.width,
            px(20.)
        );
        w.click(("local-tree-toggle", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(20), |_, cx| {
        !f.shell.read(cx).browser.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        let parent = w.find(("local-tree-toggle", 0usize)).bounds();
        let child = w.find(("local-tree-toggle", 1usize)).bounds();
        assert_eq!(child.left() - parent.left(), px(16.));
        f.shell.read(cx).browser.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("down", cx);
        w.press("space", cx);
        assert_eq!(f.shell.read(cx).browser.read(cx).marked(), ["child/a.txt"]);
        w.press("home", cx);
        w.press("v", cx);
        w.press("end", cx);
        let marks = f.shell.read(cx).browser.read(cx).marked();
        assert_eq!(marks.len(), 1);
        let browser = f.shell.read(cx).browser.clone();
        let id = browser.read(cx).id();
        let focus = w.focused(cx);
        Theme::update(cx, |theme| theme.font_size = px(32.));
        crate::test_support::settle(w, cx);
        assert_eq!(
            w.within("local-files-pane")
                .find(0usize)
                .bounds()
                .size
                .height,
            px(48.)
        );
        assert_eq!(
            w.find(("local-tree-toggle", 1usize)).bounds().left()
                - w.find(("local-tree-toggle", 0usize)).bounds().left(),
            px(32.)
        );
        assert_eq!(
            w.find(("local-tree-toggle", 0usize)).bounds().size.width,
            px(40.)
        );
        assert_eq!(browser.read(cx).id(), id);
        assert_eq!(browser.read(cx).marked(), marks);
        assert_eq!(w.focused(cx), focus);
        w.press("v", cx);
        let marks = browser.read(cx).marked();
        assert_eq!(marks.len(), 3);
        w.within("local-files-pane").right_click(1usize, cx);
        assert!(w.find("popup-menu").visible());
        assert_eq!(browser.read(cx).selected(), Some("child/a.txt"));
        assert_eq!(browser.read(cx).marked(), marks);
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(f.handle, |_, w, cx| {
        let browser = f.shell.read(cx).browser.clone();
        let marks = browser.read(cx).marked();
        assert_eq!(marks.len(), 3);
        w.click(("local-tree-toggle", 0usize), cx);
        assert_eq!(browser.read(cx).len(), 2);
        assert_eq!(browser.read(cx).marked(), marks);
        assert!(!f.shell.read(cx).comparison.read(cx).visible());
        assert!(!f.shell.read(cx).local_preview.read(cx).is_loading());
        assert_eq!(
            fs::read(local.path().join("child/a.txt")).unwrap(),
            b"nested local"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
async fn narrow_shell_long_paths_and_host_names_keep_actions_in_bounds(cx: &mut TestAppContext) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let root = local.path().join("long-project-path-".repeat(12));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("long-file-name-".repeat(12)), "no style transfer").unwrap();
    let store = Store::new(config.path().into());
    let host = drift_core::config::Host {
        name: "long-host-name-".repeat(24),
        hostname: "localhost".into(),
        ..Default::default()
    };
    store.save_host(None, None, host.clone()).unwrap();
    let f = Fixture::new(420., 1200., store, &root, None, cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        Theme::update(cx, |theme| theme.font_size = px(24.));
        w.click("remote-files-tab", cx);
        crate::test_support::settle(w, cx);
        let browser_id = f.shell.read(cx).browser.read(cx).id();
        let connection = f.shell.read(cx).remote.read(cx).connection();
        toolbar_scopes::assert_workspace_toolbar_scopes(w);
        for (id, _) in CONTROLS.into_iter().chain(COMPARE_CONTROLS) {
            let item = w.find(id);
            assert!(item.visible(), "{id}");
            assert!(
                item.bounds().left() >= px(0.) && item.bounds().right() <= px(420.),
                "{id}: {:?}",
                item.bounds()
            );
            assert!(item.bounds().bottom() <= px(1200.), "{id}");
        }
        for id in ["remote-filter", "filter"] {
            let item = w.find(id);
            assert!(item.visible(), "{id}");
            assert!(
                item.bounds().left() >= px(0.) && item.bounds().right() <= px(420.),
                "{id}: {:?}",
                item.bounds()
            );
            assert!(item.bounds().bottom() <= px(1200.), "{id}");
        }
        // Both tab groups scroll horizontally when the pane is too narrow; the
        // remote one scrolls on its own line above the wrapped host picker.
        let pane = w.find("remote-workspace").bounds();
        for (scroller, tab) in [
            ("local-pane-tabs", "local-preview-tab"),
            ("remote-tab-group", "remote-preview-tab"),
        ] {
            let viewport = w.find(scroller).bounds();
            for _ in 0..400 {
                let button = w.within(scroller).find(tab).bounds();
                if button.left() >= viewport.left() && button.right() <= viewport.right() {
                    break;
                }
                w.scroll(
                    scroller,
                    gpui_kit::ScrollDelta::Pixels(point(px(-4.), px(0.))),
                    cx,
                );
                w.render_frame(cx);
            }
            let button = w.within(scroller).find(tab);
            assert!(
                button.visible(),
                "{tab} must remain reachable by horizontal scroll"
            );
            assert!(button.bounds().right() <= viewport.right() + px(1.));
            assert!(
                button.bounds().top() >= viewport.top()
                    && button.bounds().bottom() <= viewport.bottom()
            );
        }
        let group = w.find("remote-tab-group").bounds();
        assert!(group.left() >= pane.left() && group.right() <= pane.right());
        assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
        assert_eq!(f.shell.read(cx).remote.read(cx).connection(), connection);
        assert!(!f.shell.read(cx).remote.read(cx).has_session());
        assert!(!f.shell.read(cx).comparison.read(cx).visible());
        assert!(!f.shell.read(cx).local_preview.read(cx).is_loading());
        assert!(!f.shell.read(cx).remote_preview.read(cx).is_loading());
        workspace_headers::assert_navigation_scroll(w, cx);
        let connect = w.find("remote-host-select");
        assert_eq!(connect.role(), Some(gpui_kit::Role::ComboBox));
        assert!(connect.label().is_some_and(|label| !label.is_empty()));
        assert!(connect.visible());
        assert!(
            connect.bounds().right() <= px(420.),
            "long host picker must fit: {:?}",
            connect.bounds()
        );
        let disconnect = w.find("disconnect");
        assert!(disconnect.visible(), "disconnect");
        assert!(
            disconnect.bounds().left() >= px(0.) && disconnect.bounds().right() <= px(420.),
            "disconnect: {:?}",
            disconnect.bounds()
        );
        assert!(disconnect.bounds().bottom() <= px(1200.), "disconnect");
    })
    .unwrap();
}

#[gpui_kit::test]
async fn palette_and_font_changes_preserve_real_remote_session_rows_and_active_pane_focus(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let remote_root = server.dir.path().join("files");
    fs::create_dir(remote_root.join("child")).unwrap();
    fs::write(remote_root.join("child/a.txt"), "remote child").unwrap();
    fs::write(remote_root.join("z.txt"), "remote only").unwrap();
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::write(local.path().join("local.txt"), "local unchanged").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let f = Fixture::new(
        1400.,
        1000.,
        store,
        local.path(),
        Some(server.options()),
        cx,
    )
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote-files-tab", cx);
        crate::test_support::connect_first(w, cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(20), |_, cx| {
        !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    assert!(
        f.shell
            .read_with(cx, |s, cx| s.remote.read(cx).has_session())
    );
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(
            w.within("remote-files-pane")
                .find(0usize)
                .bounds()
                .size
                .height,
            px(24.)
        );
        assert_eq!(
            w.find(("remote-tree-toggle", 0usize)).bounds().size.width,
            px(20.)
        );
        w.click(("remote-tree-toggle", 0usize), cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(20), |_, cx| {
        !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(w.find(("remote-tree-toggle", 1usize)).bounds().left() - w.find(("remote-tree-toggle", 0usize)).bounds().left(), px(16.));
        let remote = f.shell.read(cx).remote.clone();
        let browser = f.shell.read(cx).browser.clone();
        remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx); w.press("down", cx); w.press("space", cx);
        assert_eq!(remote.read(cx).marked(), [remote_root.join("child/a.txt").to_string_lossy()]);
        w.press("home", cx); w.press("v", cx); w.press("end", cx); w.press("v", cx);
        let marks = remote.read(cx).marked();
        assert_eq!(marks.len(), 3);
        let session = remote.read(cx).session_id();
        let connection = remote.read(cx).connection();
        let browser_id = browser.read(cx).id();
        let selection = remote.read(cx).selected().unwrap().to_owned();
        let focus = w.focused(cx);
        for (mode, font) in [(ThemeMode::Light, 24.), (ThemeMode::Dark, 16.)] {
            Theme::change(mode, Some(w), cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            crate::test_support::settle(w, cx);
            assert_eq!(w.rem_size(), px(font));
            assert_eq!(w.within("remote-files-pane").find(0usize).bounds().size.height, px(24. * font / 16.));
            assert_eq!(w.find(("remote-tree-toggle", 1usize)).bounds().left() - w.find(("remote-tree-toggle", 0usize)).bounds().left(), px(font));
            assert_eq!(w.within("local-files-pane").find(0usize).bounds().size.height, px(24. * font / 16.));
            assert_eq!(remote.read(cx).session_id(), session);
            assert_eq!(remote.read(cx).connection(), connection);
            assert_eq!(browser.read(cx).id(), browser_id);
            assert_eq!(remote.read(cx).marked(), marks);
            assert_eq!(remote.read(cx).selected(), Some(selection.as_str()));
            assert_eq!(w.focused(cx), focus);
            toolbar_scopes::assert_workspace_toolbar_scopes(w);
            for (id, label) in CONTROLS.into_iter().chain(LOCAL_CONTROLS).chain(COMPARE_CONTROLS).chain(workspace_headers::REMOTE_NAV_CONTROLS) { assert_eq!(w.find(id).label(), Some(label)); }
            let chosen_row = w.within("remote-files-pane").find(2usize).bounds().scale(w.scale_factor());
            assert!(w.painted_quads().iter().any(|q| q.bounds == chosen_row && q.background == Palette::current(cx).selected.into()));
            let pane = w.find("remote-files-pane").bounds().scale(w.scale_factor());
            assert!(w.painted_quads().iter().any(|q| q.bounds == pane && q.border_color == Palette::current(cx).focus), "active pane needs a painted focus border");
            browser.focus_handle(cx).focus(w, cx);
            w.press("home", cx);
            w.render_frame(cx);
            let local_pane = w.find("browser-pane").bounds().scale(w.scale_factor());
            assert!(w.painted_quads().iter().any(|q| q.bounds == local_pane && q.border_color == Palette::current(cx).focus));
            assert!(!w.painted_quads().iter().any(|q| q.bounds == pane && q.border_color == Palette::current(cx).focus));
            assert_eq!(remote.read(cx).selected(), Some(selection.as_str()));
            assert!(w.painted_quads().iter().any(|q| q.bounds == chosen_row && q.background == Palette::current(cx).selected.into()), "chosen row stays painted when its pane loses keyboard focus");
            remote.focus_handle(cx).focus(w, cx);
        }
        w.within("remote-files-pane").right_click(1usize, cx);
        assert!(w.find("popup-menu").visible());
        assert_eq!(remote.read(cx).marked(), marks);
        w.press("escape", cx);
        w.click(("remote-tree-toggle", 0usize), cx);
        assert_eq!(remote.read(cx).marked(), marks);
        assert_eq!(remote.read(cx).session_id(), session);
        assert!(!f.shell.read(cx).comparison.read(cx).visible());
        assert!(!f.shell.read(cx).remote_preview.read(cx).is_loading());
        assert_eq!(fs::read(local.path().join("local.txt")).unwrap(), b"local unchanged");
        assert_eq!(fs::read(remote_root.join("child/a.txt")).unwrap(), b"remote child");
        assert!(!local.path().join("z.txt").exists());
        assert!(!remote_root.join("local.txt").exists());
    }).unwrap();
}

#[gpui_kit::test]
async fn scaled_virtual_rows_keep_keyboard_scroll_selection_and_range(cx: &mut TestAppContext) {
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    for index in 0..512 {
        fs::write(
            local.path().join(format!("file-{index:03}.txt")),
            "untouched",
        )
        .unwrap();
    }
    let f = Fixture::new(
        1200.,
        650.,
        Store::new(config.path().into()),
        local.path(),
        None,
        cx,
    )
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        let browser = f.shell.read(cx).browser.clone();
        browser.focus_handle(cx).focus(w, cx);
        let id = browser.read(cx).id();
        for font in [16., 24., 32.] {
            Theme::update(cx, |theme| theme.font_size = px(font));
            // Keep a usable list viewport while testing scaled virtual rows.
            w.resize(size(px(1200.), metric(650., cx)));
            w.bounds_changed(cx);
            crate::test_support::settle(w, cx);
            w.press("end", cx);
            w.render_frame(cx);
            assert_eq!(browser.read(cx).selected(), Some("file-511.txt"));
            let last = w.within("local-files-pane").find(511usize);
            assert!(last.visible());
            assert_eq!(last.bounds().size.height, metric(token::ROW, cx));
            let rendered = (0..512)
                .filter(|index| w.within("local-files-pane").try_find(*index).is_some())
                .count();
            assert!(
                rendered > 0 && rendered < 64,
                "only the viewport's rows should mount, got {rendered}"
            );
            w.press("home", cx);
            w.render_frame(cx);
            assert!(w.within("local-files-pane").find(0usize).visible());
            w.press("shift-down", cx);
            assert_eq!(browser.read(cx).marked(), ["file-000.txt", "file-001.txt"]);
            w.press("escape", cx);
            assert!(browser.read(cx).marked().is_empty());
            assert_eq!(browser.read(cx).id(), id);
            assert!(!browser.read(cx).is_loading());
            assert!(!f.shell.read(cx).local_preview.read(cx).is_loading());
            assert!(!f.shell.read(cx).comparison.read(cx).visible());
        }
        assert_eq!(
            fs::read(local.path().join("file-511.txt")).unwrap(),
            b"untouched"
        );
    })
    .unwrap();
}
