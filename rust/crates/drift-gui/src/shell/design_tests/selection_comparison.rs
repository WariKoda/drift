use super::*;
use gpui_kit::{App, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent};

#[gpui_kit::test]
async fn compare_and_copy_use_selection_then_marks_then_directory_real_sftp(
    cx: &mut TestAppContext,
) {
    let server = support::Server::new(false);
    let local = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(local.path().join(name), format!("local {name}")).unwrap();
        fs::write(
            server.dir.path().join("files").join(name),
            format!("remote {name}"),
        )
        .unwrap();
    }
    let store = Store::new(config.path().into());
    store.save_host(None, None, server.host()).unwrap();
    let f = Fixture::new(1400., 900., store, local.path(), Some(server.options()), cx).await;
    cx.update_window(f.handle, |_, w, cx| {
        w.click("remote-files-tab", cx);
        crate::test_support::connect_first(w, cx);
    })
    .unwrap();
    cx.wait_for(f.handle, Duration::from_secs(60), |_, cx| {
        f.shell.read(cx).remote.read(cx).has_session()
            && !f.shell.read(cx).remote.read(cx).is_loading()
    })
    .await;
    let session = f
        .shell
        .read_with(cx, |s, cx| s.remote.read(cx).comparison_target().unwrap().1);
    for remote in [false, true] {
        for mode in ["selection", "marks", "directory"] {
            cx.update_window(f.handle, |_, w, cx| {
                let list = if remote {
                    "remote-files"
                } else {
                    "local-files"
                };
                let click = |w: &mut Window, row, toggle, cx: &mut App| {
                    let position = w.within(list).find(row).bounds().center();
                    let modifiers = Modifiers {
                        control: toggle,
                        ..Default::default()
                    };
                    w.dispatch_event(
                        MouseDownEvent {
                            button: MouseButton::Left,
                            position,
                            modifiers,
                            click_count: 1,
                            first_mouse: false,
                        }
                        .to_platform_input(),
                        cx,
                    );
                    w.render_frame(cx);
                    w.dispatch_event(
                        MouseUpEvent {
                            button: MouseButton::Left,
                            position,
                            modifiers,
                            click_count: 1,
                        }
                        .to_platform_input(),
                        cx,
                    );
                    w.render_frame(cx);
                };
                click(w, 1usize, false, cx);
                w.press("escape", cx);
                if mode == "marks" {
                    w.press("space", cx);
                }
                if mode == "directory" {
                    click(w, 1usize, true, cx);
                } else {
                    click(w, 0usize, false, cx);
                    click(w, 2usize, true, cx);
                    click(w, 1usize, true, cx);
                    click(w, 1usize, true, cx);
                    let paths = if remote {
                        f.shell.read(cx).remote.read(cx).selection()
                    } else {
                        f.shell.read(cx).browser.read(cx).selection()
                    };
                    let expected: Vec<_> = ["a.txt", "c.txt"]
                        .iter()
                        .map(|name| {
                            if remote {
                                format!("{}/{name}", session.root.trim_end_matches('/'))
                            } else {
                                (*name).to_owned()
                            }
                        })
                        .collect();
                    assert_eq!(paths, expected);
                    w.press("ctrl-shift-c", cx);
                    assert_eq!(
                        cx.read_from_clipboard().unwrap().text(),
                        Some(expected.join("\n"))
                    );
                }
            })
            .unwrap();
            cx.update_window(f.handle, |_, w, cx| {
                if mode != "directory" {
                    assert_eq!(f.shell.read(cx).status, "Copied 2 paths");
                }
                w.click(
                    if remote {
                        "compare-remote"
                    } else {
                        "compare-local"
                    },
                    cx,
                );
            })
            .unwrap();
            cx.wait_for(f.handle, Duration::from_secs(60), |_, cx| {
                let pane = f.shell.read(cx).comparison.read(cx);
                pane.visible() && !pane.is_loading()
            })
            .await;
            cx.update_window(f.handle, |_, w, cx| {
                let comparison = f.shell.read(cx).comparison.read(cx);
                let result = comparison.session.as_ref().unwrap();
                let mut names: Vec<_> = result
                    .entries
                    .iter()
                    .map(|entry| {
                        entry
                            .local
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_owned()
                    })
                    .collect();
                names.sort();
                let expected = match mode {
                    "selection" => vec!["a.txt", "c.txt"],
                    "marks" => vec!["b.txt"],
                    "directory" => vec!["a.txt", "b.txt", "c.txt"],
                    _ => unreachable!(),
                };
                assert_eq!(names, expected, "remote={remote} mode={mode}");
                if remote {
                    assert!(result.request.local.is_empty());
                    assert_eq!(
                        result.request.remote.len(),
                        if mode == "selection" { 2 } else { 1 }
                    );
                } else {
                    assert!(result.request.remote.is_empty());
                    assert_eq!(
                        result.request.local.len(),
                        if mode == "selection" { 2 } else { 1 }
                    );
                }
                w.press("escape", cx);
            })
            .unwrap();
            assert!(
                !f.shell
                    .read_with(cx, |s, cx| s.comparison.read(cx).visible())
            );
        }
    }
    session.shutdown().await;
    for name in ["a.txt", "b.txt", "c.txt"] {
        assert_eq!(
            fs::read_to_string(local.path().join(name)).unwrap(),
            format!("local {name}")
        );
        assert_eq!(
            fs::read_to_string(server.dir.path().join("files").join(name)).unwrap(),
            format!("remote {name}")
        );
    }
}
