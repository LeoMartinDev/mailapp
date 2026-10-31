//! Shows a folder of emails one at a time and measures how Blitz renders each one (PLAN step 0.2):
//!
//! ```sh
//! cargo run --release -p mailapp-mail-render --example corpus -- <folder> [results.md]
//! ```
//!
//! The folder holds `.eml` and `.html` files; subfolders are ignored. A `corpus/` folder at the
//! root of the repository is git-ignored, so real emails put there are never committed. An `.eml`
//! shows its HTML part, with its `cid:` images inlined as data URLs, else its plain text part.
//!
//! Keys:
//! - ↓ and ↑, or j and k, or a click in the list: next and previous email. Coming back to an
//!   email measures it again.
//! - s: scrolls through the whole email at 3000 logical pixels a second, a brisk flick, and
//!   measures the frames.
//! - o: opens the same HTML in the default browser, through a file in the temp folder.
//!
//! Each measurement is printed to stdout, and the latest one of each email is written as a
//! Markdown table to `results.md`, by default `mailapp-corpus-results.md` in the temp folder:
//! - first paint: from handing the HTML over to the first screenful drawn. The first email also
//!   pays Blitz's one-time setup (system fonts, default stylesheet); come back to it for a warm
//!   number.
//! - parse: the time Blitz took to build the document from the HTML, on its own thread.
//! - layout: the time it took to style and lay the email out.
//! - bands: the time it took to paint each 256-pixel strip of the first screenful.
//! - height: the email's height in logical pixels.
//! - memory: the whole process's resident memory after the first paint (macOS and Linux).
//! - scroll: the number of frames, the median and slowest frame, the frames that showed a blank
//!   strip because it was still being painted, and the slowest strip painted while scrolling.
//!
//! To compare with the browser fairly: the browser loads remote images, stylesheets and fonts,
//! which mailapp blocks by design, so missing pictures and fallback fonts are expected. Judge the
//! layout: widths, columns, spacing, alignment, colors and text wrapping, at the same window
//! width. Note apart the differences that come from legacy HTML attributes (`<font>`,
//! `cellpadding`, `valign`…): M1.6 rewrites them into CSS before Blitz sees them. Measure with a
//! release build: a debug build is about ten times slower.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{env, fs, io, process};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, WindowOptions, div, px, rgb, white,
};
use mail_parser::{Message, MessageParser, PartKind};
use mailapp_mail_render::{FirstPaint, MailBody, Rendered};

/// In logical pixels a second.
const SCROLL_SPEED: f32 = 3000.;

const HEADER: &str = "\
| email | first paint ms | parse ms | layout ms | band median ms | band max ms | height px \
| memory MB | scroll frames | frame median ms | frame max ms | blank frames | scroll band max ms |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|";

fn main() {
    let mut args = env::args().skip(1);
    let Some(folder) = args.next() else {
        eprintln!("usage: corpus <folder> [results.md]");
        process::exit(2);
    };
    let results = args.next().map_or_else(
        || env::temp_dir().join("mailapp-corpus-results.md"),
        PathBuf::from,
    );
    let mut files: Vec<_> = fs::read_dir(&folder)
        .unwrap_or_else(|error| panic!("failed to read {folder}: {error}"))
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("eml") || ext.eq_ignore_ascii_case("html")
            })
        })
        .collect();
    files.sort();
    if files.is_empty() {
        eprintln!("no .eml or .html file in {folder}");
        process::exit(2);
    }
    println!("results go to {}\n{HEADER}", results.display());
    gpui_platform::application().run(move |cx: &mut App| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| Corpus::new(files, results, window, cx))
        })
        .expect("failed to open the window");
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}

struct Corpus {
    files: Vec<PathBuf>,
    /// The latest measurement of each file.
    rows: Vec<Row>,
    selected: usize,
    html: String,
    body: Entity<MailBody>,
    _subscriptions: [Subscription; 2],
    scroll: Option<Scroll>,
    results: PathBuf,
    focus: FocusHandle,
}

#[derive(Default)]
struct Row {
    first_paint: Option<Duration>,
    parse: Option<Duration>,
    layout: Option<Duration>,
    /// The first screenful's.
    bands: Vec<Duration>,
    height: f32,
    memory_mb: Option<usize>,
    scroll: Option<Scroll>,
}

#[derive(Default)]
struct Scroll {
    last_frame: Option<Instant>,
    frames: Vec<Duration>,
    blank_frames: usize,
    bands: Vec<Duration>,
}

impl Corpus {
    fn new(
        files: Vec<PathBuf>,
        results: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let html = load(&files[0]);
        let (body, subscriptions) = mail_body(&html, cx);
        Self {
            rows: files.iter().map(|_| Row::default()).collect(),
            files,
            selected: 0,
            html,
            body,
            _subscriptions: subscriptions,
            scroll: None,
            results,
            focus,
        }
    }

    fn show(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = index;
        self.rows[index] = Row::default();
        self.scroll = None;
        self.html = load(&self.files[index]);
        (self.body, self._subscriptions) = mail_body(&self.html, cx);
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.files.len();
        match event.keystroke.key.as_str() {
            "down" | "j" => self.show((self.selected + 1) % count, cx),
            "up" | "k" => self.show((self.selected + count - 1) % count, cx),
            "s" => self.start_scroll(window, cx),
            "o" => match browser_file(&self.files[self.selected], &self.html) {
                Ok(path) => {
                    println!("opening {}", path.display());
                    cx.open_with_system(&path);
                }
                Err(error) => eprintln!("failed to write the browser file: {error}"),
            },
            _ => {}
        }
    }

    fn rendered(&mut self, event: &Rendered) {
        let row = &mut self.rows[self.selected];
        match (event, &mut self.scroll) {
            (Rendered::Parse { took }, _) => row.parse = Some(*took),
            (Rendered::Layout { height, took }, _) if row.first_paint.is_none() => {
                row.layout = Some(*took);
                row.height = f32::from(*height);
            }
            (Rendered::Band { took }, _) if row.first_paint.is_none() => row.bands.push(*took),
            (Rendered::Band { took }, Some(scroll)) => scroll.bands.push(*took),
            _ => {}
        }
    }

    fn first_paint(&mut self, took: Duration) {
        let row = &mut self.rows[self.selected];
        row.first_paint = Some(took);
        row.memory_mb = resident_memory_mb();
        self.record();
    }

    fn start_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row = &self.rows[self.selected];
        if self.scroll.is_some() || row.first_paint.is_none() {
            return;
        }
        let height = px(row.height);
        self.body.update(cx, |body, cx| body.scroll_by(-height, cx));
        self.scroll = Some(Scroll::default());
        cx.on_next_frame(window, Self::scroll_frame);
    }

    /// Runs after each frame of a scroll run: measures it, then scrolls for the next one.
    fn scroll_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let blank = self.body.read(cx).missing_bands() > 0;
        // None once another email is shown.
        let Some(scroll) = &mut self.scroll else {
            return;
        };
        let now = Instant::now();
        let frame = match scroll.last_frame {
            Some(last) => {
                scroll.frames.push(now - last);
                scroll.blank_frames += usize::from(blank);
                now - last
            }
            // Starts once the top of the email is drawn.
            None if blank => return cx.on_next_frame(window, Self::scroll_frame),
            None => Duration::from_secs(1) / 60,
        };
        scroll.last_frame = Some(now);
        let step = px(SCROLL_SPEED * frame.as_secs_f32());
        if self.body.update(cx, |body, cx| body.scroll_by(step, cx)) {
            cx.on_next_frame(window, Self::scroll_frame);
        } else {
            self.rows[self.selected].scroll = self.scroll.take();
            self.record();
        }
    }

    /// Prints the selected email's row and rewrites the results file.
    fn record(&self) {
        println!("{}", self.line(self.selected));
        let mut table = format!("{HEADER}\n");
        for index in (0..self.files.len()).filter(|&index| self.rows[index].first_paint.is_some()) {
            table += &self.line(index);
            table.push('\n');
        }
        if let Err(error) = fs::write(&self.results, table) {
            eprintln!("failed to write {}: {error}", self.results.display());
        }
    }

    fn line(&self, index: usize) -> String {
        let row = &self.rows[index];
        let name = self.files[index]
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let (band_median, band_max) = median_max(&row.bands);
        let scroll = row.scroll.as_ref();
        let (frame_median, frame_max) =
            scroll.map_or((None, None), |scroll| median_max(&scroll.frames));
        let count = |count: Option<usize>| count.map_or(String::new(), |count| count.to_string());
        format!(
            "| {name} | {} | {} | {} | {} | {} | {:.0} | {} | {} | {} | {} | {} | {} |",
            ms(row.first_paint),
            ms(row.parse),
            ms(row.layout),
            ms(band_median),
            ms(band_max),
            row.height,
            count(row.memory_mb),
            count(scroll.map(|scroll| scroll.frames.len())),
            ms(frame_median),
            ms(frame_max),
            count(scroll.map(|scroll| scroll.blank_frames)),
            ms(scroll.and_then(|scroll| scroll.bands.iter().max().copied())),
        )
    }
}

impl Render for Corpus {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let files = self.files.iter().enumerate().map(|(index, path)| {
            div()
                .id(index)
                .px_2()
                .py_1()
                .when(index == self.selected, |file| file.bg(rgb(0xdbe4f5)))
                .child(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                )
                .on_click(cx.listener(move |corpus, _, _, cx| corpus.show(index, cx)))
        });
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .size_full()
            .flex()
            .bg(white())
            .text_sm()
            .child(
                div()
                    .id("files")
                    .w(px(240.))
                    .h_full()
                    .flex_none()
                    .overflow_y_scroll()
                    .border_r_1()
                    .border_color(rgb(0xcccccc))
                    .child(
                        div()
                            .p_2()
                            .text_color(rgb(0x777777))
                            .child("↓↑ next  s scroll  o browser"),
                    )
                    .children(files),
            )
            .child(div().flex_1().h_full().child(self.body.clone()))
    }
}

fn mail_body(html: &str, cx: &mut Context<Corpus>) -> (Entity<MailBody>, [Subscription; 2]) {
    let started = Instant::now();
    let body = cx.new(|cx| MailBody::new(html.to_owned(), cx));
    let subscriptions = [
        cx.subscribe(&body, move |corpus, _, _: &FirstPaint, _| {
            corpus.first_paint(started.elapsed())
        }),
        cx.subscribe(&body, |corpus, _, event: &Rendered, _| {
            corpus.rendered(event)
        }),
    ];
    (body, subscriptions)
}

/// The HTML to show for a file: an `.html` file as it is; for an `.eml`, its HTML part with its
/// `cid:` images inlined, else its plain text part.
fn load(path: &Path) -> String {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return preformatted(&format!("failed to read {}: {error}", path.display())),
    };
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("eml"))
    {
        return String::from_utf8_lossy(&bytes).into_owned();
    }
    let Some(message) = MessageParser::default().parse(&bytes) else {
        return preformatted("not a MIME message");
    };
    // mail-parser converts a plain text part to HTML when there is no HTML part; show it as text.
    let html: String = message
        .html_body()
        .filter(|part| matches!(part.kind(), PartKind::Html))
        .filter_map(|part| part.text())
        .collect();
    if html.is_empty() {
        return preformatted(&message.body_text(0).unwrap_or_default());
    }
    inline_images(&message, html)
}

/// Replaces each `cid:` reference with a data URL of the part it names.
fn inline_images(message: &Message, mut html: String) -> String {
    let mut parts: Vec<_> = message
        .parts()
        .filter_map(|part| Some((part.content_id()?, part)))
        .collect();
    // Longest first, so that `cid:a` does not rewrite the start of `cid:ab`.
    parts.sort_by_key(|(id, _)| Reverse(id.len()));
    for (id, part) in parts {
        let mime = part
            .content_type()
            .map_or("application/octet-stream".into(), |ty| {
                format!("{}/{}", ty.ctype(), ty.subtype().unwrap_or_default())
            });
        let data = BASE64_STANDARD.encode(part.decoded());
        html = html.replace(&format!("cid:{id}"), &format!("data:{mime};base64,{data}"));
    }
    html
}

fn preformatted(text: &str) -> String {
    let text = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(r#"<pre style="white-space: pre-wrap; margin: 16px">{text}</pre>"#)
}

/// Writes the HTML next to the other temp files, for the browser.
fn browser_file(email: &Path, html: &str) -> io::Result<PathBuf> {
    let name = email.file_stem().unwrap_or_default().to_string_lossy();
    let path = env::temp_dir().join(format!("mailapp-corpus-{name}.html"));
    // The text is UTF-8 now, whatever charset the email's own <meta> declares; a BOM wins over it.
    fs::write(&path, format!("\u{feff}{html}"))?;
    Ok(path)
}

// shortcut: `ps` exists on macOS and Linux only, so Windows shows no memory; use a crate such as
// memory-stats if the corpus is ever measured on Windows.
fn resident_memory_mb() -> Option<usize> {
    let pid = process::id().to_string();
    let output = process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    let kilobytes: usize = String::from_utf8(output.stdout).ok()?.trim().parse().ok()?;
    Some(kilobytes / 1024)
}

fn median_max(durations: &[Duration]) -> (Option<Duration>, Option<Duration>) {
    let mut sorted = durations.to_vec();
    sorted.sort();
    (
        sorted.get(sorted.len() / 2).copied(),
        sorted.last().copied(),
    )
}

fn ms(duration: Option<Duration>) -> String {
    duration.map_or(String::new(), |duration| {
        format!("{:.1}", duration.as_secs_f64() * 1000.)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples/corpus/fixtures")
                .join(name),
        )
    }

    #[test]
    fn inlines_cid_images() {
        let html = fixture("newsletter.eml");
        assert!(html.contains(r#"<img src="data:image/png;base64,iVBORw0KGgo"#));
        assert!(!html.contains("cid:"));
        // The remote image is left as it is, for Blitz not to fetch it.
        assert!(html.contains(r#"<img src="https://example.com/banner.jpg""#));
    }

    #[test]
    fn shows_plain_text_as_preformatted_text() {
        let html = fixture("plain.eml");
        assert!(html.starts_with("<pre"));
        assert!(html.contains("In the config, &lt;port&gt; &amp; &lt;host&gt; are placeholders."));
        assert!(html.contains("Café on Monday?"));
    }

    #[test]
    fn writes_the_browser_file() {
        let path = browser_file(Path::new("browser-file-test.eml"), "<p>Café</p>").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "\u{feff}<p>Café</p>");
        fs::remove_file(path).unwrap();
    }
}
