use super::*;
use crate::design::space;

pub(super) const REMOTE_NAV_CONTROLS: [(&str, &str); 7] = [
    ("remote-back", "Back"),
    ("remote-forward", "Forward"),
    ("remote-up", "Up"),
    ("remote-refresh", "Refresh"),
    ("remote-ignored", "Show ignored"),
    ("remote-hidden", "Show hidden"),
    ("remote-cancel", "Cancel remote operation"),
];

pub(super) fn assert_aligned_headers(w: &mut Window, cx: &gpui_kit::App) {
    toolbar_scopes::assert_workspace_toolbar_scopes(w);
    for scope in ["local-workspace", "remote-workspace"] {
        let workspace = w.find(scope).bounds();
        for id in ["preview-header", "preview-pane"] {
            if let Some(preview) = w.within(scope).try_find(id) {
                let bounds = preview.bounds();
                assert!(
                    bounds.left() >= workspace.left() && bounds.right() <= workspace.right(),
                    "{scope}/{id}: {bounds:?}"
                );
                assert!(
                    bounds.top() >= workspace.top() && bounds.bottom() <= workspace.bottom(),
                    "{scope}/{id}: {bounds:?}"
                );
            }
        }
    }
    let local_slot = w.find("local-aligned-header").bounds();
    let remote_slot = w.find("remote-aligned-header").bounds();
    assert_eq!(local_slot.top(), remote_slot.top());
    assert_eq!(local_slot.bottom(), remote_slot.bottom());
    let local_natural_bottom = w
        .within("local-workspace")
        .try_find("preview-header")
        .or_else(|| w.try_find("local-actions-header"))
        .unwrap_or_else(|| w.find("local-aligned-filter"))
        .bounds()
        .bottom();
    let remote_natural_bottom = w
        .within("remote-workspace")
        .try_find("preview-header")
        .unwrap_or_else(|| w.find("remote-aligned-filter"))
        .bounds()
        .bottom();
    assert!(
        (local_slot.bottom() - local_natural_bottom.max(remote_natural_bottom)).abs() <= px(1.),
        "shared chrome must use the actual maximum natural height, not a fixed blank spacer"
    );
    let local_path = w
        .within("local-workspace")
        .find("local-path-header")
        .bounds();
    let remote_path = w
        .within("remote-workspace")
        .find("remote-path-header")
        .bounds();
    assert_eq!(
        local_path.top(),
        remote_path.top(),
        "path rows must follow the shared measured chrome height"
    );
    assert_eq!(local_path.bottom(), remote_path.bottom());
    assert_eq!(local_path.size.height, metric(token::HEADER, cx));
    let mut bodies = Vec::new();
    let mut footers = Vec::new();
    for (scope, path_id, list_id, footer_id, breadcrumbs) in [
        (
            "local-workspace",
            "local-path-header",
            "local-files-pane",
            "local-selection-status",
            "local-breadcrumbs",
        ),
        (
            "remote-workspace",
            "remote-path-header",
            "remote-files-pane",
            "remote-selection-status",
            "remote-breadcrumbs",
        ),
    ] {
        let path = w.within(scope).find(path_id).bounds();
        let footer = w.within(scope).find(footer_id).bounds();
        let workspace_id = gpui_kit::ElementId::from(scope);
        let editors = snapshots(w)
            .into_iter()
            .filter(|s| {
                s.path().contains(&workspace_id)
                    && s.role() == Some(gpui_kit::Role::MultilineTextInput)
            })
            .collect::<Vec<_>>();
        let preview = w.within(scope).try_find("preview-location");
        let body = if preview.is_some() {
            assert_eq!(editors.len(), 1, "one native preview editor in {scope}");
            assert!(
                editors[0]
                    .path()
                    .contains(&gpui_kit::ElementId::from("preview-pane"))
            );
            editors[0].bounds()
        } else {
            w.within(scope).find(list_id).bounds()
        };
        assert_eq!(path.bottom(), body.top(), "{scope}: path/body gap");
        assert_eq!(
            body.bottom(),
            footer.top(),
            "{scope}: footer belongs below the body in both views"
        );
        assert_eq!(footer.bottom(), w.find(scope).bounds().bottom());
        assert!(body.size.height > px(0.), "{scope} needs a usable body");
        if preview.is_some() {
            let title = w.within(scope).find("preview-location").bounds();
            assert_eq!(title.top(), path.top());
            assert_eq!(title.bottom(), path.bottom());
            assert!(
                w.within(scope)
                    .within("preview")
                    .try_find("preview-header")
                    .is_none()
            );
            assert!(
                w.within(scope)
                    .within("preview")
                    .try_find("preview-location")
                    .is_none()
            );
            assert!(
                w.within(scope)
                    .within("preview")
                    .try_find(footer_id)
                    .is_none()
            );
        } else {
            let crumb = w.within(scope).find(breadcrumbs).bounds();
            assert!(crumb.top() >= path.top() && crumb.bottom() <= path.bottom());
            let pane = if scope == "local-workspace" {
                "browser-pane"
            } else {
                "remote-pane"
            };
            assert!(w.within(pane).try_find(path_id).is_none());
            assert!(w.within(pane).try_find(footer_id).is_none());
        }
        bodies.push(body);
        footers.push(footer);
    }
    assert_eq!(
        bodies[0].top(),
        bodies[1].top(),
        "list/editor tops must align"
    );
    assert_eq!(
        bodies[0].bottom(),
        bodies[1].bottom(),
        "list/editor bottoms must align"
    );
    assert_eq!(footers[0].top(), footers[1].top());
    assert_eq!(footers[0].bottom(), footers[1].bottom());
    let local_tabs = w.within("local-workspace").find("local-pane-tabs").bounds();
    let remote_tabs = w
        .within("remote-workspace")
        .find("remote-pane-tabs")
        .bounds();
    assert_eq!(local_tabs.top(), remote_tabs.top());
    assert_eq!(local_tabs.size.height, metric(token::HEADER, cx));
    assert!(remote_tabs.size.height >= metric(token::HEADER, cx));
    let tabs_slot = w.find("local-aligned-tabs").bounds();
    assert!(
        tabs_slot.size.height - px(1.) - remote_tabs.size.height < px(1.),
        "the tabs slot must not add chrome beyond the taller row and its border"
    );
    let local_nav = w.find("local-navigation-toolbar").bounds();
    assert_eq!(local_nav.top(), tabs_slot.bottom());
    let remote_nav = w.find("remote-navigation-toolbar").bounds();
    assert_eq!(local_nav.top(), remote_nav.top());
    assert_eq!(local_nav.bottom(), remote_nav.bottom());
    assert_eq!(local_nav.size.height, metric(token::HEADER, cx));
    let local_header = w.find("local-browser-header").bounds();
    let remote_header = w.find("remote-browser-header").bounds();
    assert_eq!(local_header.top(), remote_header.top());
    assert_eq!(local_header.bottom(), remote_header.bottom());
    let local_input = w.find("filter").bounds();
    let remote_input = w.find("remote-filter").bounds();
    assert_eq!(local_input.top(), remote_input.top());
    assert_eq!(local_input.bottom(), remote_input.bottom());
    assert!(local_input.size.height > px(0.));
    for (header, input) in [(local_header, local_input), (remote_header, remote_input)] {
        assert!(
            (input.left() - header.left() - metric(space::PANEL, cx)).abs() <= px(1.),
            "input must fill pane with matching left padding: header={header:?}, input={input:?}"
        );
        assert!(
            (header.right() - input.right() - metric(space::PANEL, cx)).abs() <= px(1.),
            "input must fill pane with matching right padding: header={header:?}, input={input:?}"
        );
        assert!(input.top() >= header.top() && input.bottom() <= header.bottom());
    }
    for (nav_id, controls) in [
        ("local-navigation-toolbar", LOCAL_CONTROLS.as_slice()),
        ("remote-navigation-toolbar", REMOTE_NAV_CONTROLS.as_slice()),
    ] {
        let nav = w.find(nav_id).bounds();
        let top = w.within(nav_id).find(controls[0].0).bounds().top();
        let mut previous_right = None;
        for &(id, _) in controls {
            let bounds = w.within(nav_id).find(id).bounds();
            assert_eq!(bounds.top(), top, "{id} must stay in a single row");
            assert_eq!(
                bounds.size,
                size(metric(token::CONTROL, cx), metric(token::CONTROL, cx))
            );
            assert!(bounds.top() >= nav.top() && bounds.bottom() <= nav.bottom());
            if let Some(right) = previous_right {
                assert!(bounds.left() >= right, "overlapping navigation icon {id}");
            }
            previous_right = Some(bounds.right());
        }
    }
    // Root hoists these controls out of RemotePane; there must be no second input/header.
    for id in [
        "remote-navigation-toolbar",
        "remote-browser-header",
        "remote-filter",
    ] {
        let id = gpui_kit::ElementId::from(id);
        assert_eq!(
            snapshots(w)
                .iter()
                .filter(|s| s.path().last() == Some(&id))
                .count(),
            1
        );
    }
}

pub(super) fn assert_navigation_scroll(w: &mut Window, cx: &mut gpui_kit::App) {
    for (nav, first, last) in [
        ("local-navigation-toolbar", "back", "cancel"),
        ("remote-navigation-toolbar", "remote-back", "remote-cancel"),
    ] {
        w.scroll(
            nav,
            gpui_kit::ScrollDelta::Pixels(point(px(4000.), px(0.))),
            cx,
        );
        let viewport = w.find(nav).bounds();
        let before = w.within(nav).find(last).bounds();
        if before.right() > viewport.right() {
            w.scroll(
                nav,
                gpui_kit::ScrollDelta::Pixels(point(px(-4000.), px(0.))),
                cx,
            );
            let after = w.within(nav).find(last).bounds();
            assert!(
                after.left() < before.left(),
                "{nav} must scroll horizontally"
            );
            assert!(
                after.right() <= viewport.right() + px(1.),
                "last icon must be reachable in {nav}"
            );
            assert!(w.within(nav).find(first).bounds().left() < viewport.left());
            assert_eq!(
                w.find(nav).bounds(),
                viewport,
                "scroll must not change row height"
            );
            // Native button fills may extend beyond layout bounds, but their paint must be clipped.
            let last_bounds = after.scale(w.scale_factor());
            let viewport = viewport.scale(w.scale_factor());
            for quad in w.painted_quads().iter().filter(|q| q.bounds == last_bounds) {
                let painted = quad.bounds.intersect(&quad.content_mask.bounds);
                assert!(painted.left() >= viewport.left() && painted.right() <= viewport.right());
            }
            w.scroll(
                nav,
                gpui_kit::ScrollDelta::Pixels(point(px(4000.), px(0.))),
                cx,
            );
            assert!(w.within(nav).find(first).bounds().left() >= w.find(nav).bounds().left());
        }
    }
}

#[gpui_kit::test]
async fn measured_chrome_aligns_empty_and_long_host_catalogs_and_does_not_clip_native_popup(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    fs::write(local.path().join("local.txt"), "unchanged").unwrap();
    for host_count in [0, 12] {
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path().into());
        for index in 0..host_count {
            let mut host = server.host();
            host.name = format!("{index:02} {}", "long host name ".repeat(24));
            store.save_host(None, None, host).unwrap();
        }
        let f = Fixture::new(
            1400.,
            1200.,
            store,
            local.path(),
            Some(server.options()),
            cx,
        )
        .await;
        cx.update_window(f.handle, |_, w, cx| {
            let remote = f.shell.read(cx).remote.clone();
            let connection = remote.read(cx).connection();
            for (width, height, font) in [
                (1400., 1200., 16.),
                (640., 1400., 24.),
                (420., 1600., 24.),
                (1400., 1200., 16.),
            ] {
                w.resize(size(px(width), px(height)));
                w.bounds_changed(cx);
                Theme::update(cx, |theme| theme.font_size = px(font));
                for _ in 0..3 {
                    w.render_frame(cx);
                }
                assert_aligned_headers(w, cx);
                let picker = w.find("remote-host-select");
                assert_eq!(picker.role(), Some(gpui_kit::Role::ComboBox));
                assert_eq!(picker.label(), Some("Remote host"));
                if host_count == 0 {
                    assert_eq!(picker.value(), Some("No hosts configured"));
                    w.click("remote-host-select", cx);
                    assert_eq!(w.find("remote-host-select").expanded(), Some(false));
                }
                assert!(picker.bounds().right() <= w.find("remote-workspace").bounds().right());
                assert_eq!(remote.read(cx).connection(), connection);
                assert!(!remote.read(cx).has_session());
                assert!(!remote.read(cx).is_loading());
            }
            if host_count != 0 {
                w.click("remote-host-select", cx);
                w.render_frame(cx);
                let trigger = w
                    .find("remote-host-select")
                    .bounds()
                    .scale(w.scale_factor());
                let header = w
                    .find("remote-aligned-header")
                    .bounds()
                    .scale(w.scale_factor());
                assert_eq!(w.find("remote-host-select").expanded(), Some(true));
                assert!(
                    w.painted_quads().iter().any(|q| {
                        q.bounds.size.width >= trigger.size.width * 0.99
                            && q.bounds.size.width <= trigger.size.width * 1.01
                            && q.bounds.top() >= trigger.bottom() - trigger.size.height * 0.1
                            && q.bounds.bottom() > header.bottom()
                            && q.bounds.size.height > trigger.size.height * 2.
                            && q.content_mask.bounds.bottom() >= q.bounds.bottom()
                    }),
                    "the native popup must paint below, not be clipped to the measured chrome slot"
                );
                w.press("escape", cx);
            }
        })
        .unwrap();
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            assert_eq!(w.find("remote-host-select").expanded(), Some(false));
            assert!(!f.shell.read(cx).remote.read(cx).has_session());
        })
        .unwrap();
    }
    assert_eq!(
        fs::read_to_string(local.path().join("local.txt")).unwrap(),
        "unchanged"
    );
}

#[gpui_kit::test]
async fn paired_navigation_and_native_filters_align_across_splits_fonts_resize_and_preview(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let remote_file = server.dir.path().join("files/keep-remote.txt");
    fs::write(&remote_file, "remote unchanged").unwrap();
    fs::write(server.dir.path().join("files/other.txt"), "other remote").unwrap();
    fs::write(local.path().join("keep-local.txt"), "local unchanged").unwrap();
    fs::write(local.path().join("other.txt"), "other local").unwrap();
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let f = Fixture::new(
        1400.,
        1100.,
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
    cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
        f.shell.read(cx).remote.read(cx).has_session()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        let browser = f.shell.read(cx).browser.clone();
        let remote = f.shell.read(cx).remote.clone();
        browser.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("space", cx);
        remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("space", cx);
        assert_eq!(browser.read(cx).marked(), ["keep-local.txt"]);
        assert_eq!(remote.read(cx).marked(), [remote_file.to_string_lossy()]);
    })
    .unwrap();
    let original = f.shell.read_with(cx, |shell, cx| {
        (
            shell.browser.clone(),
            shell.remote.clone(),
            (shell.local_preview.clone(), shell.remote_preview.clone()),
            shell.browser.read(cx).id(),
            shell.remote.read(cx).session_id(),
            shell.remote.read(cx).connection(),
            shell.browser.read(cx).marked(),
            shell.remote.read(cx).marked(),
        )
    });
    let mut editor_ids = [None, None];
    for (local_preview, remote_preview) in [
        (false, false),
        (true, false),
        (false, true),
        (true, true),
        (false, false),
    ] {
        cx.update_window(f.handle, |_, w, cx| {
            w.resize(size(px(1400.), px(1600.)));
            w.bounds_changed(cx);
            Theme::update(cx, |theme| theme.font_size = px(16.));
            w.render_frame(cx);
            w.click(
                if local_preview {
                    "local-preview-tab"
                } else {
                    "local-files-tab"
                },
                cx,
            );
            w.click(
                if remote_preview {
                    "remote-preview-tab"
                } else {
                    "remote-files-tab"
                },
                cx,
            );
        })
        .unwrap();
        cx.wait_for(f.handle, Duration::from_secs(30), |_, cx| {
            !f.shell.read(cx).local_preview.read(cx).is_loading()
                && !f.shell.read(cx).remote_preview.read(cx).is_loading()
        })
        .await;
        cx.update_window(f.handle, |_, w, cx| {
            assert_eq!(
                f.shell
                    .read(cx)
                    .pane_view(crate::pane_tabs::PaneSide::Local),
                if local_preview {
                    crate::pane_tabs::PaneView::Preview
                } else {
                    crate::pane_tabs::PaneView::Files
                }
            );
            assert_eq!(
                f.shell
                    .read(cx)
                    .pane_view(crate::pane_tabs::PaneSide::Remote),
                if remote_preview {
                    crate::pane_tabs::PaneView::Preview
                } else {
                    crate::pane_tabs::PaneView::Files
                }
            );
            w.render_frame(cx);
            for (index, scope) in ["local-workspace", "remote-workspace"]
                .into_iter()
                .enumerate()
            {
                let scope = gpui_kit::ElementId::from(scope);
                if let Some(editor) = snapshots(w).into_iter().find(|s| {
                    s.path().contains(&scope)
                        && s.role() == Some(gpui_kit::Role::MultilineTextInput)
                }) {
                    let id = editor.path().last().unwrap().clone();
                    if let Some(original) = &editor_ids[index] {
                        assert_eq!(
                            original, &id,
                            "pane tab changes must retain the native editor entity"
                        );
                    } else {
                        editor_ids[index] = Some(id);
                    }
                }
            }
            for delta in [0., -360., 360.] {
                if remote_preview {
                    f.shell.read(cx).remote_focus.clone().focus(w, cx);
                } else {
                    f.shell.read(cx).local_focus.clone().focus(w, cx);
                }
                w.press("ctrl-alt-0", cx);
                let from = w
                    .within("browser-split")
                    .find("pane-divider")
                    .bounds()
                    .center();
                if delta != 0. {
                    w.drag(from, point(from.x + px(delta), from.y), cx);
                }
                let ratio = w
                    .within("browser-split")
                    .find("split-first")
                    .bounds()
                    .size
                    .width
                    / w.find("browser-split").bounds().size.width;
                if delta == 0. {
                    assert!((ratio - 0.5).abs() < 0.01);
                } else {
                    assert!((ratio - 0.5).abs() > 0.15, "exercise an unbalanced split");
                }
                for (width, height, font) in [
                    (1400., 1600., 16.),
                    (1400., 1600., 24.),
                    (640., 1200., 16.),
                    (420., 1600., 24.),
                    (1100., 900., 24.),
                    (1400., 1600., 16.),
                ] {
                    w.resize(size(px(width), px(height)));
                    w.bounds_changed(cx);
                    Theme::update(cx, |theme| theme.font_size = px(font));
                    for _ in 0..3 {
                        w.render_frame(cx);
                    }
                    assert_aligned_headers(w, cx);
                    assert_navigation_scroll(w, cx);
                    for (disabled, id) in
                        [(local_preview, "filter"), (remote_preview, "remote-filter")]
                    {
                        if disabled {
                            w.click(id, cx);
                            assert_ne!(w.find(id).focused(), Some(true));
                        }
                    }
                    if remote_preview {
                        let nav = w.find("remote-navigation-toolbar").bounds();
                        for (id, label) in REMOTE_NAV_CONTROLS {
                            let bounds = w.find(id).bounds();
                            if bounds.left() >= nav.left() && bounds.right() <= nav.right() {
                                w.click(id, cx);
                                assert_ne!(w.find(id).focused(), Some(true), "disabled {id}");
                                assert_eq!(w.find(id).label(), Some(label));
                                assert!(!f.shell.read(cx).remote.read(cx).is_loading());
                            }
                        }
                    }
                    if !remote_preview {
                        assert!(w.within("remote-workspace").find("remote-pane").visible());
                        assert!(w.within("remote-pane").try_find("remote-filter").is_none());
                    } else {
                        assert!(w.try_find("remote-pane").is_none());
                        assert!(
                            w.within("remote-workspace")
                                .find("preview-header")
                                .visible()
                        );
                    }
                    if local_preview {
                        assert!(w.within("local-workspace").find("preview-header").visible());
                        assert!(w.try_find("browser-pane").is_none());
                    } else {
                        assert!(w.within("local-workspace").find("browser-pane").visible());
                    }
                    assert_eq!(w.find("filter").value(), Some(""));
                    assert_eq!(w.find("remote-filter").value(), Some(""));
                    let shell = f.shell.read(cx);
                    assert_eq!(
                        (
                            shell.browser.clone(),
                            shell.remote.clone(),
                            (shell.local_preview.clone(), shell.remote_preview.clone())
                        ),
                        (original.0.clone(), original.1.clone(), original.2.clone())
                    );
                    assert_eq!(shell.browser.read(cx).id(), original.3);
                    assert_eq!(shell.remote.read(cx).session_id(), original.4);
                    assert_eq!(shell.remote.read(cx).connection(), original.5);
                    assert_eq!(shell.browser.read(cx).marked(), original.6);
                    assert_eq!(shell.remote.read(cx).marked(), original.7);
                    if width == 1400. {
                        let resized_ratio = w
                            .within("browser-split")
                            .find("split-first")
                            .bounds()
                            .size
                            .width
                            / w.find("browser-split").bounds().size.width;
                        assert!((resized_ratio - ratio).abs() < 0.01);
                    }
                }
            }
        })
        .unwrap();
    }
    let connection = f.shell.read_with(cx, |shell, cx| {
        shell.remote.read(cx).comparison_target().unwrap().1
    });
    connection.shutdown().await;
    assert_eq!(fs::read_to_string(remote_file).unwrap(), "remote unchanged");
    assert_eq!(
        fs::read_to_string(local.path().join("keep-local.txt")).unwrap(),
        "local unchanged"
    );
    assert!(!local.path().join("keep-remote.txt").exists());
    assert!(!server.dir.path().join("files/keep-local.txt").exists());
}
