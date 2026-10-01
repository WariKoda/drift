mod shell;

use gpui_kit::{AppContext, Bounds, QuitMode, WindowBounds, WindowOptions, px, size};
use shell::Shell;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx| {
            gpui_kit::init(cx);
            shell::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
            if let Err(error) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Shell::new(window, cx)),
            ) {
                // Startup has not entered a TUI or opened a logging session.
                eprintln!("drift-gui: cannot open window: {error}");
                cx.quit();
            }
        });
}
