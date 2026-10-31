//! A single-line text input. It edits through GPUI's platform input handler, which is what lets the
//! OS input methods (dead keys, accents, Asian input methods, the emoji picker) write into it.
//! Adapted from GPUI's `examples/input.rs` (Apache-2.0), informed by gpui-component's Input (Apache-2.0).

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the input that uses it comes next")
)]
mod buffer;
