//! The Blitz side. A Blitz document is not `Send`, so each one lives on its own thread, which lays
//! it out and rasterizes bands of it when `MailBody` asks.

use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyrender::render_to_buffer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{DocumentConfig, Point};
use blitz_html::HtmlDocument;
use blitz_paint::paint_scene;
use blitz_traits::net::{NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use data_url::DataUrl;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::RenderImage;
use image::{Frame, RgbaImage};

/// Height of a band in CSS pixels, which are GPUI's logical pixels.
pub(crate) const BAND_HEIGHT: f32 = 256.;

pub(crate) enum Command {
    /// Lays the document out in a viewport of this size, in device pixels. Each layout has a
    /// new `generation`, which the answers carry.
    Layout {
        generation: u64,
        width: u32,
        height: u32,
        scale: f32,
    },
    Paint {
        generation: u64,
        band: usize,
    },
}

/// Each event carries the time its work took on this thread.
pub(crate) enum Event {
    /// The HTML is parsed, once, before any command is handled.
    Parsed { took: Duration },
    /// The document's height in CSS pixels.
    Layout {
        generation: u64,
        height: f32,
        took: Duration,
    },
    Band {
        generation: u64,
        band: usize,
        image: Arc<RenderImage>,
        took: Duration,
    },
}

/// Starts the thread; it stops when the command sender is dropped.
pub(crate) fn spawn(html: String) -> (mpsc::Sender<Command>, UnboundedReceiver<Event>) {
    let (commands, command_receiver) = mpsc::channel();
    let (event_sender, events) = unbounded();
    thread::Builder::new()
        .name("mail-render".into())
        .spawn(move || run(&html, command_receiver, event_sender))
        .expect("failed to start the mail render thread");
    (commands, events)
}

fn run(html: &str, commands: mpsc::Receiver<Command>, events: UnboundedSender<Event>) {
    let started = Instant::now();
    let mut document = document(html);
    let parsed = Event::Parsed {
        took: started.elapsed(),
    };
    if events.unbounded_send(parsed).is_err() {
        return;
    }
    while let Ok(command) = commands.recv() {
        let mut batch: Vec<_> = [command].into_iter().chain(commands.try_iter()).collect();
        // Commands sent before the latest layout are stale, e.g. while the window is resized.
        let latest = batch
            .iter()
            .rposition(|command| matches!(command, Command::Layout { .. }))
            .unwrap_or(0);
        for command in batch.drain(latest..) {
            let started = Instant::now();
            let event = match command {
                Command::Layout {
                    generation,
                    width,
                    height,
                    scale,
                } => {
                    document.set_viewport(Viewport::new(width, height, scale, ColorScheme::Light));
                    document.resolve(0.);
                    Event::Layout {
                        generation,
                        height: document.root_element().scroll_height(),
                        took: started.elapsed(),
                    }
                }
                // Fields are evaluated in order, so `took` includes the painting.
                Command::Paint { generation, band } => Event::Band {
                    generation,
                    band,
                    image: paint_band(&mut document, band),
                    took: started.elapsed(),
                },
            };
            if events.unbounded_send(event).is_err() {
                return;
            }
        }
    }
}

fn document(html: &str) -> HtmlDocument {
    let config = DocumentConfig {
        net_provider: Some(Arc::new(DataUrls)),
        ..DocumentConfig::default()
    };
    HtmlDocument::from_html(html, config)
}

/// Serves `data:` URLs, which is how inline (`cid:`) images reach the document, and nothing else:
/// remote images, stylesheets and fonts are never fetched.
struct DataUrls;

impl NetProvider for DataUrls {
    fn fetch(&self, _: usize, request: Request, handler: Box<dyn NetHandler>) {
        if let Ok(url) = DataUrl::process(request.url.as_str())
            && let Ok((bytes, _)) = url.decode_to_vec()
        {
            handler.bytes(request.url.into(), bytes.into());
        }
    }

    // Blitz holds rendering while a stylesheet from the `<head>` is pending, unless the provider
    // is a no-op. Ours answers at once or never, so nothing is worth waiting for.
    fn is_noop(&self) -> bool {
        true
    }
}

fn paint_band(document: &mut HtmlDocument, band: usize) -> Arc<RenderImage> {
    let viewport = document.viewport();
    let (width, scale) = (viewport.window_size.0, viewport.hidpi_scale);
    let height = (BAND_HEIGHT * scale).ceil() as u32;
    let top = band as f32 * BAND_HEIGHT;
    document.set_viewport_scroll(Point {
        x: 0.,
        y: top.into(),
    });
    let mut pixels = render_to_buffer::<VelloCpuImageRenderer, _>(
        |scene| paint_scene(scene, document, scale.into(), width, height, 0, 0),
        width,
        height,
    );
    // Vello gives premultiplied RGBA, transparent where the mail has no background; GPUI wants
    // BGRA. Mail is designed for a white page, so compose over white.
    for [r, g, b, a] in pixels.as_chunks_mut::<4>().0 {
        let white = 255 - *a;
        (*r, *g, *b, *a) = (
            b.saturating_add(white),
            g.saturating_add(white),
            r.saturating_add(white),
            255,
        );
    }
    let image = RgbaImage::from_raw(width, height, pixels).expect("the buffer is width × height");
    Arc::new(RenderImage::new([Frame::new(image)]))
}

#[cfg(test)]
mod tests {
    use futures::{StreamExt as _, executor::block_on};

    use super::*;

    const SAMPLE: &str = include_str!("../examples/sample.html");

    type Worker = (mpsc::Sender<Command>, UnboundedReceiver<Event>);

    /// Lays out at `width` CSS pixels and returns the document height.
    fn layout(worker: &mut Worker, generation: u64, width: f32, scale: f32) -> f32 {
        let (width, height) = ((width * scale) as u32, (600. * scale) as u32);
        let command = Command::Layout {
            generation,
            width,
            height,
            scale,
        };
        worker.0.send(command).unwrap();
        match block_on(worker.1.next()) {
            Some(Event::Layout { height, .. }) => height,
            _ => panic!("expected a layout"),
        }
    }

    fn first_band(worker: &mut Worker, generation: u64) -> Arc<RenderImage> {
        let command = Command::Paint {
            generation,
            band: 0,
        };
        worker.0.send(command).unwrap();
        match block_on(worker.1.next()) {
            Some(Event::Band { image, .. }) => image,
            _ => panic!("expected a band"),
        }
    }

    /// Spawns a worker, which reports the parsing first.
    fn start(html: &str) -> Worker {
        let mut worker = spawn(html.into());
        assert!(matches!(
            block_on(worker.1.next()),
            Some(Event::Parsed { .. })
        ));
        worker
    }

    fn has_ink(image: &RenderImage) -> bool {
        let pixels = image.as_bytes(0).unwrap();
        pixels.chunks(4).any(|pixel| pixel != [255; 4])
    }

    #[test]
    fn renders_the_sample() {
        let mut worker = start(SAMPLE);
        let height = layout(&mut worker, 1, 800., 1.);
        assert!((500. ..5000.).contains(&height), "height: {height}");
        let band = first_band(&mut worker, 1);
        assert_eq!(band.size(0), gpui::size(800.into(), 256.into()));
        assert!(has_ink(&band));

        // A narrower body wraps the text onto more lines.
        assert!(layout(&mut worker, 2, 400., 1.) > height);

        // At 2× the layout is the same, give or take rounding, with twice the pixels each way.
        assert!((layout(&mut worker, 3, 800., 2.) - height).abs() <= 1.);
        let band = first_band(&mut worker, 3);
        assert_eq!(band.size(0), gpui::size(1600.into(), 512.into()));
    }

    #[test]
    fn never_fetches_remote_resources() {
        struct Unreachable;
        impl NetHandler for Unreachable {
            fn bytes(self: Box<Self>, url: String, _: blitz_traits::net::Bytes) {
                panic!("fetched {url}");
            }
        }
        let url = "https://example.com/pixel.png".parse().unwrap();
        DataUrls.fetch(0, Request::get(url), Box::new(Unreachable));

        // Nor does it wait for them before painting.
        let html = r#"<link rel="stylesheet" href="https://example.com/style.css">
            <img src="https://example.com/pixel.png" width="1" height="1"><p>Hello</p>"#;
        let mut worker = start(html);
        layout(&mut worker, 1, 400., 1.);
        assert!(has_ink(&first_band(&mut worker, 1)));
    }

    #[test]
    fn draws_data_url_images() {
        // A 1×1 red PNG, stretched over the top-left corner.
        let html = r#"<body style="margin: 0"><img width="40" height="40"
            src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR42mP4z8AAAAMBAQD3A0FDAAAAAElFTkSuQmCC">"#;
        let mut worker = start(html);
        layout(&mut worker, 1, 400., 1.);
        let band = first_band(&mut worker, 1);
        let at = (20 * 400 + 20) * 4;
        // BGRA.
        assert_eq!(band.as_bytes(0).unwrap()[at..at + 4], [0, 0, 255, 255]);
    }
}
