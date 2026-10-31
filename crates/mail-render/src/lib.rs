//! Mail body rendering for the UI: sanitize and preprocess HTML, then render it with Blitz into GPUI.

mod body;
mod worker;

pub use body::{FirstPaint, MailBody};
