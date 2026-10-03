use super::*;
use crate::sftp_test_support::ftp::Server;
use drift_app::{
    browser::{BrowserService, OperationId},
    remote::RemoteService,
};
use drift_core::{error::Error, store::Store, tlstrust::Manager};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Subscription, TestAppContext, WindowBounds, WindowOptions, size,
};
use std::sync::Arc;

struct PromptHarness {
    prompt: Entity<CertificatePrompt>,
    decisions: Vec<Decision>,
    _subscription: Subscription,
}
impl Render for PromptHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.prompt.clone())
    }
}

#[gpui_kit::test]
async fn certificate_keyboard_defaults_scroll_and_busy_approval(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let server = Server::tls("valid");
    let config = tempfile::tempdir().unwrap();
    let store = Store::new(config.path().into());
    let service = RemoteService::with_options(BrowserService::new().unwrap(), server.options())
        .with_trust(Arc::new(Manager::with_roots(
            store.clone(),
            rustls::RootCertStore::empty(),
        )));
    let operation = service.test_host(
        store,
        None,
        server.host(),
        None,
        OperationId {
            project: 1,
            operation: 1,
        },
    );
    let error = operation.task.await.unwrap().unwrap_err();
    let Error::Certificate { challenge, .. } = error else {
        panic!("expected certificate challenge")
    };
    let mut challenge = *challenge;
    challenge.subject = "Long certificate subject
"
    .repeat(80);
    let (handle, harness) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1000.), px(500.)),
                })),
                ..Default::default()
            },
            cx,
            |w, cx| {
                cx.new(|cx| {
                    let prompt = cx.new(|cx| CertificatePrompt::new(challenge, w, cx));
                    let subscription = cx.subscribe(
                        &prompt,
                        |this: &mut PromptHarness, _, decision: &Decision, cx| {
                            this.decisions.push(*decision);
                            cx.notify();
                        },
                    );
                    PromptHarness {
                        prompt,
                        decisions: vec![],
                        _subscription: subscription,
                    }
                })
            },
        )
        .unwrap()
    });
    let prompt = harness.read_with(cx, |h, _| h.prompt.clone());
    cx.update_window(handle, |_, w, cx| {
        for key in ["tab", "shift-tab", "left", "right"] {
            w.press(key, cx);
        }
        assert_eq!(prompt.read(cx).choice, 0);
        w.press("enter", cx);
        w.press("down", cx);
        assert!(prompt.read(cx).scroll.offset().y < px(0.));
        w.press("home", cx);
        assert_eq!(prompt.read(cx).scroll.offset().y, px(0.));
        w.press("pagedown", cx);
        let page = prompt.read(cx).scroll.offset().y;
        assert!(page < px(-25.));
        w.press("end", cx);
        assert!(prompt.read(cx).scroll.offset().y < page);
        w.press("pageup", cx);
        w.press("home", cx);
        w.press("up", cx);
        assert_eq!(prompt.read(cx).scroll.offset().y, px(0.));
        prompt.update(cx, |p, cx| {
            p.busy = true;
            cx.notify();
        });
        w.press("right", cx);
        w.press("enter", cx); // Busy approval produces no decision.
    })
    .unwrap();
    harness.read_with(cx, |h, _| {
        assert_eq!(h.decisions.len(), 1);
        assert!(matches!(h.decisions[0], Decision::Reject));
    });
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx); // Reject stays available while approval is pending.
        prompt.update(cx, |p, cx| {
            p.busy = false;
            cx.notify();
        });
        w.press("right", cx);
        w.press("right", cx);
        w.press("enter", cx);
    })
    .unwrap();
    harness.read_with(cx, |h, _| {
        assert_eq!(h.decisions.len(), 3);
        assert!(matches!(h.decisions[1], Decision::Reject));
        assert!(matches!(h.decisions[2], Decision::Permanent));
    });
    assert_eq!(server.commands("PASS"), 0);
}
