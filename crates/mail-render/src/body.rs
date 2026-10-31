//! The GPUI side. `MailBody` asks the Blitz thread for a layout at its own size, then for the bands
//! it is about to show, and draws and scrolls them.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use futures::StreamExt as _;
use gpui::{
    Bounds, Context, Corners, DispatchPhase, EventEmitter, HitboxBehavior, IntoElement, Pixels,
    Render, RenderImage, ScrollEvent, Size, Styled as _, Task, Window, canvas, fill, point, px,
    size, white,
};

use crate::worker::{self, BAND_HEIGHT, Command, Event};

/// Memory budget for the bands kept ready to draw; GPUI holds a GPU copy of those it draws.
const CACHE_BYTES: f32 = 64. * 1024. * 1024.;

/// Emitted once, when the first screenful of the mail is drawn.
pub struct FirstPaint;

/// Emitted when the Blitz thread finishes parsing, a layout or a band, with the time it took there.
pub enum Rendered {
    Parse { took: Duration },
    Layout { height: Pixels, took: Duration },
    Band { took: Duration },
}

/// A mail body rendered by Blitz, filling its parent and scrolling on its own.
pub struct MailBody {
    worker: mpsc::Sender<Command>,
    _events: Task<()>,
    /// Size and scale factor of the latest layout requested, numbered by `generation`.
    viewport: Option<(Size<Pixels>, f32)>,
    generation: u64,
    /// Document height, once the latest layout is done.
    height: Option<Pixels>,
    bands: HashMap<usize, Arc<RenderImage>>,
    requested: HashSet<usize>,
    scroll_top: Pixels,
    max_scroll_top: Pixels,
    missing: usize,
    painted: bool,
}

impl MailBody {
    pub fn new(html: String, cx: &mut Context<Self>) -> Self {
        let (worker, mut events) = worker::spawn(html);
        let events = cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this.update(cx, |body, cx| body.receive(event, cx)).is_err() {
                    break;
                }
            }
        });
        // GPUI keeps the images it has drawn until told to drop them.
        cx.on_release(|body, cx| {
            for (_, image) in body.bands.drain() {
                cx.drop_image(image, None);
            }
        })
        .detach();
        Self {
            worker,
            _events: events,
            viewport: None,
            generation: 0,
            height: None,
            bands: HashMap::new(),
            requested: HashSet::new(),
            scroll_top: px(0.),
            max_scroll_top: px(0.),
            missing: 0,
            painted: false,
        }
    }

    /// Scrolls down by `delta`, or up if it is negative, within the mail. Returns whether it moved.
    pub fn scroll_by(&mut self, delta: Pixels, cx: &mut Context<Self>) -> bool {
        let top = (self.scroll_top + delta).clamp(px(0.), self.max_scroll_top);
        if top == self.scroll_top {
            return false;
        }
        self.scroll_top = top;
        cx.notify();
        true
    }

    /// Visible bands left blank in the latest frame, because they were still being painted.
    pub fn missing_bands(&self) -> usize {
        self.missing
    }

    fn receive(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Parsed { took } => cx.emit(Rendered::Parse { took }),
            Event::Layout {
                generation,
                height,
                took,
            } if generation == self.generation => {
                self.height = Some(px(height));
                cx.emit(Rendered::Layout {
                    height: px(height),
                    took,
                });
            }
            Event::Band {
                generation,
                band,
                image,
                took,
            } if generation == self.generation => {
                self.requested.remove(&band);
                self.bands.insert(band, image);
                cx.emit(Rendered::Band { took });
            }
            // An answer about a layout that has since been replaced.
            _ => return,
        }
        cx.notify();
    }

    /// Requests a new layout if the size changed, and the bands around the visible ones. Returns
    /// where to draw the visible bands that are ready.
    fn prepaint(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<(Bounds<Pixels>, Arc<RenderImage>)> {
        let scale = window.scale_factor();
        let width = (f32::from(bounds.size.width) * scale).round() as u32;
        let height = (f32::from(bounds.size.height) * scale).round() as u32;
        // Blitz cannot rasterize an empty band; there is nothing to show anyway.
        if width == 0 || height == 0 {
            return Vec::new();
        }
        if self.viewport != Some((bounds.size, scale)) {
            self.viewport = Some((bounds.size, scale));
            self.generation += 1;
            self.height = None;
            self.requested.clear();
            for (_, image) in self.bands.drain() {
                window.drop_image(image).ok();
            }
            self.send(Command::Layout {
                generation: self.generation,
                width,
                height,
                scale,
            });
        }
        let Some(height) = self.height else {
            return Vec::new();
        };
        self.max_scroll_top = (height - bounds.size.height).max(px(0.));
        self.scroll_top = self.scroll_top.clamp(px(0.), self.max_scroll_top);

        let band_height = px(BAND_HEIGHT);
        // Below a short mail, bands carry on its background color, as a browser does.
        let band_count = (height.max(bounds.size.height) / band_height).ceil() as usize;
        let first = (self.scroll_top / band_height) as usize;
        let end = ((self.scroll_top + bounds.size.height) / band_height).ceil() as usize;
        let visible = first..end.min(band_count);
        // The visible bands first, then one either side, so that scrolling rarely reveals a band
        // still being painted.
        let margins = [visible.start.checked_sub(1), Some(visible.end)];
        for band in visible.clone().chain(margins.into_iter().flatten()) {
            if band < band_count && !self.bands.contains_key(&band) && self.requested.insert(band) {
                self.send(Command::Paint {
                    generation: self.generation,
                    band,
                });
            }
        }

        // Over budget, drop the bands farthest from the visible ones.
        let band_bytes = 4. * f32::from(bounds.size.width) * BAND_HEIGHT * scale * scale;
        let budget = ((CACHE_BYTES / band_bytes) as usize).max(visible.len() + 2);
        if self.bands.len() > budget {
            let center = (visible.start + visible.end) / 2;
            let mut cached: Vec<usize> = self.bands.keys().copied().collect();
            cached.sort_by_key(|band| band.abs_diff(center));
            for band in &cached[budget..] {
                if let Some(image) = self.bands.remove(band) {
                    window.drop_image(image).ok();
                }
            }
        }

        let ready: Vec<_> = visible
            .clone()
            .filter_map(|band| {
                let top = bounds.origin.y + band_height * band - self.scroll_top;
                let band_bounds = Bounds::new(
                    point(bounds.origin.x, top),
                    size(bounds.size.width, band_height),
                );
                Some((band_bounds, self.bands.get(&band)?.clone()))
            })
            .collect();
        self.missing = visible.len() - ready.len();
        if !self.painted && self.missing == 0 {
            self.painted = true;
            cx.emit(FirstPaint);
        }
        ready
    }

    fn send(&self, command: Command) {
        // Fails only once the thread has stopped, which leaves the body blank rather than wrong.
        self.worker.send(command).ok();
    }
}

impl EventEmitter<FirstPaint> for MailBody {}
impl EventEmitter<Rendered> for MailBody {}

impl Render for MailBody {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = cx.entity();
        let scrolled = body.downgrade();
        canvas(
            move |bounds, window, cx| {
                let bands = body.update(cx, |body, cx| body.prepaint(bounds, window, cx));
                (window.insert_hitbox(bounds, HitboxBehavior::Normal), bands)
            },
            move |bounds, (hitbox, bands), window, _| {
                window.paint_quad(fill(bounds, white()));
                for (band_bounds, image) in bands {
                    // Fails only if the GPU atlas is full; the band is then left blank.
                    window
                        .paint_image(bounds, band_bounds, Corners::default(), image, 0, false)
                        .ok();
                }
                window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                        return;
                    }
                    let delta = event.delta.pixel_delta(window.line_height()).y;
                    scrolled
                        .update(cx, |body, cx| body.scroll_by(-delta, cx))
                        .ok();
                });
            },
        )
        .size_full()
    }
}
