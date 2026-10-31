//! The editing model of the text input, kept apart from drawing so that it is tested without a window.
//!
//! Offsets are byte offsets into the UTF-8 text, always on a char boundary. The OS text services
//! (input methods, accessibility) count in UTF-16 code units instead, so the methods that mirror
//! `EntityInputHandler` take and return UTF-16 ranges.

use std::ops::Range;

use gpui::UTF16Selection;
use unicode_segmentation::UnicodeSegmentation as _;

#[derive(Default)]
pub struct Buffer {
    text: String,
    selection: Range<usize>,
    /// The cursor is at the start of the selection rather than at its end.
    reversed: bool,
    /// Text an input method is still composing, drawn underlined: the `´` of a dead key waiting for
    /// its letter, or the kana typed before choosing a kanji.
    marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The kind of the last edit and the selection it left, to tell whether the next one continues it.
    last_edit: Option<(EditKind, Range<usize>)>,
}

/// shortcut: an undo step copies the whole text, fine at the size of an email; store the edits
/// instead if long notes make it heavy.
struct Snapshot {
    text: String,
    selection: Range<usize>,
    reversed: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum EditKind {
    Typing,
    Deleting,
    /// Cut and paste, never merged with another edit.
    Clipboard,
}

impl Buffer {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.clone()]
    }

    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    pub fn move_to(&mut self, offset: usize) {
        self.selection = offset..offset;
        self.reversed = false;
    }

    /// Moves the cursor and keeps the other end of the selection where it is.
    pub fn select_to(&mut self, offset: usize) {
        let anchor = if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        };
        self.selection = anchor.min(offset)..anchor.max(offset);
        self.reversed = offset < anchor;
    }

    pub fn select_all(&mut self) {
        self.selection = 0..self.text.len();
        self.reversed = false;
    }

    /// Left and Right collapse a selection to its edge, else move by one grapheme.
    pub fn move_left(&mut self) {
        if self.selection.is_empty() {
            self.move_to(self.previous_grapheme(self.cursor()));
        } else {
            self.move_to(self.selection.start);
        }
    }

    pub fn move_right(&mut self) {
        if self.selection.is_empty() {
            self.move_to(self.next_grapheme(self.cursor()));
        } else {
            self.move_to(self.selection.end);
        }
    }

    /// Backspace and Delete remove the selection, else the grapheme before or after the cursor.
    pub fn delete_backward(&mut self) {
        self.edit(EditKind::Deleting, |buffer| {
            if buffer.selection.is_empty() {
                buffer.select_to(buffer.previous_grapheme(buffer.cursor()));
            }
            buffer.replace(buffer.selection.clone(), "");
        });
    }

    pub fn delete_forward(&mut self) {
        self.edit(EditKind::Deleting, |buffer| {
            if buffer.selection.is_empty() {
                buffer.select_to(buffer.next_grapheme(buffer.cursor()));
            }
            buffer.replace(buffer.selection.clone(), "");
        });
    }

    /// Cut and paste: one undo step each.
    pub fn replace_selection(&mut self, new_text: &str) {
        self.edit(EditKind::Clipboard, |buffer| {
            buffer.replace(buffer.selection.clone(), new_text)
        });
    }

    /// Goes back to the state before the last step, with what was selected then.
    pub fn undo(&mut self) {
        // Edits that changed nothing, such as Backspace at the start, leave steps to skip.
        while let Some(snapshot) = self.undo.pop() {
            if snapshot.text != self.text {
                self.redo.push(self.snapshot());
                self.restore(snapshot);
                return;
            }
        }
    }

    pub fn redo(&mut self) {
        if let Some(snapshot) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(snapshot);
        }
    }

    /// Saves the state an undo goes back to, unless this edit continues the last one: typing or
    /// deleting on from where it left the cursor. An input method's composition is one edit, saved
    /// when it starts.
    fn edit(&mut self, kind: EditKind, edit: impl FnOnce(&mut Self)) {
        let continues =
            kind != EditKind::Clipboard && self.last_edit == Some((kind, self.selection.clone()));
        if self.marked.is_none() && !continues {
            self.undo.push(self.snapshot());
        }
        self.redo.clear();
        edit(self);
        self.last_edit = Some((kind, self.selection.clone()));
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            selection: self.selection.clone(),
            reversed: self.reversed,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.selection = snapshot.selection;
        self.reversed = snapshot.reversed;
        self.marked = None;
        self.last_edit = None;
    }

    /// A grapheme is what a reader sees as one character, even when it is several chars: `e` plus
    /// a combining accent, a flag, an emoji with a skin tone.
    pub fn previous_grapheme(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .rev()
            .map(|(start, _)| start)
            .find(|&start| start < offset)
            .unwrap_or(0)
    }

    pub fn next_grapheme(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(start, grapheme)| start + grapheme.len())
            .find(|&end| end > offset)
            .unwrap_or(self.text.len())
    }

    /// Word moves skip spaces and punctuation, like Alt+arrows in macOS text fields.
    pub fn previous_word_start(&self, offset: usize) -> usize {
        self.text
            .unicode_word_indices()
            .rev()
            .map(|(start, _)| start)
            .find(|&start| start < offset)
            .unwrap_or(0)
    }

    pub fn next_word_end(&self, offset: usize) -> usize {
        self.text
            .unicode_word_indices()
            .map(|(start, word)| start + word.len())
            .find(|&end| end > offset)
            .unwrap_or(self.text.len())
    }

    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..offset].encode_utf16().count()
    }

    /// Clamps to the end of the text, and rounds an offset inside a surrogate pair up.
    fn offset_from_utf16(&self, offset_utf16: usize) -> usize {
        let mut utf16 = 0;
        for (offset, char) in self.text.char_indices() {
            if utf16 >= offset_utf16 {
                return offset;
            }
            utf16 += char.len_utf16();
        }
        self.text.len()
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        let start = self.offset_from_utf16(range_utf16.start);
        start..self.offset_from_utf16(range_utf16.end).max(start)
    }

    fn replace(&mut self, range: Range<usize>, new_text: &str) {
        // The input is a single line, so line breaks become spaces.
        let new_text = new_text.lines().collect::<Vec<_>>().join(" ");
        self.text.replace_range(range.clone(), &new_text);
        self.move_to(range.start + new_text.len());
        self.marked = None;
    }

    /// Without a range, the OS means the marked text if there is some, else the selection.
    fn range_or_default(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range_utf16| self.range_from_utf16(&range_utf16))
            .or(self.marked.clone())
            .unwrap_or(self.selection.clone())
    }

    pub fn text_for_range(
        &self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
    ) -> String {
        let range = self.range_from_utf16(&range_utf16);
        adjusted_range.replace(self.range_to_utf16(&range));
        self.text[range].to_string()
    }

    pub fn selected_text_range(&self) -> UTF16Selection {
        UTF16Selection {
            range: self.range_to_utf16(&self.selection),
            reversed: self.reversed,
        }
    }

    pub fn set_selected_text_range(&mut self, range_utf16: Range<usize>) {
        self.selection = self.range_from_utf16(&range_utf16);
        self.reversed = false;
    }

    pub fn text_length_utf16(&self) -> usize {
        self.text.encode_utf16().count()
    }

    pub fn marked_text_range(&self) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    /// The composition is accepted as it stands.
    pub fn unmark_text(&mut self) {
        self.marked = None;
    }

    /// Committed text: a typed character, the end of a composition.
    pub fn replace_text_in_range(&mut self, range_utf16: Option<Range<usize>>, new_text: &str) {
        let range = self.range_or_default(range_utf16);
        self.edit(EditKind::Typing, |buffer| buffer.replace(range, new_text));
    }

    /// Text an input method is composing. `new_selected_range_utf16` is relative to `new_text`.
    pub fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
    ) {
        let range = self.range_or_default(range_utf16);
        self.edit(EditKind::Typing, |buffer| {
            buffer.replace(range.clone(), new_text);
            if !new_text.is_empty() {
                buffer.marked = Some(range.start..buffer.cursor());
            }
            if let Some(selected) = new_selected_range_utf16 {
                let start = buffer.offset_to_utf16(range.start);
                let len = new_text.encode_utf16().count();
                buffer.selection = buffer.range_from_utf16(
                    &(start + selected.start.min(len)..start + selected.end.min(len)),
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> Buffer {
        let mut buffer = Buffer::default();
        buffer.replace_text_in_range(None, text);
        buffer
    }

    #[test]
    fn converts_between_utf8_and_utf16() {
        // `é` is 2 bytes and 1 UTF-16 unit, `🙂` 4 bytes and 2 units (a surrogate pair), `漢` 3 and 1.
        let mut buffer = buffer("é🙂漢");
        assert_eq!(buffer.text_length_utf16(), 4);

        buffer.set_selected_text_range(1..3);
        assert_eq!(buffer.selection(), 2..6);
        assert_eq!(buffer.selected_text(), "🙂");
        assert_eq!(buffer.selected_text_range().range, 1..3);

        // Inside the surrogate pair, past the end.
        buffer.set_selected_text_range(2..99);
        assert_eq!(buffer.selection(), 6..9);

        let mut adjusted = None;
        assert_eq!(buffer.text_for_range(1..99, &mut adjusted), "🙂漢");
        assert_eq!(adjusted, Some(1..4));
    }

    #[test]
    fn moves_and_deletes_whole_graphemes() {
        // `e` + combining acute accent, a flag (two chars), a thumb with a skin tone, a ZWJ family.
        let text = "e\u{301}🇫🇷👍🏽👨‍👩‍👧";
        let mut buffer = buffer(text);
        buffer.move_to(0);
        let mut stops = vec![];
        while buffer.cursor() < text.len() {
            buffer.move_right();
            stops.push(buffer.cursor());
        }
        assert_eq!(stops, [3, 11, 19, text.len()]);

        buffer.move_left();
        assert_eq!(buffer.cursor(), 19);
        buffer.delete_backward();
        assert_eq!(buffer.text(), "e\u{301}🇫🇷👨‍👩‍👧");
        buffer.move_to(0);
        buffer.delete_forward();
        assert_eq!(buffer.text(), "🇫🇷👨‍👩‍👧");
    }

    #[test]
    fn moves_by_word() {
        let buffer = buffer("Hello, wörld! l'été");
        assert_eq!(buffer.next_word_end(0), 5);
        assert_eq!(buffer.next_word_end(5), 13);
        assert_eq!(buffer.previous_word_start(13), 7);
        // UAX #29 keeps the French elision in one word.
        assert_eq!(buffer.previous_word_start(buffer.text().len()), 15);
        assert_eq!(
            buffer.next_word_end(buffer.text().len()),
            buffer.text().len()
        );
        assert_eq!(buffer.previous_word_start(0), 0);
    }

    #[test]
    fn selects_from_an_anchor() {
        let mut buffer = buffer("hello");
        buffer.move_to(2);
        buffer.select_to(4);
        assert_eq!((buffer.selection(), buffer.cursor()), (2..4, 4));
        // Crossing the anchor flips the selection.
        buffer.select_to(0);
        assert_eq!((buffer.selection(), buffer.cursor()), (0..2, 0));
        assert!(buffer.selected_text_range().reversed);

        buffer.move_right();
        assert_eq!((buffer.selection(), buffer.cursor()), (2..2, 2));

        buffer.select_all();
        buffer.delete_backward();
        assert_eq!(buffer.text(), "");
    }

    #[test]
    fn replaces_the_selection_and_flattens_line_breaks() {
        let mut buffer = buffer("hello world");
        buffer.move_to(6);
        buffer.select_to(11);
        buffer.replace_text_in_range(None, "one\ntwo\r\nthree\n");
        assert_eq!(buffer.text(), "hello one two three");
        assert_eq!(buffer.selection(), 19..19);

        buffer.replace_text_in_range(Some(0..5), "bye");
        assert_eq!(buffer.text(), "bye one two three");
        assert_eq!(buffer.cursor(), 3);
    }

    #[test]
    fn composes_a_dead_key_accent() {
        // On macOS, `´` then `e` arrives as marked `´`, then `é` committed over it.
        let mut buffer = buffer("caf");
        buffer.replace_and_mark_text_in_range(None, "´", Some(1..1));
        assert_eq!(buffer.text(), "caf´");
        assert_eq!(buffer.marked(), Some(3..5));
        assert_eq!(buffer.marked_text_range(), Some(3..4));
        assert_eq!(buffer.cursor(), 5);

        buffer.replace_text_in_range(None, "é");
        assert_eq!(buffer.text(), "café");
        assert_eq!(buffer.marked(), None);
        assert_eq!(buffer.cursor(), 5);
    }

    #[test]
    fn composes_with_an_asian_input_method() {
        let mut buffer = buffer("A🙂Z");
        // Japanese: `k` then `a` becomes `か`, still marked; then the candidate `火` is chosen.
        buffer.move_to(1);
        buffer.replace_and_mark_text_in_range(None, "k", Some(1..1));
        buffer.replace_and_mark_text_in_range(None, "か", Some(1..1));
        assert_eq!(buffer.text(), "Aか🙂Z");
        assert_eq!(buffer.marked(), Some(1..4));
        buffer.replace_text_in_range(None, "火");
        assert_eq!(buffer.text(), "A火🙂Z");

        // An explicit UTF-16 range, and a selection inside the marked text.
        buffer.replace_and_mark_text_in_range(Some(2..4), "é🙂", Some(1..3));
        assert_eq!(buffer.text(), "A火é🙂Z");
        assert_eq!(buffer.marked(), Some(4..10));
        assert_eq!(buffer.selection(), 6..10);

        // Escape: the input method removes its marked text.
        buffer.replace_and_mark_text_in_range(None, "", Some(0..0));
        assert_eq!(buffer.text(), "A火Z");
        assert_eq!(buffer.marked(), None);
        assert_eq!(buffer.cursor(), 4);

        // Clicking elsewhere keeps the composition as it stands.
        buffer.replace_and_mark_text_in_range(None, "か", None);
        buffer.unmark_text();
        assert_eq!(buffer.text(), "A火かZ");
        assert_eq!(buffer.marked(), None);
    }

    #[test]
    fn undoes_typing_in_one_step_until_the_cursor_moves_or_deletes() {
        let mut buffer = Buffer::default();
        for char in ["a", "b", "c"] {
            buffer.replace_text_in_range(None, char);
        }
        buffer.delete_backward();
        buffer.delete_backward();
        buffer.replace_text_in_range(None, "x");
        buffer.move_to(0);
        buffer.replace_text_in_range(None, "y");
        assert_eq!(buffer.text(), "yax");

        let mut steps = vec![];
        for _ in 0..5 {
            buffer.undo();
            steps.push(buffer.text().to_string());
        }
        assert_eq!(steps, ["ax", "a", "abc", "", ""]);
    }

    #[test]
    fn undoes_cut_and_paste_in_their_own_step() {
        let mut buffer = buffer("ab");
        buffer.replace_selection("cd");
        buffer.replace_text_in_range(None, "e");
        buffer.select_to(2);
        buffer.replace_selection("");
        buffer.delete_backward();
        assert_eq!(buffer.text(), "a");

        let mut steps = vec![];
        for _ in 0..4 {
            buffer.undo();
            steps.push(buffer.text().to_string());
        }
        assert_eq!(steps, ["ab", "abcde", "abcd", "ab"]);
    }

    #[test]
    fn restores_the_selection_and_redoes_until_a_new_edit() {
        let mut buffer = buffer("hello world");
        buffer.move_to(6);
        buffer.select_to(11);
        buffer.replace_text_in_range(None, "you");
        buffer.undo();
        assert_eq!(buffer.text(), "hello world");
        assert_eq!(buffer.selection(), 6..11);

        buffer.redo();
        assert_eq!(buffer.text(), "hello you");
        assert_eq!(buffer.cursor(), 9);
        buffer.undo();
        buffer.replace_text_in_range(None, "all");
        buffer.redo();
        assert_eq!(buffer.text(), "hello all");
    }

    #[test]
    fn undoes_a_composition_in_one_step() {
        let mut buffer = buffer("AZ");
        buffer.move_to(1);
        buffer.replace_and_mark_text_in_range(None, "k", Some(1..1));
        buffer.replace_and_mark_text_in_range(None, "か", Some(1..1));
        buffer.replace_text_in_range(None, "火");
        // A dead key accent continues the typing it is part of.
        buffer.replace_text_in_range(None, "n");
        buffer.replace_and_mark_text_in_range(None, "^", Some(1..1));
        buffer.replace_text_in_range(None, "ê");
        assert_eq!(buffer.text(), "A火nêZ");

        buffer.undo();
        assert_eq!(buffer.text(), "AZ");
        assert_eq!(buffer.cursor(), 1);
        buffer.redo();
        assert_eq!(buffer.text(), "A火nêZ");

        // A cancelled composition leaves no step of its own: undo goes past it.
        buffer.move_to(0);
        buffer.replace_and_mark_text_in_range(None, "k", Some(1..1));
        buffer.replace_and_mark_text_in_range(None, "", Some(0..0));
        buffer.undo();
        assert_eq!(buffer.text(), "AZ");
    }
}
