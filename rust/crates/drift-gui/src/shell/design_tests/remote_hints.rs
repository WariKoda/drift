use super::*;
use crate::design::{Text, space};
use drift_core::pathmap::Mapping;
use gpui_kit::component::ActiveTheme;
use std::os::unix::fs::symlink;

#[gpui_kit::test]
async fn narrow_unicode_unmapped_symlinks_keep_hints_outside_filename_and_deep_indent(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let remote_root = server.dir.path().join("files");
    let long_name = format!("zz-{}.txt", "日本-e\u{301}-👩‍💻-".repeat(7));
    let mapped = remote_root.join("a-mapped");
    let mut deep = remote_root.join("z-unmapped");
    for level in 0..8 {
        deep.push(format!("n{level:02}"));
    }
    fs::create_dir_all(&deep).unwrap();
    fs::create_dir(&mapped).unwrap();
    fs::write(mapped.join("a.txt"), "remote unchanged").unwrap();
    symlink("a.txt", mapped.join(&long_name)).unwrap();
    symlink("a-mapped/a.txt", remote_root.join(&long_name)).unwrap();
    symlink(mapped.join("a.txt"), deep.join(&long_name)).unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::create_dir(local.path().join("src")).unwrap();
    fs::write(local.path().join("src/a.txt"), "local unchanged").unwrap();
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let mut host = server.host();
    host.mappings = vec![Mapping {
        local: "src".into(),
        remote: "a-mapped".into(),
    }];
    store.save_host(None, None, host).unwrap();
    let f = Fixture::new(
        1400.,
        1800.,
        store,
        local.path(),
        Some(server.options()),
        cx,
    )
    .await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote", cx);
        w.click(("connect-host", 0usize), cx);
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

    // Use the production disclosure callbacks and real asynchronous SFTP listings.
    // The mapped directory has two children; the unmapped chain has eight levels.
    for index in std::iter::once(0usize).chain(3..=11) {
        cx.update_window(f.handle, |_, w, cx| {
            w.render_frame(cx);
            w.click(("remote-tree-toggle", index), cx);
        })
        .unwrap();
        cx.wait_for(f.handle, Duration::from_secs(20), |_, cx| {
            !f.shell.read(cx).remote.read(cx).is_loading()
        })
        .await;
    }
    cx.update_window(f.handle, |_, w, cx| {
        w.render_frame(cx);
        let remote = f.shell.read(cx).remote.clone();
        assert_eq!(
            (0usize..32)
                .filter(|index| w.within("remote-files-pane").try_find(*index).is_some())
                .count(),
            14
        );
        let toggle = |index| w.find(("remote-tree-toggle", index)).bounds();
        assert_eq!(
            toggle(12usize).left() - toggle(13usize).left(),
            px(9. * 16.)
        );
        remote.focus_handle(cx).focus(w, cx);
        w.press("home", cx);
        w.press("down", cx);
        assert_eq!(remote.read(cx).selected(), mapped.join("a.txt").to_str());
        w.press("space", cx);
        let marks = remote.read(cx).marked();
        assert_eq!(marks, [mapped.join("a.txt").to_string_lossy()]);
        w.press("end", cx);
        assert_eq!(
            remote.read(cx).selected(),
            remote_root.join(&long_name).to_str()
        );
        w.press("space", cx);
        assert_eq!(
            remote.read(cx).marked(),
            marks,
            "unmapped symlinks cannot be marked"
        );
        w.press("up", cx);
        assert_eq!(remote.read(cx).selected(), deep.join(&long_name).to_str());
        w.press("space", cx);
        assert_eq!(remote.read(cx).marked(), marks);
        let session = remote.read(cx).session_id();
        let connection = remote.read(cx).connection();
        let browser_id = f.shell.read(cx).browser.read(cx).id();
        let selected = remote.read(cx).selected().unwrap().to_owned();
        let focus = w.focused(cx);
        w.render_frame(cx);
        assert!(w.find(("remote-row-symlink", 2usize)).visible(), "mapped symlink retains its arrow");
        assert!(w.try_find(("remote-row-unmapped", 2usize)).is_none(), "mapped symlink must not gain an unmapped warning");
        assert!(w.try_find(("remote-row-symlink", 1usize)).is_none(), "regular files must not gain a symlink arrow");
        assert!(w.try_find(("remote-row-unmapped", 1usize)).is_none());

        for (width, font) in [(840., 16.), (420., 16.), (840., 24.), (420., 24.)] {
            w.resize(size(px(width), px(1800.)));
            w.bounds_changed(cx);
            Theme::update(cx, |theme| theme.font_size = px(font));
            w.render_frame(cx);
            w.scroll("remote-files-pane", gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-2000.))), cx);
            w.render_frame(cx);
            let pane = w.find("remote-files-pane").bounds();
            assert!(
                pane.size.width <= px(width / 2. + 1.),
                "exercise a narrow split pane: {pane:?}"
            );
            let pad = metric(space::PANEL, cx);
            let gap = metric(space::TIGHT, cx);
            // Compare actual layout widths against GPUI's native shaper, not an
            // accessibility label (which would not prove that visible text fits).
            let shaped_width = |text: &str, text_size: Text| {
                let mut style = w.text_style();
                style.font_family = cx.theme().font_family.clone();
                w.text_system()
                    .shape_line(
                        text.to_owned().into(),
                        text_size.rems().to_pixels(w.rem_size()),
                        &[style.to_run(text.len())],
                        None,
                    )
                    .width
            };
            let arrow_width = shaped_width("→", Text::Body);
            let badge_width = shaped_width("Unmapped", Text::Metadata) + 2. * gap;
            let filename_width = shaped_width(&long_name, Text::Body);
            eprintln!("remote hints: window={width}, pane={:?}, font={font}, arrow={arrow_width:?}, Unmapped={badge_width:?}", pane.size.width);
            let mut hint_rights = vec![];
            for index in [12usize, 13usize] {
                let row = w.within("remote-files-pane").find(index).bounds();
                let group = w.find(("remote-row-name-group", index)).bounds();
                let name = w.find(("remote-row-name", index)).bounds();
                let arrow = w.find(("remote-row-symlink", index));
                let badge = w.find(("remote-row-unmapped", index));
                assert_eq!(row.size.height, metric(token::ROW, cx));
                assert!(group.left() >= row.left() + pad - px(0.1));
                assert!(group.right() + gap <= arrow.bounds().left() + px(0.1));
                assert!(
                    name.size.width < filename_width,
                    "the filename must actually be constrained: row {index}, width {width}, font {font}, name {name:?}, group {group:?}, intrinsic {filename_width:?}"
                );
                for (hint, intrinsic) in [(&arrow, arrow_width), (&badge, badge_width)] {
                    let bounds = hint.bounds();
                    assert!(hint.visible());
                    assert!(
                        (bounds.size.width - intrinsic).abs() < px(1.),
                        "untruncated native text width: {bounds:?} vs {intrinsic:?}"
                    );
                    assert!(bounds.left() >= row.left() + pad);
                    assert!(bounds.right() <= row.right() - pad + px(0.1));
                    assert!(bounds.top() >= row.top() && bounds.bottom() <= row.bottom());
                    assert!(bounds.left() >= pane.left() && bounds.right() <= pane.right());
                    assert!(bounds.top() >= pane.top() && bounds.bottom() <= pane.bottom());
                    assert!(bounds.right() <= px(width));
                }
                assert!(arrow.bounds().right() + gap <= badge.bounds().left() + px(0.1));
                hint_rights.push((arrow.bounds().right(), badge.bounds().right()));
                let badge_bounds = badge.bounds().scale(w.scale_factor());
                let quad = w
                    .painted_quads()
                    .into_iter()
                    .find(|quad| {
                        quad.bounds == badge_bounds
                            && quad.background == Palette::current(cx).chrome.into()
                    })
                    .expect("the metadata badge must be painted, not merely observed");
                let mask = quad.content_mask.bounds;
                assert!(
                    mask.left() <= badge_bounds.left()
                        && mask.right() >= badge_bounds.right()
                        && mask.top() <= badge_bounds.top()
                        && mask.bottom() >= badge_bounds.bottom(),
                    "badge must not be clipped by an ancestor"
                );
            }
            assert_eq!(
                hint_rights[0], hint_rights[1],
                "depth cannot shift trailing warnings"
            );
            assert_eq!(remote.read(cx).session_id(), session);
            assert_eq!(remote.read(cx).connection(), connection);
            assert_eq!(remote.read(cx).selected(), Some(selected.as_str()));
            assert_eq!(remote.read(cx).marked(), marks);
            assert_eq!(f.shell.read(cx).browser.read(cx).id(), browser_id);
            assert_eq!(w.focused(cx), focus);
            assert!(!remote.read(cx).is_loading());
            assert!(!f.shell.read(cx).comparison.read(cx).visible());
            assert!(!f.shell.read(cx).preview.read(cx).is_loading());
        }
        // Measure in the completed, visible layout. The tiny shell's file pane
        // is not visible in macOS headless CI, so wheel dispatch or querying its
        // virtualized rows cannot establish this unaccepted layout bound.
        let hint_rem = w.rem_size();
        let arrow = w.find(("remote-row-symlink", 13usize)).bounds();
        let badge = w.find(("remote-row-unmapped", 13usize)).bounds();
        let required = 2. * metric(space::PANEL, cx) + 2. * metric(space::TIGHT, cx)
            + arrow.size.width + badge.size.width;
        w.resize(size(px(240.), px(1800.)));
        w.bounds_changed(cx);
        w.render_frame(cx);
        assert_eq!(w.rem_size(), hint_rem, "resize must not change hint typography");
        let tiny_files = w.find("remote-files-pane");
        let tiny_pane = tiny_files.bounds();
        assert!((tiny_pane.size.width - px(120.)).abs() < px(1.));
        assert!(required > tiny_pane.size.width);
        eprintln!("remote hints limit: pane={:?}, visible={}, font=24, required={required:?}, arrow={arrow:?}, badge={badge:?}", tiny_pane, tiny_files.visible());
        assert_eq!(remote.read(cx).session_id(), session);
        assert_eq!(remote.read(cx).marked(), marks);
        assert_eq!(remote.read(cx).selected(), Some(selected.as_str()));
        assert_eq!(w.focused(cx), focus);

        // Collapsing a real parent still preserves marks and the live session.
        w.resize(size(px(1400.), px(1800.)));
        w.bounds_changed(cx);
        w.render_frame(cx);
        w.click(("remote-tree-toggle", 3usize), cx);
        w.render_frame(cx);
        assert_eq!(
            (0usize..32)
                .filter(|index| w.within("remote-files-pane").try_find(*index).is_some())
                .count(),
            5
        );
        assert_eq!(remote.read(cx).marked(), marks);
        assert_eq!(remote.read(cx).session_id(), session);
    })
    .unwrap();
    assert_eq!(
        fs::read(local.path().join("src/a.txt")).unwrap(),
        b"local unchanged"
    );
    assert_eq!(fs::read(mapped.join("a.txt")).unwrap(), b"remote unchanged");
    assert_eq!(
        fs::read_link(mapped.join(&long_name)).unwrap(),
        Path::new("a.txt")
    );
    assert_eq!(
        fs::read_link(remote_root.join(&long_name)).unwrap(),
        Path::new("a-mapped/a.txt")
    );
    assert_eq!(
        fs::read_link(deep.join(&long_name)).unwrap(),
        mapped.join("a.txt")
    );
    assert!(!local.path().join(&long_name).exists());
}
