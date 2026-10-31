//! A text input, on one line or several. It edits through GPUI's platform input handler, which is
//! what lets the OS input methods (dead keys, accents, Asian input methods, the emoji picker) write
//! into it. Adapted from GPUI's `examples/input.rs` (Apache-2.0), informed by gpui-component's Input
//! (Apache-2.0).

mod buffer;

use std::{ops::Range, rc::Rc};

use gpui::{
    Action, App, Bounds, ClipboardItem, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, GlobalElementId, InspectorElementId, KeyBinding, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ScrollEvent, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, WrappedLine, actions,
    fill, point, relative, size,
};
use theme::ActiveTheme as _;
use zed_ui::prelude::*;

use buffer::{Buffer, Layout};

const KEY_CONTEXT: &str = "TextInput";
const CURSOR_WIDTH: Pixels = px(2.);

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        Home,
        End,
        SelectHome,
        SelectEnd,
        TextStart,
        TextEnd,
        SelectTextStart,
        SelectTextEnd,
        SelectAll,
        Newline,
        Undo,
        Redo,
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
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new(&format!("{word}-left"), WordLeft, context),
        KeyBinding::new(&format!("{word}-right"), WordRight, context),
        KeyBinding::new(&format!("{word}-shift-left"), SelectWordLeft, context),
        KeyBinding::new(&format!("{word}-shift-right"), SelectWordRight, context),
        KeyBinding::new("home", Home, context),
        KeyBinding::new("end", End, context),
        KeyBinding::new("shift-home", SelectHome, context),
        KeyBinding::new("shift-end", SelectEnd, context),
        KeyBinding::new("secondary-a", SelectAll, context),
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("secondary-z", Undo, context),
        KeyBinding::new("secondary-shift-z", Redo, context),
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
        KeyBinding::new("cmd-up", TextStart, context),
        KeyBinding::new("cmd-down", TextEnd, context),
        KeyBinding::new("cmd-shift-up", SelectTextStart, context),
        KeyBinding::new("cmd-shift-down", SelectTextEnd, context),
        // Without a menu bar, nothing else opens the emoji picker.
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, context),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-home", TextStart, context),
        KeyBinding::new("ctrl-end", TextEnd, context),
        KeyBinding::new("ctrl-shift-home", SelectTextStart, context),
        KeyBinding::new("ctrl-shift-end", SelectTextEnd, context),
        KeyBinding::new("ctrl-y", Redo, context),
    ]);
}

pub struct TextInput {
    focus_handle: FocusHandle,
    buffer: Buffer,
    placeholder: SharedString,
    /// Keeps the cursor in view when the text does not fit in the input.
    scroll: Point<Pixels>,
    /// Set by edits and moves; the mouse wheel scrolls away from the cursor.
    scroll_to_cursor: bool,
    /// What was last painted, to turn mouse positions and OS ranges into text offsets and back.
    last_layout: Option<Rc<ShapedText>>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            buffer: Buffer::default(),
            placeholder: placeholder.into(),
            scroll: Point::default(),
            scroll_to_cursor: false,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    /// Enter inserts a line break and long lines wrap. The input takes the height of its parent.
    pub fn multi_line(mut self) -> Self {
        self.buffer = Buffer::new(true);
        self
    }

    fn edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Buffer),
    ) {
        edit(&mut self.buffer);
        self.scroll_to_cursor = true;
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
            self.edit(window, cx, |buffer| buffer.replace_selection(""));
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.edit(window, cx, |buffer| buffer.replace_selection(&text));
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        if let Some(offset) = self.offset_for_point(event.position) {
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
            && let Some(offset) = self.offset_for_point(event.position)
        {
            self.edit(window, cx, |buffer| buffer.select_to(offset));
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_scroll(&mut self, event: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll -= event.delta.pixel_delta(window.line_height());
        self.scroll_to_cursor = false;
        cx.notify();
    }

    /// `None` until the current text has been painted: an offset from the layout of an older text
    /// may fall inside a char of the new one.
    fn layout(&self) -> Option<Rc<ShapedText>> {
        self.last_layout
            .clone()
            .filter(|layout| layout.text.as_ref() == self.buffer.text())
    }

    fn offset_for_point(&self, point: Point<Pixels>) -> Option<usize> {
        let origin = self.last_bounds?.origin - self.scroll;
        Some(self.layout()?.offset_for_position(point - origin))
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
        let layout = self.last_layout.as_ref()?;
        let range = self.buffer.range_from_utf16(&range_utf16);
        let origin = bounds.origin - self.scroll;
        let start = origin + layout.position_for_offset(range.start);
        let end = origin + layout.position_for_offset(range.end);
        let clamp = |corner: Point<Pixels>| {
            point(
                corner.x.clamp(bounds.left(), bounds.right()),
                corner.y.clamp(bounds.top(), bounds.bottom()),
            )
        };
        Some(Bounds::from_corners(
            clamp(start),
            clamp(point(end.x.max(start.x), start.y + layout.line_height)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.last_bounds?.localize(&point)?;
        Some(self.buffer.offset_to_utf16(self.offset_for_point(point)?))
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let multi_line = self.buffer.multi_line();
        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(on::<Backspace>(cx, Buffer::delete_backward))
            .on_action(on::<Delete>(cx, Buffer::delete_forward))
            .on_action(on::<Left>(cx, Buffer::move_left))
            .on_action(on::<Right>(cx, Buffer::move_right))
            .on_action(on_layout::<Up>(cx, |b, l| {
                b.move_vertically(l, false, false)
            }))
            .on_action(on_layout::<Down>(cx, |b, l| {
                b.move_vertically(l, true, false)
            }))
            .on_action(on::<SelectLeft>(cx, |b| {
                b.select_to(b.previous_grapheme(b.cursor()))
            }))
            .on_action(on::<SelectRight>(cx, |b| {
                b.select_to(b.next_grapheme(b.cursor()))
            }))
            .on_action(on_layout::<SelectUp>(cx, |b, l| {
                b.move_vertically(l, false, true)
            }))
            .on_action(on_layout::<SelectDown>(cx, |b, l| {
                b.move_vertically(l, true, true)
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
            .on_action(on_layout::<Home>(cx, |b, l| {
                b.move_to(b.line_edge(l, false))
            }))
            .on_action(on_layout::<End>(cx, |b, l| b.move_to(b.line_edge(l, true))))
            .on_action(on_layout::<SelectHome>(cx, |b, l| {
                b.select_to(b.line_edge(l, false))
            }))
            .on_action(on_layout::<SelectEnd>(cx, |b, l| {
                b.select_to(b.line_edge(l, true))
            }))
            .on_action(on::<TextStart>(cx, |b| b.move_to(0)))
            .on_action(on::<TextEnd>(cx, |b| b.move_to(b.text().len())))
            .on_action(on::<SelectTextStart>(cx, |b| b.select_to(0)))
            .on_action(on::<SelectTextEnd>(cx, |b| b.select_to(b.text().len())))
            .on_action(on::<SelectAll>(cx, Buffer::select_all))
            .when(multi_line, |input| {
                input.on_action(on::<Newline>(cx, |b| b.replace_text_in_range(None, "\n")))
            })
            .on_action(on::<Undo>(cx, Buffer::undo))
            .on_action(on::<Redo>(cx, Buffer::redo))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(|_: &ShowCharacterPalette, window, _| window.show_character_palette())
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll(cx.listener(Self::on_scroll))
            .w_full()
            .when(multi_line, |input| input.h_full())
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
            .child(
                div()
                    .h_full()
                    .overflow_hidden()
                    .child(TextElement(cx.entity())),
            )
    }
}

/// Applies `edit` to the buffer when action `A` fires.
fn on<A: Action>(
    cx: &Context<TextInput>,
    edit: fn(&mut Buffer),
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    cx.listener(move |this, _: &A, window, cx| this.edit(window, cx, edit))
}

/// Like `on`, for the moves that depend on where the lines wrap.
fn on_layout<A: Action>(
    cx: &Context<TextInput>,
    edit: fn(&mut Buffer, &ShapedText),
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    cx.listener(move |this, _: &A, window, cx| {
        // shortcut: a key pressed before the last edit is painted is dropped; rare at human
        // speed, lay the text out on demand if it shows.
        if let Some(layout) = this.layout() {
            this.edit(window, cx, |buffer| edit(buffer, &layout))
        }
    })
}

/// The text as shaped for painting: one `WrappedLine` per line of the text, cut into rows where it
/// wraps.
struct ShapedText {
    text: SharedString,
    lines: Vec<WrappedLine>,
    rows: Vec<Row>,
    line_height: Pixels,
}

/// A visual line: a line of the text, or the part of it on one row when it wraps.
struct Row {
    /// Index in `ShapedText::lines`, and the offset where that line starts in the text.
    line: usize,
    line_start: usize,
    range: Range<usize>,
}

impl ShapedText {
    fn new(text: SharedString, lines: Vec<WrappedLine>, line_height: Pixels) -> Self {
        let mut rows = Vec::new();
        let mut line_start = 0;
        for (ix, line) in lines.iter().enumerate() {
            let wraps = line
                .wrap_boundaries()
                .iter()
                .map(|wrap| line.runs()[wrap.run_ix].glyphs[wrap.glyph_ix].index);
            let mut start = line_start;
            for end in wraps.chain([line.len()]) {
                rows.push(Row {
                    line: ix,
                    line_start,
                    range: start..line_start + end,
                });
                start = line_start + end;
            }
            line_start += line.len() + 1;
        }
        Self {
            text,
            lines,
            rows,
            line_height,
        }
    }

    /// Where `offset` is drawn on `row`, from the left edge of the input.
    fn x(&self, row: &Row, offset: usize) -> Pixels {
        let line = &self.lines[row.line].unwrapped_layout;
        line.x_for_index(offset - row.line_start)
            - line.x_for_index(row.range.start - row.line_start)
    }
}

impl Layout for ShapedText {
    /// Where a line wraps, the cursor shows at the start of the next row.
    fn position_for_offset(&self, offset: usize) -> Point<Pixels> {
        let ix = self
            .rows
            .partition_point(|row| row.range.start <= offset)
            .saturating_sub(1);
        self.rows.get(ix).map_or(Point::default(), |row| {
            point(
                self.x(row, offset.min(row.range.end)),
                self.line_height * ix,
            )
        })
    }

    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        if position.y < px(0.) {
            return 0;
        }
        let ix = (position.y / self.line_height) as usize;
        let Some(row) = self.rows.get(ix) else {
            return self.text.len();
        };
        let line = &self.lines[row.line].unwrapped_layout;
        let x = line.x_for_index(row.range.start - row.line_start) + position.x;
        let offset =
            (row.line_start + line.closest_index_for_x(x)).clamp(row.range.start, row.range.end);
        // A wrapped row ends where the next one starts: stay on this one, before its last char.
        match self.rows.get(ix + 1) {
            Some(next) if next.range.start == offset => self.text[..offset]
                .chars()
                .next_back()
                .map_or(offset, |char| offset - char.len_utf8()),
            _ => offset,
        }
    }

    fn line_height(&self) -> Pixels {
        self.line_height
    }
}

/// Draws the text, the selection and the cursor, and hands the OS the input handler while focused.
struct TextElement(Entity<TextInput>);

struct PrepaintState {
    layout: Rc<ShapedText>,
    scroll: Point<Pixels>,
    selection: Vec<PaintQuad>,
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
        style.size.height = if self.0.read(cx).buffer.multi_line() {
            relative(1.).into()
        } else {
            window.line_height().into()
        };
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
        let line_height = window.line_height();
        let wrap_width = buffer
            .multi_line()
            .then_some(bounds.size.width - CURSOR_WIDTH);
        // Text that fails to shape is left blank rather than taking the app down.
        let lines = window
            .text_system()
            .shape_text(text.clone(), font_size, &runs, wrap_width, None)
            .map(|lines| lines.into_vec())
            .unwrap_or_default();
        let layout = ShapedText::new(text, lines, line_height);

        // Scroll as little as possible to show the cursor, and never past the end of the text.
        let width = layout
            .lines
            .iter()
            .fold(px(0.), |width, line| width.max(line.width()));
        let height = line_height * layout.rows.len();
        let cursor = layout.position_for_offset(buffer.cursor());
        let mut scroll = point(
            input
                .scroll
                .x
                .min(width + CURSOR_WIDTH - bounds.size.width)
                .max(px(0.)),
            input.scroll.y.min(height - bounds.size.height).max(px(0.)),
        );
        if input.scroll_to_cursor {
            scroll.x = scroll
                .x
                .max(cursor.x + CURSOR_WIDTH - bounds.size.width)
                .min(cursor.x);
            scroll.y = scroll
                .y
                .max(cursor.y + line_height - bounds.size.height)
                .min(cursor.y);
        }
        let origin = bounds.origin - scroll;

        let player = cx.theme().players().local();
        let selection = buffer.selection();
        let selection = (layout.rows.iter().enumerate())
            .filter_map(|(ix, row)| {
                let start = selection.start.max(row.range.start);
                let end = selection.end.min(row.range.end);
                let top = origin.y + line_height * ix;
                (start < end).then(|| {
                    fill(
                        Bounds::from_corners(
                            point(origin.x + layout.x(row, start), top),
                            point(origin.x + layout.x(row, end), top + line_height),
                        ),
                        player.selection,
                    )
                })
            })
            .collect();
        let cursor = fill(
            Bounds::new(origin + cursor, size(CURSOR_WIDTH, line_height)),
            player.cursor,
        );
        PrepaintState {
            layout: Rc::new(layout),
            scroll,
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
        for selection in prepaint.selection.drain(..) {
            window.paint_quad(selection);
        }
        let line_height = window.line_height();
        let mut origin = bounds.origin - prepaint.scroll;
        for line in &prepaint.layout.lines {
            // A line that fails to paint is left blank rather than taking the app down.
            line.paint(origin, line_height, TextAlign::Left, None, window, cx)
                .ok();
            origin.y += line.size(line_height).height;
        }
        if focus_handle.is_focused(window) {
            window.paint_quad(prepaint.cursor.clone());
        }
        self.0.update(cx, |input, _| {
            input.last_layout = Some(prepaint.layout.clone());
            input.last_bounds = Some(bounds);
            input.scroll = prepaint.scroll;
            input.scroll_to_cursor = false;
        });
    }
}
