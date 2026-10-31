//! Shows a mail body in a window: `cargo run -p mailapp-mail-render --example view [file.html]`.
//! Without a file, shows the built-in sample newsletter. Prints the time to first paint to stderr.

use std::time::Instant;

use gpui::{App, AppContext as _, WindowOptions};
use mailapp_mail_render::{FirstPaint, MailBody};

fn main() {
    let html = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {path}: {error}")),
        None => include_str!("sample.html").to_owned(),
    };
    gpui_platform::application().run(move |cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_, cx| {
            // Measured from the mail being handed over, not from the window opening.
            let started = Instant::now();
            let body = cx.new(|cx| MailBody::new(html, cx));
            cx.subscribe(&body, move |_, _: &FirstPaint, _| {
                eprintln!("time to first paint: {} ms", started.elapsed().as_millis());
            })
            .detach();
            body
        })
        .expect("failed to open the window");
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}
