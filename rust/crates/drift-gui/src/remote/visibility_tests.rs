use super::*;
use crate::{browser_menu::MenuAction, sftp_test_support as support};
use drift_app::browser::BrowserService;
use drift_core::{pathmap::Mapping, remote::ConnectOptions, store::Store};
use gpui_kit::test::{ElementSnapshot, TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Bounds, ScrollDelta, TestAppContext, WindowBounds, WindowOptions, size,
};
use std::{fs, path::Path, process::Command, time::Duration};

struct VisibilityWindow {
    pane: Entity<RemotePane>,
    statuses: Vec<String>,
    effects: Vec<&'static str>,
    _subscription: Subscription,
}
impl Render for VisibilityWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Drift")
            .size_full()
            .flex()
            .child(self.pane.clone())
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("Git is required for visibility tests");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_root(ignore: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::create_dir(root.path().join("alternate")).unwrap();
    git(root.path(), &["init", "--quiet"]);
    git(root.path(), &["config", "core.excludesFile", "/dev/null"]);
    fs::write(root.path().join(".gitignore"), ignore).unwrap();
    root
}

async fn location(root: &Path, mappings: Vec<Mapping>) -> Location {
    let config = tempfile::tempdir().unwrap();
    let service = BrowserService::new().unwrap();
    let directory = service
        .open(
            Store::new(config.path().into()),
            root.into(),
            OperationId {
                project: 7,
                operation: 1,
            },
            true,
            true,
        )
        .task
        .await
        .unwrap()
        .unwrap();
    assert_eq!(directory.location.root.base(), root);
    let mut location = directory.location;
    location.config.mappings = mappings;
    location
}

fn mapping(local: &str, remote: &str) -> Mapping {
    Mapping {
        local: local.into(),
        remote: remote.into(),
    }
}

struct Fixture {
    handle: AnyWindowHandle,
    view: Entity<VisibilityWindow>,
    pane: Entity<RemotePane>,
}
impl Fixture {
    fn open(
        cx: &mut TestAppContext,
        host: Host,
        options: ConnectOptions,
        location: Option<Location>,
    ) -> Self {
        cx.executor().allow_parking();
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::actions::bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1100.), px(1000.)),
                    })),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx: &mut Context<VisibilityWindow>| {
                        let pane = cx.new(|cx| {
                            RemotePane::new(
                                RemoteService::with_options(
                                    BrowserService::new().unwrap(),
                                    options,
                                ),
                                window,
                                cx,
                            )
                        });
                        pane.update(cx, |pane, cx| {
                            pane.set_context(7, vec![host], cx);
                            pane.set_selection_location(location, window, cx);
                            pane.set_comparison_ready(true, window, cx);
                        });
                        let subscription = cx.subscribe(&pane, |this, _, event, _| match event {
                            RemoteEvent::Status { message, .. } => {
                                this.statuses.push(message.clone())
                            }
                            RemoteEvent::Preview { .. } => this.effects.push("preview"),
                            RemoteEvent::Compare { .. } => this.effects.push("compare"),
                            RemoteEvent::Toolbar { .. } => this.effects.push("toolbar"),
                            _ => {}
                        });
                        VisibilityWindow {
                            pane,
                            statuses: vec![],
                            effects: vec![],
                            _subscription: subscription,
                        }
                    })
                },
            )
            .unwrap()
        });
        let pane = view.read_with(cx, |view, _| view.pane.clone());
        Self { handle, view, pane }
    }
    async fn idle(&self, cx: &mut TestAppContext) {
        cx.wait_for(self.handle, Duration::from_secs(60), |_, cx| {
            !self.pane.read(cx).is_loading()
        })
        .await;
    }
    async fn connect(&self, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, window, cx| {
            window.click(("connect-host", 0usize), cx)
        })
        .unwrap();
        self.idle(cx).await;
        assert!(self.pane.read_with(cx, |pane, _| pane.has_session()));
    }
    async fn filter(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.handle, |_, window, cx| {
            window.click("remote-filter", cx);
            window.press(
                if cfg!(target_os = "macos") {
                    "cmd-a"
                } else {
                    "ctrl-a"
                },
                cx,
            );
            window.input(text, cx);
        })
        .unwrap();
        // InputEvent::Change is delivered after the window transaction.
        cx.update_window(self.handle, |_, window, cx| {
            assert_eq!(self.pane.read(cx).filter.read(cx).value().as_str(), text);
            window.press("enter", cx);
        })
        .unwrap();
    }
    fn no_effects(&self, cx: &TestAppContext) {
        assert!(self.view.read_with(cx, |view, _| view.effects.is_empty()));
    }
}

fn visible(pane: &RemotePane, path: &str) -> bool {
    (0..pane.files.len()).any(|row| pane.files.row(row) == Some(path))
}

fn menu_item(window: &mut Window, label: &str) -> (usize, ElementSnapshot) {
    let menu = window.within("popup-menu");
    (0..32)
        .find_map(|index| {
            let item = menu.try_find(index)?;
            (item.label() == Some(label)).then_some((index, item))
        })
        .unwrap_or_else(|| panic!("missing remote menu item {label}"))
}

fn click_menu(window: &mut Window, label: &str, cx: &mut App) {
    let (index, item) = menu_item(window, label);
    if !item.visible() {
        window.scroll(
            "popup-menu",
            ScrollDelta::Pixels(point(px(0.), px(-1000.))),
            cx,
        );
    }
    window.within("popup-menu").click(index, cx);
}

fn ignored_checked(window: &mut Window, expected: bool, cx: &mut App) {
    window.right_click("remote-files-pane", cx);
    assert!(menu_item(window, "Show ignored").1.visible());
    // Kit renders a check icon but does not expose it as checked observation.
    assert_eq!(
        window.find("remote-ignored").label(),
        Some(if expected {
            "Hide ignored"
        } else {
            "Show ignored"
        })
    );
    window.press("escape", cx);
}

#[gpui_kit::test]
async fn mapped_git_remote_only_negation_tracked_and_hard_exclusions_preserve_marks(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let remote = server.dir.path().join("files");
    fs::create_dir(remote.join("deployed")).unwrap();
    fs::create_dir(remote.join("ordinary")).unwrap();
    fs::create_dir(remote.join("metadata")).unwrap();
    let staging = drift_core::staging::staging_name("partial.txt").unwrap();
    for name in [
        "cache.log",
        "keep.log",
        "tracked.log",
        "visible.txt",
        ".hidden",
        &staging,
    ] {
        fs::write(remote.join("deployed").join(name), "remote contents").unwrap();
    }
    fs::write(remote.join("ordinary/package.txt"), "hard excluded locally").unwrap();
    let local = git_root("/src/*.log\n!/src/keep.log\n");
    fs::write(
        local.path().join("src/tracked.log"),
        "local tracked contents",
    )
    .unwrap();
    git(local.path(), &["add", "-f", "src/tracked.log"]);
    assert!(!local.path().join("src/cache.log").exists());
    let mut host = server.host();
    host.mappings = vec![
        mapping("src", "deployed"),
        mapping("node_modules", "ordinary"),
        mapping(".git", "metadata"),
    ];
    let fixture = Fixture::open(
        cx,
        host,
        server.options(),
        Some(location(local.path(), vec![]).await),
    );
    fixture.connect(cx).await;
    let pane = &fixture.pane;
    let folder = remote.join("deployed").to_string_lossy().into_owned();
    let ignored = remote
        .join("deployed/cache.log")
        .to_string_lossy()
        .into_owned();
    let session = pane.read_with(cx, |pane, _| pane.session.clone().unwrap());
    cx.update_window(fixture.handle, |_, window, cx| {
        assert!(!pane.read(cx).show_ignored);
        assert!(window.find("remote-ignored").visible());
        ignored_checked(window, false, cx);
        for name in ["ordinary", "metadata"] {
            let path = remote.join(name).to_string_lossy().into_owned();
            assert!(!visible(pane.read(cx), &path));
            assert!(
                pane.read(cx)
                    .visibility
                    .as_ref()
                    .unwrap()
                    .excluded
                    .contains(&path)
            );
            assert!(!pane.read(cx).files.can_mark(&path));
        }
        pane.update(cx, |pane, cx| pane.expand(folder.clone(), window, cx));
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        assert!(!visible(pane.read(cx), &ignored));
        assert!(
            pane.read(cx)
                .visibility
                .as_ref()
                .unwrap()
                .ignored
                .contains(&ignored)
        );
        for name in ["keep.log", "tracked.log", "visible.txt"] {
            assert!(visible(
                pane.read(cx),
                remote.join("deployed").join(name).to_str().unwrap()
            ));
        }
        assert!(!visible(
            pane.read(cx),
            remote.join("deployed/.hidden").to_str().unwrap()
        ));
        window.click("remote-ignored", cx);
        assert!(pane.read(cx).show_ignored);
        assert!(visible(pane.read(cx), &ignored));
        ignored_checked(window, true, cx);
        pane.update(cx, |pane, cx| {
            pane.files.select_path(&ignored);
            pane.focus.focus(window, cx);
        });
        window.press("space", cx);
        assert_eq!(pane.read(cx).marked(), std::slice::from_ref(&ignored));
        window.press("shift-i", cx);
        assert!(!pane.read(cx).show_ignored);
        assert!(!visible(pane.read(cx), &ignored));
        assert!(pane.read(cx).tree.node(&ignored).is_some());
        assert!(
            pane.read(cx)
                .entries
                .iter()
                .any(|entry| entry.path == ignored)
        );
        assert_eq!(pane.read(cx).marked(), std::slice::from_ref(&ignored));
        window.right_click("remote-files-pane", cx);
        click_menu(window, "Show ignored", cx);
        assert!(visible(pane.read(cx), &ignored));
        window.press(".", cx);
        assert!(pane.read(cx).show_hidden);
        assert!(visible(
            pane.read(cx),
            remote.join("deployed/.hidden").to_str().unwrap()
        ));
        for name in ["ordinary", "metadata"] {
            let path = remote.join(name).to_string_lossy().into_owned();
            assert!(
                !visible(pane.read(cx), &path),
                "hard exclusions survive both visibility toggles"
            );
            assert!(!pane.read(cx).files.can_mark(&path));
        }
        let staging_path = remote
            .join("deployed")
            .join(&staging)
            .to_string_lossy()
            .into_owned();
        assert!(!visible(pane.read(cx), &staging_path));
        assert!(
            !pane
                .read(cx)
                .entries
                .iter()
                .any(|entry| entry.path == staging_path)
        );
        assert!(!pane.read(cx).files.can_mark(&staging_path));
        pane.update(cx, |pane, cx| pane.toggle_tree(folder.clone(), window, cx));
        assert!(!visible(pane.read(cx), &ignored));
        assert_eq!(pane.read(cx).marked(), std::slice::from_ref(&ignored));
        assert_eq!(pane.read(cx).files.marked_descendants(&folder), 1);
        pane.update(cx, |pane, cx| pane.expand(folder.clone(), window, cx));
    })
    .unwrap();
    fixture.idle(cx).await;
    fixture.filter(cx, "visible").await;
    cx.update_window(fixture.handle, |_, window, cx| {
        assert_eq!(pane.read(cx).files.len(), 1);
        assert_eq!(pane.read(cx).marked(), std::slice::from_ref(&ignored));
        pane.update(cx, |pane, cx| pane.refresh(&Refresh, window, cx));
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        assert_eq!(pane.read(cx).filter.read(cx).value().as_str(), "visible");
        assert_eq!(pane.read(cx).marked(), std::slice::from_ref(&ignored));
        assert_eq!(pane.read(cx).session_id(), Some(session.id));
        assert_eq!(session.state(), ConnectionState::Connected);
        assert!(pane.read(cx).tree.node(&folder).unwrap().expanded);
        pane.update(cx, |pane, cx| pane.toggle_ignored(window, cx));
        assert_eq!(pane.read(cx).marked(), std::slice::from_ref(&ignored));
    })
    .unwrap();
    fixture.filter(cx, "cache").await;
    cx.update_window(fixture.handle, |_, window, cx| {
        assert!(pane.read(cx).files.is_empty());
        window.press("shift-i", cx);
        assert!(visible(pane.read(cx), &ignored));
        assert_eq!(pane.read(cx).marked(), std::slice::from_ref(&ignored));
    })
    .unwrap();
    fixture.no_effects(cx);
    assert!(!local.path().join("src/cache.log").exists());
    assert_eq!(
        fs::read_to_string(local.path().join("src/tracked.log")).unwrap(),
        "local tracked contents"
    );
    assert_eq!(
        fs::read_to_string(remote.join("deployed/cache.log")).unwrap(),
        "remote contents"
    );
}

#[gpui_kit::test]
async fn cached_ftp_toggle_preserves_filter_marks_history_and_performs_no_network_io(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::ftp::Server::new(4);
    let remote = server.dir.path().join("files");
    fs::create_dir(remote.join("published")).unwrap();
    fs::write(remote.join("published/cache.log"), "remote only").unwrap();
    fs::write(remote.join("published/keep.txt"), "keep").unwrap();
    fs::write(remote.join("unmapped.txt"), "unmapped").unwrap();
    let local = git_root("/src/*.log\n");
    let location = location(local.path(), vec![mapping("src", "published")]).await;
    let fixture = Fixture::open(cx, server.host(), server.options(), Some(location));
    fixture.connect(cx).await;
    let pane = &fixture.pane;
    cx.update_window(fixture.handle, |_, window, cx| {
        assert!(visible(pane.read(cx), "/unmapped.txt"));
        assert!(!pane.read(cx).files.can_mark("/unmapped.txt"));
        pane.update(cx, |pane, cx| {
            pane.files.select_path("/unmapped.txt");
            pane.focus.focus(window, cx);
        });
        window.press("space", cx);
        assert!(pane.read(cx).marked().is_empty());
        pane.update(cx, |pane, cx| {
            pane.navigate("/published".into(), Navigation::Push, window, cx)
        });
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        window.click("remote-ignored", cx);
        pane.update(cx, |pane, cx| {
            pane.files.select_path("/published/cache.log");
            pane.focus.focus(window, cx);
        });
        window.press("space", cx);
        assert_eq!(pane.read(cx).marked(), ["/published/cache.log"]);
    })
    .unwrap();
    fixture.filter(cx, "cache").await;
    let before = fs::read(server.dir.path().join("commands")).unwrap();
    let (session, connection, operation, revision, previous, next) =
        pane.read_with(cx, |pane, _| {
            (
                pane.session.clone().unwrap(),
                pane.connection,
                pane.operation,
                pane.visibility_revision,
                pane.history.previous(),
                pane.history.next(),
            )
        });
    cx.update_window(fixture.handle, |_, window, cx| {
        for _ in 0..3 {
            window.press("shift-i", cx);
            assert!(!pane.read(cx).show_ignored);
            assert!(pane.read(cx).files.is_empty());
            ignored_checked(window, false, cx);
            window.right_click("remote-files-pane", cx);
            click_menu(window, "Show ignored", cx);
            assert!(pane.read(cx).show_ignored);
            assert!(visible(pane.read(cx), "/published/cache.log"));
            ignored_checked(window, true, cx);
        }
        let state = pane.read(cx);
        assert_eq!(state.filter.read(cx).value().as_str(), "cache");
        assert_eq!(state.marked(), ["/published/cache.log"]);
        assert_eq!(state.history.previous(), previous);
        assert_eq!(state.history.next(), next);
        assert_eq!(state.path, "/published");
        assert_eq!(state.session_id(), Some(session.id));
        assert_eq!(state.connection, connection);
        assert_eq!(state.operation, operation);
        assert_eq!(state.visibility_revision, revision);
        assert!(!state.is_loading());
    })
    .unwrap();
    // A fresh local Git query must not be needed to reveal the cached listing.
    fs::write(local.path().join(".gitignore"), "").unwrap();
    cx.update_window(fixture.handle, |_, window, cx| {
        window.press("shift-i", cx);
        assert!(pane.read(cx).files.is_empty());
        window.click("remote-ignored", cx);
        assert!(visible(pane.read(cx), "/published/cache.log"));
    })
    .unwrap();
    assert_eq!(
        fs::read(server.dir.path().join("commands")).unwrap(),
        before
    );
    assert_eq!(session.state(), ConnectionState::Connected);
    fixture.no_effects(cx);
    for command in ["RETR", "STOR", "APPE", "DELE", "RMD", "MKD", "RNFR", "RNTO"] {
        assert_eq!(
            server.commands(command),
            0,
            "unexpected remote operation {command}"
        );
    }
    assert!(!local.path().join("src/cache.log").exists());
    assert_eq!(
        fs::read_to_string(remote.join("published/cache.log")).unwrap(),
        "remote only"
    );
}

#[gpui_kit::test]
async fn mapping_precedence_revisions_roots_projects_and_connections_reject_old_results(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    // Four sessions may overlap asynchronous shutdown; each owns up to four sockets.
    let server = support::ftp::Server::new(16);
    fs::write(server.dir.path().join("files/cache.log"), "remote").unwrap();
    fs::write(server.dir.path().join("files/unmapped.txt"), "unmapped").unwrap();
    let local = git_root("/src/cache.log\n");
    let original = location(local.path(), vec![mapping("src", ".")]).await;
    let mut changed = original.clone();
    changed.config.mappings = vec![mapping("alternate", ".")];
    let fixture = Fixture::open(cx, server.host(), server.options(), Some(original.clone()));
    fixture.connect(cx).await;
    let pane = &fixture.pane;
    let (session, operation, connection) = pane.read_with(cx, |pane, _| {
        (
            pane.session.clone().unwrap(),
            pane.operation,
            pane.connection,
        )
    });
    cx.update_window(fixture.handle, |_, window, cx| {
        assert!(!visible(pane.read(cx), "/cache.log"));
        pane.update(cx, |pane, cx| {
            pane.files.select_path("/unmapped.txt");
            pane.focus.focus(window, cx);
        });
        window.press("menu", cx);
        pane.update(cx, |pane, cx| {
            let (snapshot, menu) = pane.menu.as_ref().unwrap();
            let snapshot = snapshot.clone();
            let popup = menu.popup.entity_id();
            pane.start_visibility(window, cx);
            let cancel = pane.visibility_cancel.as_ref().unwrap().clone();
            let revision = pane.visibility_revision;
            pane.set_selection_location(Some(changed.clone()), window, cx);
            assert!(cancel.is_cancelled());
            assert!(pane.visibility_revision > revision);
            let revision = pane.visibility_revision;
            pane.set_selection_location(Some(original.clone()), window, cx);
            assert!(pane.visibility_revision > revision);
            pane.set_selection_location(Some(changed.clone()), window, cx);
            assert_eq!(
                pane.operation, operation,
                "local classification has an independent revision"
            );
            assert_eq!(pane.connection, connection);
            assert!(!pane.menu_valid(&snapshot, cx));
            pane.menu_action(&snapshot, Some(popup), MenuAction::ComparePath, window, cx);
            assert!(pane.menu.is_none());
        });
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, _, cx| {
        assert!(visible(pane.read(cx), "/cache.log"));
        assert!(
            !pane
                .read(cx)
                .visibility
                .as_ref()
                .unwrap()
                .ignored
                .contains("/cache.log")
        );
        assert_eq!(pane.read(cx).session_id(), Some(session.id));
    })
    .unwrap();
    let other_root = git_root("/alternate/cache.log\n");
    let other = location(other_root.path(), changed.config.mappings.clone()).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        pane.update(cx, |pane, cx| {
            pane.start_visibility(window, cx);
            let cancel = pane.visibility_cancel.as_ref().unwrap().clone();
            let revision = pane.visibility_revision;
            pane.set_selection_location(Some(other.clone()), window, cx);
            assert!(cancel.is_cancelled());
            assert!(pane.visibility_revision > revision);
        });
    })
    .unwrap();
    fixture.idle(cx).await;
    assert!(!pane.read_with(cx, |pane, _| visible(pane, "/cache.log")));
    cx.update_window(fixture.handle, |_, window, cx| {
        pane.update(cx, |pane, cx| {
            pane.start_visibility(window, cx);
            let cancel = pane.visibility_cancel.as_ref().unwrap().clone();
            let revision = pane.visibility_revision;
            let operation = pane.operation;
            pane.refresh(&Refresh, window, cx);
            assert!(
                !cancel.is_cancelled(),
                "busy Refresh must not supersede the classifier"
            );
            assert_eq!(pane.visibility_revision, revision);
            assert_eq!(pane.operation, operation);
        });
    })
    .unwrap();
    fixture.idle(cx).await;
    assert!(!pane.read_with(cx, |pane, _| visible(pane, "/cache.log")));
    let old_revision = cx
        .update_window(fixture.handle, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                pane.start_visibility(window, cx);
                let cancel = pane.visibility_cancel.as_ref().unwrap().clone();
                let revision = pane.visibility_revision;
                pane.set_context(8, vec![server.host()], cx);
                pane.set_selection_location(Some(changed.clone()), window, cx);
                assert!(cancel.is_cancelled());
                assert!(pane.session.is_none());
                assert!(pane.visibility_cancel.is_none());
                assert!(pane.files.is_empty());
                assert!(
                    pane.visibility
                        .as_ref()
                        .is_none_or(|visibility| visibility.ignored.is_empty()
                            && visibility.excluded.is_empty())
                );
                revision
            })
        })
        .unwrap();
    fixture.connect(cx).await;
    cx.update_window(fixture.handle, |_, _, cx| {
        let state = pane.read(cx);
        assert_eq!(state.project, 8);
        assert!(state.visibility_revision > old_revision);
        assert!(visible(state, "/cache.log"));
        assert!(state.visibility.as_ref().unwrap().ignored.is_empty());
        assert_ne!(state.session_id(), Some(session.id));
    })
    .unwrap();
    cx.wait_for(fixture.handle, Duration::from_secs(60), |_, _| {
        session.state() == ConnectionState::Closed
    })
    .await;
    let previous_session = pane.read_with(cx, |pane, _| pane.session.clone().unwrap());
    cx.update_window(fixture.handle, |_, window, cx| {
        pane.update(cx, |pane, cx| {
            pane.set_selection_location(Some(other.clone()), window, cx);
            let cancel = pane.visibility_cancel.as_ref().unwrap().clone();
            let revision = pane.visibility_revision;
            let connection = pane.connection;
            pane.disconnect(cx);
            pane.set_selection_location(Some(changed.clone()), window, cx);
            // Reconnect in the same transaction, before the obsolete callback can run.
            pane.connect_required(server.host(), None, window, cx);
            assert!(cancel.is_cancelled());
            assert!(pane.visibility_revision > revision);
            assert!(pane.connection > connection);
        });
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, _, cx| {
        let state = pane.read(cx);
        assert_eq!(state.project, 8);
        assert!(
            state.has_session(),
            "reconnect failed: {:?}",
            fixture.view.read(cx).statuses
        );
        assert_ne!(state.session_id(), Some(previous_session.id));
        assert!(
            visible(state, "/cache.log"),
            "reconnect: session={:?} root={} visibility={:?} query={:?} entries={:?} statuses={:?}",
            state.session_id(),
            state.directory(),
            state
                .visibility
                .as_ref()
                .map(|visibility| (&visibility.ignored, &visibility.excluded)),
            state.filter.read(cx).value(),
            state
                .entries
                .iter()
                .map(|entry| &entry.path)
                .collect::<Vec<_>>(),
            fixture.view.read(cx).statuses
        );
        assert!(state.visibility.as_ref().unwrap().ignored.is_empty());
        assert_eq!(
            state.session.as_ref().unwrap().state(),
            ConnectionState::Connected
        );
    })
    .unwrap();
    // A host-level mapping overrides the changed project's fallback.
    let mut host = server.host();
    host.mappings = vec![mapping("src/cache.log", "cache.log")];
    cx.update_window(fixture.handle, |_, window, cx| {
        pane.update(cx, |pane, cx| {
            pane.set_context(8, vec![host.clone()], cx);
            pane.set_selection_location(Some(changed.clone()), window, cx);
        });
    })
    .unwrap();
    if !pane.read_with(cx, |pane, _| pane.has_session()) {
        fixture.connect(cx).await;
    }
    fixture.idle(cx).await;
    assert!(!pane.read_with(cx, |pane, _| visible(pane, "/cache.log")));
    cx.update_window(fixture.handle, |_, window, cx| {
        pane.update(cx, |pane, cx| {
            pane.set_selection_location(Some(original.clone()), window, cx)
        });
    })
    .unwrap();
    fixture.idle(cx).await;
    assert!(!pane.read_with(cx, |pane, _| visible(pane, "/cache.log")));
    cx.update_window(fixture.handle, |_, window, cx| {
        assert!(visible(pane.read(cx), "/unmapped.txt"));
        assert!(!pane.read(cx).files.can_mark("/unmapped.txt"));
        pane.update(cx, |pane, cx| {
            pane.files.select_path("/unmapped.txt");
            pane.focus.focus(window, cx);
        });
        window.press("space", cx);
        assert!(pane.read(cx).marked().is_empty());
    })
    .unwrap();
    fixture.no_effects(cx);
}

#[gpui_kit::test]
async fn local_classifier_busy_cancel_disconnect_and_app_cancellation_guard_sessions(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::ftp::Server::new(4);
    fs::write(server.dir.path().join("files/cache.log"), "remote").unwrap();
    fs::write(server.dir.path().join("files/keep.txt"), "visible").unwrap();
    let local = git_root("/src/cache.log\n");
    let location = location(local.path(), vec![mapping("src", ".")]).await;
    let fixture = Fixture::open(cx, server.host(), server.options(), Some(location.clone()));
    fixture.connect(cx).await;
    let pane = &fixture.pane;
    let session = pane.read_with(cx, |pane, _| pane.session.clone().unwrap());
    let operation = pane.read_with(cx, |pane, _| {
        pane.service.classify_entries(
            location.clone(),
            server.host(),
            session.clone(),
            pane.entries.clone(),
            OperationId {
                project: pane.project,
                operation: pane.operation + 1,
            },
        )
    });
    operation.cancel.cancel();
    let result = operation.task.await.unwrap();
    // Cancellation may race an already completed local query; neither outcome owns the session.
    if let Err(error) = result {
        assert!(error.to_string().contains("cancelled"));
    }
    assert!(operation.cancel.is_cancelled());
    assert_eq!(session.state(), ConnectionState::Connected);
    for command in ["Cancel loading", "Disconnect"] {
        let session = pane.read_with(cx, |pane, _| pane.session.clone().unwrap());
        let cancel = cx
            .update_window(fixture.handle, |_, window, cx| {
                pane.update(cx, |pane, cx| {
                    pane.files.select_path("/keep.txt");
                    pane.focus.focus(window, cx);
                    pane.start_visibility(window, cx);
                });
                assert_eq!(pane.read(cx).selected(), Some("/keep.txt"));
                let cancel = pane.read(cx).visibility_cancel.as_ref().unwrap().clone();
                assert!(
                    pane.read(cx).listing.is_none(),
                    "only local classification is busy"
                );
                assert!(pane.read(cx).is_loading());
                let operation = pane.read(cx).operation;
                let revision = pane.read(cx).visibility_revision;
                let show_ignored = pane.read(cx).show_ignored;
                pane.update(cx, |pane, cx| pane.toggle_ignored(window, cx));
                window.press("shift-i", cx);
                window.click("remote-ignored", cx);
                assert_eq!(pane.read(cx).show_ignored, show_ignored);
                assert_eq!(pane.read(cx).operation, operation);
                assert_eq!(pane.read(cx).visibility_revision, revision);
                window.right_click("remote-files-pane", cx);
                let popup = pane.read(cx).menu.as_ref().unwrap().1.popup.entity_id();
                click_menu(window, "Show ignored", cx);
                assert_eq!(
                    pane.read(cx).menu.as_ref().unwrap().1.popup.entity_id(),
                    popup
                );
                pane.update(cx, |pane, cx| pane.preview_selected(window, cx));
                click_menu(window, command, cx);
                assert!(cancel.is_cancelled());
                assert!(!pane.read(cx).has_session());
                assert!(!pane.read(cx).is_loading());
                assert!(pane.read(cx).visibility.is_none());
                assert!(pane.read(cx).files.is_empty());
                cancel
            })
            .unwrap();
        assert!(cancel.is_cancelled());
        cx.wait_for(fixture.handle, Duration::from_secs(60), |_, _| {
            session.state() == ConnectionState::Closed
        })
        .await;
        cx.update_window(fixture.handle, |_, window, cx| {
            assert!(
                pane.read(cx).visibility.is_none(),
                "queued old classifier must not repopulate a disconnected pane"
            );
            let show_ignored = pane.read(cx).show_ignored;
            pane.update(cx, |pane, cx| {
                pane.start_visibility(window, cx);
                pane.toggle_ignored(window, cx);
            });
            assert!(pane.read(cx).visibility_cancel.is_none());
            assert!(!pane.read(cx).is_loading());
            assert_eq!(pane.read(cx).show_ignored, show_ignored);
            assert!(pane.read(cx).files.is_empty());
        })
        .unwrap();
        fixture.connect(cx).await;
        assert!(!pane.read_with(cx, |pane, _| visible(pane, "/cache.log")));
    }
    fixture.no_effects(cx);
    for command in ["RETR", "STOR", "DELE"] {
        assert_eq!(server.commands(command), 0);
    }
}

#[gpui_kit::test]
async fn standalone_without_location_skips_classification_and_ignored_toggle(
    cx: &mut TestAppContext,
) {
    let server = support::ftp::Server::new(4);
    fs::write(server.dir.path().join("files/cache.log"), "remote").unwrap();
    let fixture = Fixture::open(cx, server.host(), server.options(), None);
    fixture.connect(cx).await;
    let pane = &fixture.pane;
    let session = pane.read_with(cx, |pane, _| pane.session.clone().unwrap());
    let commands = fs::read(server.dir.path().join("commands")).unwrap();
    cx.update_window(fixture.handle, |_, window, cx| {
        assert!(pane.read(cx).selection_location.is_none());
        assert!(pane.read(cx).visibility.as_ref().is_none_or(|visibility| {
            visibility.ignored.is_empty() && visibility.excluded.is_empty()
        }));
        assert!(pane.read(cx).visibility_cancel.is_none());
        assert!(!pane.read(cx).show_ignored);
        assert!(visible(pane.read(cx), "/cache.log"));
        pane.update(cx, |pane, cx| {
            pane.start_visibility(window, cx);
            pane.toggle_ignored(window, cx);
        });
        window.press("shift-i", cx);
        window.click("remote-ignored", cx);
        assert!(!pane.read(cx).show_ignored);
        assert!(!pane.read(cx).is_loading());
        window.right_click("remote-files-pane", cx);
        let popup = pane.read(cx).menu.as_ref().unwrap().1.popup.entity_id();
        click_menu(window, "Show ignored", cx);
        assert_eq!(
            pane.read(cx).menu.as_ref().unwrap().1.popup.entity_id(),
            popup
        );
        assert_eq!(session.state(), ConnectionState::Connected);
    })
    .unwrap();
    assert_eq!(
        fs::read(server.dir.path().join("commands")).unwrap(),
        commands
    );
    fixture.no_effects(cx);
}

#[gpui_kit::test]
async fn real_git_index_error_is_reported_without_closing_session_and_refresh_recovers(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::ftp::Server::new(4);
    fs::write(server.dir.path().join("files/cache.log"), "remote").unwrap();
    let local = git_root("/src/cache.log\n");
    fs::write(local.path().join("src/tracked.txt"), "tracked").unwrap();
    git(local.path(), &["add", "src/tracked.txt"]);
    let location = location(local.path(), vec![mapping("src", ".")]).await;
    let index = local.path().join(".git/index");
    let saved = fs::read(&index).unwrap();
    fs::write(&index, b"invalid Git index").unwrap();
    let fixture = Fixture::open(cx, server.host(), server.options(), Some(location));
    fixture.connect(cx).await;
    let session = fixture
        .pane
        .read_with(cx, |pane, _| pane.session.clone().unwrap());
    assert!(fixture.view.read_with(cx, |view, _| {
        view.statuses
            .iter()
            .any(|message| message.contains("Git ignore classification"))
    }));
    assert_eq!(session.state(), ConnectionState::Connected);
    assert!(
        fixture
            .pane
            .read_with(cx, |pane, _| pane.visibility_cancel.is_none())
    );
    fs::write(&index, saved).unwrap();
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture
            .pane
            .update(cx, |pane, cx| pane.refresh(&Refresh, window, cx));
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, window, cx| {
        let state = fixture.pane.read(cx);
        assert!(!visible(state, "/cache.log"));
        assert!(
            state
                .visibility
                .as_ref()
                .unwrap()
                .ignored
                .contains("/cache.log")
        );
        assert_eq!(state.session_id(), Some(session.id));
        window.click("remote-ignored", cx);
        assert!(visible(fixture.pane.read(cx), "/cache.log"));
    })
    .unwrap();
    fixture.no_effects(cx);
    for command in ["RETR", "STOR", "DELE"] {
        assert_eq!(server.commands(command), 0);
    }
}

#[gpui_kit::test]
async fn marked_off_tree_directories_and_restore_never_override_mapped_hard_exclusions(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let remote = server.dir.path().join("files");
    fs::create_dir(remote.join("folder")).unwrap();
    fs::write(remote.join("folder/child.txt"), "remote").unwrap();
    let local = git_root("");
    let context = location(local.path(), vec![mapping("src", "folder")]).await;
    let fixture = Fixture::open(cx, server.host(), server.options(), Some(context.clone()));
    fixture.connect(cx).await;
    let folder = remote.join("folder").to_string_lossy().into_owned();
    cx.update_window(fixture.handle, |_, w, cx| {
        fixture.pane.update(cx, |pane, cx| {
            pane.files.select_path(&folder);
            pane.focus.focus(w, cx);
        });
        w.press("space", cx);
        w.press("alt-enter", cx);
    })
    .unwrap();
    fixture.idle(cx).await;
    assert_eq!(
        fixture.pane.read_with(cx, |pane, _| pane.marked()),
        std::slice::from_ref(&folder)
    );
    let mut excluded = context.clone();
    excluded.config.mappings = vec![mapping("node_modules", "folder")];
    cx.update_window(fixture.handle, |_, w, cx| {
        fixture.pane.update(cx, |pane, cx| {
            pane.set_selection_location(Some(excluded.clone()), w, cx)
        });
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, w, cx| {
        assert!(fixture.pane.read(cx).marked().is_empty());
        assert!(fixture.pane.read(cx).files.is_empty());
        w.click("remote-up", cx);
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, w, cx| {
        fixture.pane.update(cx, |pane, cx| {
            pane.set_selection_location(Some(context.clone()), w, cx)
        });
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, w, cx| {
        fixture
            .pane
            .update(cx, |pane, cx| pane.expand(folder.clone(), w, cx));
    })
    .unwrap();
    fixture.idle(cx).await;
    assert!(
        fixture
            .pane
            .read_with(cx, |pane, _| pane.tree.node(&folder).unwrap().expanded)
    );
    cx.update_window(fixture.handle, |_, w, cx| {
        fixture.pane.update(cx, |pane, cx| {
            pane.set_selection_location(Some(excluded.clone()), w, cx)
        });
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, w, cx| {
        w.click("remote-refresh", cx);
    })
    .unwrap();
    fixture.idle(cx).await;
    cx.update_window(fixture.handle, |_, w, cx| {
        let pane = fixture.pane.read(cx);
        assert!(!pane.tree.node(&folder).unwrap().expanded);
        assert!(pane.tree.node(&format!("{folder}/child.txt")).is_none());
        assert!(pane.tree.expanded_paths().any(|path| path == &folder));
        fixture.pane.update(cx, |pane, cx| {
            pane.expand(folder.clone(), w, cx);
            assert!(!pane.is_loading());
            pane.toggle_ignored(w, cx);
            assert!(!visible(pane, &folder));
        });
    })
    .unwrap();
    fixture.no_effects(cx);
}

#[gpui_kit::test]
async fn external_host_root_names_are_not_project_exclusions(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = support::Server::new(false);
    let root = server.dir.path().join("files/node_modules/site");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("normal.txt"), "remote").unwrap();
    fs::write(root.join("node_modules"), "regular file").unwrap();
    let local = git_root("");
    let mut host = server.host();
    host.root_path = root.to_string_lossy().into_owned();
    let fixture = Fixture::open(
        cx,
        host,
        server.options(),
        Some(location(local.path(), vec![]).await),
    );
    fixture.connect(cx).await;
    cx.update_window(fixture.handle, |_, w, cx| {
        let normal = root.join("normal.txt").to_string_lossy().into_owned();
        let named = root.join("node_modules").to_string_lossy().into_owned();
        let pane = fixture.pane.read(cx);
        assert!(visible(pane, &normal));
        assert!(visible(pane, &named));
        assert!(pane.files.can_mark(&normal));
        assert!(pane.files.can_mark(&named));
        fixture.pane.update(cx, |pane, cx| {
            pane.focus.focus(w, cx);
            pane.files.select_path(&normal);
        });
        w.press("space", cx);
        assert_eq!(fixture.pane.read(cx).marked(), [normal]);
        w.click("remote-refresh", cx);
    })
    .unwrap();
    fixture.idle(cx).await;
    assert_eq!(fixture.pane.read_with(cx, |pane, _| pane.files.len()), 2);
    assert_eq!(fixture.pane.read_with(cx, |pane, _| pane.marked().len()), 1);
    fixture.no_effects(cx);
}
