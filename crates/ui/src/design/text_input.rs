//! A single-line text input. It edits through GPUI's platform input handler, which is what lets the
//! OS input methods (dead keys, accents, Asian input methods, the emoji picker) write into it.
//! Adapted from GPUI's `examples/input.rs` (Apache-2.0), informed by gpui-component's Input (Apache-2.0).

mod buffer;

use std::ops::Range;

use gpui::{
    Action, App, Bounds, ClipboardItem, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, GlobalElementId, InspectorElementId, KeyBinding, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, actions, fill, point,
    relative, size,
};
use theme::ActiveTheme as _;
use zed_ui::prelude::*;

use buffer::Buffer;

const KEY_CONTEXT: &str = "TextInput";
const CURSOR_WIDTH: Pixels = px(2.);

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        Home,
        End,
        SelectHome,
        SelectEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
        ShowCharacterPalette,
    ]
);

/// Called once at startup. Word moves use Alt on macOS and Ctrl elsewhere, like each OS's own text fields.
pub fn bind_keys(cx: &mut App) {
    let word = if cfg!(target_os = "macos") {
        "alt"
    } else {
        "ctrl"
    };
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new(&format!("{word}-left"), WordLeft, context),
        KeyBinding::new(&format!("{word}-right"), WordRight, context),
        KeyBinding::new(&format!("{word}-shift-left"), SelectWordLeft, context),
        KeyBinding::new(&format!("{word}-shift-right"), SelectWordRight, context),
        KeyBinding::new("home", Home, context),
        KeyBinding::new("end", End, context),
        KeyBinding::new("shift-home", SelectHome, context),
        KeyBinding::new("shift-end", SelectEnd, context),
        KeyBinding::new("secondary-a", SelectAll, context),
        KeyBinding::new("secondary-c", Copy, context),
        KeyBinding::new("secondary-x", Cut, context),
        KeyBinding::new("secondary-v", Paste, context),
    ]);
    // Mac keyboards often have no Home and End keys: Cmd+arrows are the native line moves.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-left", Home, context),
        KeyBinding::new("cmd-right", End, context),
        KeyBinding::new("cmd-shift-left", SelectHome, context),
        KeyBinding::new("cmd-shift-right", SelectEnd, context),
        // Without a menu bar, nothing else opens the emoji picker.
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, context),
    ]);
}

pub struct TextInput {
    focus_handle: FocusHandle,
    buffer: Buffer,
    placeholder: SharedString,
    /// Keeps the cursor in view when the text is wider than the input.
    scroll_x: Pixels,
    /// What was last painted, to turn mouse positions and OS ranges into text offsets and back.
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            buffer: Buffer::default(),
            placeholder: placeholder.into(),
            scroll_x: px(0.),
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    fn edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Buffer),
    ) {
        edit(&mut self.buffer);
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.selection().is_empty() {
            let text = self.buffer.selected_text().to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.selection().is_empty() {
            self.copy(&Copy, window, cx);
            self.edit(window, cx, Buffer::delete_backward);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.edit(window, cx, |buffer| {
                buffer.replace_text_in_range(None, &text)
            });
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        if let Some(offset) = self.offset_for_x(event.position.x) {
            self.edit(window, cx, |buffer| {
                if event.modifiers.shift {
                    buffer.select_to(offset)
                } else {
                    buffer.move_to(offset)
                }
            });
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting
            && let Some(offset) = self.offset_for_x(event.position.x)
        {
            self.edit(window, cx, |buffer| buffer.select_to(offset));
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    /// `None` until the current text has been painted: an offset from an older layout may fall
    /// inside a char of the new text.
    fn offset_for_x(&self, x: Pixels) -> Option<usize> {
        let line = self.last_layout.as_ref()?;
        if line.text.as_ref() != self.buffer.text() {
            return None;
        }
        Some(line.closest_index_for_x(x - self.last_bounds?.left() + self.scroll_x))
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        Some(self.buffer.text_for_range(range_utf16, adjusted_range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(self.buffer.selected_text_range())
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(window, cx, |buffer| {
            buffer.set_selected_text_range(range_utf16)
        });
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.buffer.text_length_utf16())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buffer.marked_text_range()
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit(window, cx, Buffer::unmark_text);
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(window, cx, |buffer| {
            buffer.replace_text_in_range(range_utf16, text)
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(window, cx, |buffer| {
            buffer.replace_and_mark_text_in_range(range_utf16, text, new_selected_range_utf16)
        });
    }

    /// Where the input method shows its candidate window. Called right after an edit, before the
    /// repaint: the layout of the previous text (even the placeholder) usually still places the
    /// composition right, and clamping keeps it on the input when it does not.
    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.buffer.range_from_utf16(&range_utf16);
        let x = |offset| {
            (bounds.left() - self.scroll_x + line.x_for_index(offset))
                .clamp(bounds.left(), bounds.right())
        };
        Some(Bounds::from_corners(
            point(x(range.start), bounds.top()),
            point(x(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.last_bounds?.localize(&point)?;
        Some(self.buffer.offset_to_utf16(self.offset_for_x(point.x)?))
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(on::<Backspace>(cx, Buffer::delete_backward))
            .on_action(on::<Delete>(cx, Buffer::delete_forward))
            .on_action(on::<Left>(cx, Buffer::move_left))
            .on_action(on::<Right>(cx, Buffer::move_right))
            .on_action(on::<SelectLeft>(cx, |b| {
                b.select_to(b.previous_grapheme(b.cursor()))
            }))
            .on_action(on::<SelectRight>(cx, |b| {
                b.select_to(b.next_grapheme(b.cursor()))
            }))
            .on_action(on::<WordLeft>(cx, |b| {
                b.move_to(b.previous_word_start(b.cursor()))
            }))
            .on_action(on::<WordRight>(cx, |b| {
                b.move_to(b.next_word_end(b.cursor()))
            }))
            .on_action(on::<SelectWordLeft>(cx, |b| {
                b.select_to(b.previous_word_start(b.cursor()))
            }))
            .on_action(on::<SelectWordRight>(cx, |b| {
                b.select_to(b.next_word_end(b.cursor()))
            }))
            .on_action(on::<Home>(cx, |b| b.move_to(0)))
            .on_action(on::<End>(cx, |b| b.move_to(b.text().len())))
            .on_action(on::<SelectHome>(cx, |b| b.select_to(0)))
            .on_action(on::<SelectEnd>(cx, |b| b.select_to(b.text().len())))
            .on_action(on::<SelectAll>(cx, Buffer::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(|_: &ShowCharacterPalette, window, _| window.show_character_palette())
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(if self.focus_handle.is_focused(window) {
                colors.border_focused
            } else {
                colors.border
            })
            .bg(colors.editor_background)
            .font_ui(cx)
            .text_ui(cx)
            .text_color(colors.text)
            .child(div().overflow_hidden().child(TextElement(cx.entity())))
    }
}

/// Applies `edit` to the buffer when action `A` fires.
fn on<A: Action>(
    cx: &Context<TextInput>,
    edit: fn(&mut Buffer),
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    cx.listener(move |this, _: &A, window, cx| this.edit(window, cx, edit))
}

/// Draws the text, the selection and the cursor, and hands the OS the input handler while focused.
struct TextElement(Entity<TextInput>);

struct PrepaintState {
    line: ShapedLine,
    scroll_x: Pixels,
    selection: Option<PaintQuad>,
    cursor: PaintQuad,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.0.read(cx);
        let buffer = &input.buffer;
        let style = window.text_style();
        let (text, color) = if buffer.text().is_empty() {
            (
                input.placeholder.clone(),
                cx.theme().colors().text_placeholder,
            )
        } else {
            (buffer.text().to_string().into(), style.color)
        };

        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match buffer.marked() {
            Some(marked) => [
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.len(),
                    underline: Some(UnderlineStyle {
                        color: Some(color),
                        thickness: px(1.),
                        ..Default::default()
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: text.len() - marked.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            None => vec![run],
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(text, font_size, &runs, None);

        // Scroll as little as possible to show the cursor, and never past the end of the text.
        let cursor_x = line.x_for_index(buffer.cursor());
        let visible = bounds.size.width - CURSOR_WIDTH;
        let scroll_x = input
            .scroll_x
            .min((line.width - visible).max(px(0.)))
            .max(cursor_x - visible)
            .min(cursor_x);
        let left = bounds.left() - scroll_x;

        let player = cx.theme().players().local();
        let selection = buffer.selection();
        let selection = (!selection.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    point(left + line.x_for_index(selection.start), bounds.top()),
                    point(left + line.x_for_index(selection.end), bounds.bottom()),
                ),
                player.selection,
            )
        });
        let cursor = fill(
            Bounds::new(
                point(left + cursor_x, bounds.top()),
                size(CURSOR_WIDTH, bounds.size.height),
            ),
            player.cursor,
        );
        PrepaintState {
            line,
            scroll_x,
            selection,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.0.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.0.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let origin = point(bounds.left() - prepaint.scroll_x, bounds.top());
        // A line that fails to paint is left blank rather than taking the app down.
        prepaint
            .line
            .paint(
                origin,
                window.line_height(),
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
        if focus_handle.is_focused(window) {
            window.paint_quad(prepaint.cursor.clone());
        }
        self.0.update(cx, |input, _| {
            input.last_layout = Some(prepaint.line.clone());
            input.last_bounds = Some(bounds);
            input.scroll_x = prepaint.scroll_x;
        });
    }
}
