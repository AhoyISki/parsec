pub mod inner;
pub mod reader;

use std::{
    iter::Peekable,
    ops::{Range, RangeBounds, RangeInclusive}
};

use ropey::Rope;

use self::{inner::InnerText, reader::MutTextReader};
use crate::{
    history::Change,
    position::{Cursor, Pos},
    tags::{
        form::{FormPalette, EXTRA_SEL, MAIN_SEL},
        Lock, Tag, TagOrSkip, Tags
    },
    ui::{Area, Label, Ui}, log_info
};

/// Builds and modifies a [`Text<U>`], based on replacements applied
/// to it.
///
/// The generation of text by the `TextBuilder<U>` has a few
/// peculiarities that are convenient in the situations where it is
/// useful:
///
/// - The user cannot insert [`Tag`]s directly, only by appending and
///   modifying
/// existing tags.
/// - All [`Tag`]s that are appended result in an inverse [`Tag`]
///   being placed
/// before the next one, or at the end of the [`Tags`] (e.g.
/// [`Tag::PushForm`] would be followed a [`Tag::PopForm`]).
/// - You can insert swappable text with
///   [`push_swappable()`][Self::push_swappable].
///
/// These properties allow for quick and easy modification of the
/// [`Text<U>`] within, which can then be accessed with
/// [`text()`][Self::text()].
pub struct TextBuilder<U>
where
    U: Ui
{
    text: Text<U>,
    swappables: Vec<usize>
}

impl<U> Default for TextBuilder<U>
where
    U: Ui
{
    fn default() -> Self {
        TextBuilder {
            text: Text::default_string(),
            swappables: Vec::default()
        }
    }
}

impl<U> TextBuilder<U>
where
    U: Ui
{
    pub fn push_text(&mut self, edit: impl AsRef<str>) {
        let edit = edit.as_ref();
        let edit_len = edit.chars().count() as u32;
        self.text.inner.string().push_str(edit);

        self.add_to_last_skip(edit_len);
    }

    pub fn push_swappable(&mut self, edit: impl AsRef<str>) {
        let edit = edit.as_ref();
        let edit_len = edit.chars().count() as u32;

        let last_skip = self
            .text
            .tags
            .vec()
            .iter()
            .filter(|tag_or_skip| matches!(tag_or_skip, TagOrSkip::Skip(_)))
            .count();

        self.swappables.push(last_skip);
        self.text.inner.string().push_str(edit);

        self.add_to_last_skip(edit_len);
    }

    /// Pushes a [`Tag`] to the end of the list of [`Tag`]s, as well
    /// as its inverse at the end of the [`Text<U>`].
    pub fn push_tag(&mut self, tag: Tag) -> Lock {
        let lock = self.text.tags.get_lock();
        self.text.tags.vec().push(TagOrSkip::Tag(tag, lock));
        if let Some(inv_tag) = tag.inverse() {
            self.text.tags.vec().push(TagOrSkip::Tag(inv_tag, lock));
        }
        lock
    }

    /// Replaces a range with a new piece of text.
    pub fn swap_range(&mut self, index: usize, edit: impl AsRef<str>) {
        let edit = edit.as_ref();
        let swap_index = self.swappables[index];

        let Some((start, skip)) = self.get_mut_skip(swap_index) else {
            return;
        };
        let old_skip = *skip as usize;
        *skip = edit.chars().count() as u32;

        self.text.inner.replace(start..(start + old_skip), edit);
    }

    pub fn swap_tag(&mut self, tag_index: usize, new_tag: Tag) {
        let tags_vec = self.text.tags.vec();
        let mut tags =
            tags_vec
                .iter_mut()
                .enumerate()
                .filter_map(|(index, tag_or_skip)| match tag_or_skip {
                    TagOrSkip::Tag(Tag::PopForm(_), _) => None,
                    TagOrSkip::Tag(tag, lock) => Some((index, tag, *lock)),
                    TagOrSkip::Skip(_) => None
                });

        if let Some((index, tag, lock)) = tags.nth(tag_index) {
            let inv_tag = tag.inverse();

            *tag = new_tag;

            if let Some(new_inv_tag) = new_tag.inverse() {
                let forward = match &tags_vec[index + 1] {
                    TagOrSkip::Tag(_, _) => 1,
                    TagOrSkip::Skip(_) => 2
                };

                if let Some(_) = inv_tag {
                    tags_vec[index + forward] = TagOrSkip::Tag(new_inv_tag, lock);
                } else {
                    tags_vec.insert(index + 1, TagOrSkip::Tag(new_inv_tag, lock));
                }
            } else {
                tags_vec.remove(index + 1);
            }
        }
    }

    pub fn get_mut_skip(&mut self, index: usize) -> Option<(usize, &mut u32)> {
        self.text
            .tags
            .vec()
            .iter_mut()
            .filter_map(|tag_or_skip| match tag_or_skip {
                TagOrSkip::Skip(skip) => Some(skip),
                TagOrSkip::Tag(..) => None
            })
            .scan(0, |accum, skip| {
                let prev_accum = *accum;
                *accum += *skip as usize;
                Some((prev_accum, skip))
            })
            .nth(index)
    }

    fn add_to_last_skip(&mut self, edit_len: u32) {
        let tags_vec = self.text.tags.vec();
        let Some(tag_or_skip) = tags_vec.last_mut() else {
            return;
        };

        match tag_or_skip {
            TagOrSkip::Tag(Tag::PopForm(_), _) => {
                let prev = tags_vec.len() - 1;
                if let Some(TagOrSkip::Skip(skip)) = tags_vec.get_mut(prev) {
                    *skip += edit_len;
                } else {
                    tags_vec.insert(prev, TagOrSkip::Skip(edit_len));
                }
            }
            TagOrSkip::Tag(..) => tags_vec.push(TagOrSkip::Skip(edit_len)),
            TagOrSkip::Skip(skip) => *skip += edit_len
        }
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.swappables.clear();
    }

    pub fn truncate(&mut self, range_index: usize) {
        let tags_vec = self.text.tags.vec();

        let Some(swap_index) = self.swappables.get(range_index) else {
            return;
        };
        let (cutoff, index) = tags_vec
            .iter_mut()
            .enumerate()
            .filter_map(|(index, tag_or_skip)| match tag_or_skip {
                TagOrSkip::Skip(skip) => Some((index, skip)),
                TagOrSkip::Tag(..) => None
            })
            .scan(0, |accum, (index, skip)| {
                let prev_accum = *accum;
                *accum += *skip as usize;
                Some((prev_accum, index))
            })
            .nth(*swap_index)
            .unwrap();

        self.swappables.truncate(range_index);
        self.text.inner.string().truncate(cutoff);
        tags_vec.truncate(index);
    }

    pub fn ranges_len(&self) -> usize {
        self.swappables.len()
    }

    pub fn text(&self) -> &Text<U> {
        &self.text
    }
}

/// The text in a given area.
pub struct Text<U>
where
    U: Ui + ?Sized
{
    pub inner: InnerText,
    pub tags: Tags,
    lock: Lock,
    _replacements: Vec<(Vec<Text<U>>, RangeInclusive<usize>, bool)>,
    _readers: Vec<Box<dyn MutTextReader<U>>>
}

impl<U> std::fmt::Debug for Text<U>
where
    U: Ui + ?Sized
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Text")
            .field("inner", &self.inner)
            .field("tags", &self.tags)
            .field("lock", &self.lock)
            .finish()
    }
}

// TODO: Properly implement _replacements.
impl<U> Text<U>
where
    U: Ui
{
    pub fn default_string() -> Self {
        let mut tags = Tags::default_vec();
        let lock = tags.get_lock();
        Text {
            inner: InnerText::String(String::default()),
            tags,
            lock,
            _replacements: Vec::new(),
            _readers: Vec::new()
        }
    }

    pub fn default_rope() -> Self {
        let mut tags = Tags::default_rope();
        let lock = tags.get_lock();
        Text {
            inner: InnerText::Rope(Rope::default()),
            tags,
            lock,
            _replacements: Vec::new(),
            _readers: Vec::new()
        }
    }

    pub fn new_string(string: impl ToString) -> Self {
        let inner = InnerText::String(string.to_string());
        let mut tags = Tags::new(&inner);
        let lock = tags.get_lock();
        Text {
            inner,
            tags,
            lock,
            _replacements: Vec::new(),
            _readers: Vec::new()
        }
    }

    pub fn new_rope(string: impl ToString) -> Self {
        let inner = InnerText::Rope(Rope::from(string.to_string()));
        let mut tags = Tags::new(&inner);
        let lock = tags.get_lock();
        Text {
            inner,
            tags,
            lock,
            _replacements: Vec::new(),
            _readers: Vec::new()
        }
    }

    /// Prints the contents of a given area in a given `EndNode`.
    pub(crate) fn print(
        &self, label: &mut U::Label, print_info: PrintInfo, print_cfg: PrintCfg,
        palette: &FormPalette
    ) {
        let cur_char = {
            let first_line = self.inner.char_to_line(print_info.first_char);
            self.inner.line_to_char(first_line)
        };

        let text_iter = self.iter_range(cur_char..);

        label.print(text_iter, print_info, print_cfg, palette);
    }

    /// Merges `String`s with the body of text, given a range to
    /// replace.
    fn replace_range(&mut self, old: Range<usize>, edit: impl AsRef<str>) {
        let edit = edit.as_ref();
        let edit_len = edit.chars().count();
        let new = old.start..(old.start + edit_len);

        self.inner.replace(old.start..old.end, edit);

        if old != new {
            self.tags.transform_range(old, new);
        }
    }

    pub(crate) fn apply_change(&mut self, change: &Change) {
        self.replace_range(change.taken_range(), &change.added_text);
    }

    pub(crate) fn undo_change(&mut self, change: &Change, chars: isize) {
        let start = change.start.saturating_add_signed(chars);
        let end = change.added_end().saturating_add_signed(chars);
        self.replace_range(start..end, &change.taken_text);
    }

    pub fn inner(&self) -> &InnerText {
        &self.inner
    }

    pub fn is_empty(&self) -> bool {
        self.inner.len_chars() == 0
    }

    /// Removes the tags for all the cursors, used before they are
    /// expected to move.
    pub(crate) fn add_cursor_tags(&mut self, cursors: &[Cursor], main_index: usize) {
        for (index, cursor) in cursors.iter().enumerate() {
            let Range { start, end } = cursor.range();
            let (caret_tag, start_tag, end_tag) = cursor_tags(index == main_index);

            let pos_list =
                [(start, start_tag), (end, end_tag), (cursor.caret().true_char(), caret_tag)];

            let no_selection = if start == end { 2 } else { 0 };

            for (pos, tag) in pos_list.into_iter().skip(no_selection) {
                self.tags.insert(pos, tag, self.lock);
            }
        }
    }

    /// Adds the tags for all the cursors, used after they are
    /// expected to have moved.
    pub(crate) fn remove_cursor_tags(&mut self, cursors: &[Cursor]) {
        for cursor in cursors.iter() {
            let Range { start, end } = cursor.range();
            let skip = if start == end { 1 } else { 0 };
            for ch_index in [start, end].into_iter().skip(skip) {
                self.tags.remove_on(ch_index, self.lock);
            }
        }
    }

    pub fn len_chars(&self) -> usize {
        self.inner.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.inner.len_lines()
    }

    pub fn len_bytes(&self) -> usize {
        self.inner.len_bytes()
    }

    fn clear(&mut self) {
        self.inner.clear();
        self.tags.clear();
    }
}

// Iterator methods.
impl<U> Text<U>
where
    U: Ui + ?Sized
{
    pub fn iter(&self) -> impl Iterator<Item = (usize, TextBit)> + '_ {
        let chars = self.inner.chars_at(0);
        let tags = self.tags.iter_at(0).peekable();

        TextIter::new(self, chars, tags, 0)
    }

    pub fn iter_line(&self, line: usize) -> impl Iterator<Item = (usize, TextBit)> + '_ {
        let start = self.inner.line_to_char(line);
        let end = self.inner.line_to_char(line + 1);
        let chars = self.inner.chars_at(start).take(end - start);
        let tags = self.tags.iter_at(start).take_while(move |(index, _)| *index < end).peekable();

        TextIter::new(self, chars, tags, start)
    }

    pub fn iter_range(
        &self, range: impl RangeBounds<usize>
    ) -> impl Iterator<Item = (usize, TextBit)> + '_ {
        let start = match range.start_bound() {
            std::ops::Bound::Included(start) => *start,
            std::ops::Bound::Excluded(start) => *start + 1,
            std::ops::Bound::Unbounded => 0
        };

        let end = match range.end_bound() {
            std::ops::Bound::Included(end) => end + 1,
            std::ops::Bound::Excluded(end) => *end,
            std::ops::Bound::Unbounded => self.inner.len_chars()
        };

        let chars = self.inner.chars_at(start).take(end - start);
        let tags = self.tags.iter_at(start).take_while(move |(index, _)| *index < end).peekable();

        TextIter::new(self, chars, tags, start)
    }
}

/// A part of the [`Text`], can be a [`char`] or a [`Tag`].
pub enum TextBit {
    Tag(Tag),
    Char(char)
}

impl std::fmt::Debug for TextBit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextBit::Char(char) => f.write_fmt(format_args!("Char({})", char)),
            TextBit::Tag(tag) => f.write_fmt(format_args!("Tag({:?})", tag))
        }
    }
}

impl TextBit {
    pub fn as_char(&self) -> Option<&char> {
        if let Self::Char(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

/// An [`Iterator`] over the [`TextBit`]s of the [`Text`].
///
/// This is useful for both printing and measurement of [`Text`], and
/// can incorporate string replacements as part of its design.
pub struct TextIter<'a, U, Ci, Ti>
where
    U: Ui + ?Sized,
    Ci: Iterator<Item = char> + 'a,
    Ti: Iterator<Item = (usize, Tag)> + 'a
{
    text: &'a Text<U>,
    chars: Ci,
    tags: Peekable<Ti>,
    cur_char: usize
}

impl<'a, U, Ci, Ti> TextIter<'a, U, Ci, Ti>
where
    U: Ui + ?Sized,
    Ci: Iterator<Item = char> + 'a,
    Ti: Iterator<Item = (usize, Tag)> + 'a
{
    pub fn new(text: &'a Text<U>, chars: Ci, tags: Peekable<Ti>, cur_char: usize) -> Self {
        Self {
            text,
            chars,
            tags,
            cur_char
        }
    }
}

impl<U, CI, TI> Iterator for TextIter<'_, U, CI, TI>
where
    U: Ui + ?Sized,
    CI: Iterator<Item = char>,
    TI: Iterator<Item = (usize, Tag)>
{
    type Item = (usize, TextBit);

    fn next(&mut self) -> Option<Self::Item> {
        // `<=` because some `Tag`s may be triggered before printing.
        if let Some(&(index, tag)) = self.tags.peek().filter(|(index, _)| *index <= self.cur_char) {
            self.tags.next();
            Some((index, TextBit::Tag(tag)))
        } else if let Some(char) = self.chars.next() {
            self.cur_char += 1;
            Some((self.cur_char - 1, TextBit::Char(char)))
        } else {
            None
        }
    }
}

impl<'a, U, Ci, Ti> Clone for TextIter<'a, U, Ci, Ti>
where
    U: Ui + ?Sized,
    Ci: Iterator<Item = char> + Clone + 'a,
    Ti: Iterator<Item = (usize, Tag)> + Clone + 'a
{
    fn clone(&self) -> Self {
        TextIter {
            text: &self.text,
            chars: self.chars.clone(),
            tags: self.tags.clone(),
            cur_char: self.cur_char
        }
    }
}

// NOTE: The defaultness in here, when it comes to `last_main`, may
// cause issues in the future.
/// Information about how to print the file on the `Label`.
#[derive(Default, Debug, Clone, Copy)]
pub struct PrintInfo {
    /// The index of the first [char] that should be printed on the
    /// screen.
    first_char: usize,
    /// How shifted the text is to the left.
    x_shift: usize,
    /// The last position of the main cursor.
    last_main: Pos
}

impl PrintInfo {
    /// Scrolls up until the gap between the main cursor and the top
    /// of the widget is equal to `config.scrolloff.y_gap`.
    fn scroll_up_to_gap<U>(&mut self, target: Pos, text: &Text<U>, label: &U::Label, cfg: &PrintCfg)
    where
        U: Ui
    {
        let max_dist = cfg.scrolloff.y_gap + 1;

        let mut accum = 0;
        for index in (0..=target.true_row()).rev() {
            let line_char = text.inner.line_to_char(index);
            // After the first line, will always be whole.
            let line = text.iter_line(index).take(target.true_char() - line_char);

            accum += 1 + label.wrap_count(line, cfg);
            if accum >= max_dist && line_char < self.first_char {
                // `max_dist - accum` is the amount of wraps that should be offscreen.
                let line = text.iter_line(index).take((target.true_char() - line_char).max(1));
                self.first_char = label.char_at_wrap(line, accum - max_dist, cfg).unwrap();
                break;
            } else if accum >= max_dist {
                break;
            }
        }

        // This means we can't scroll up anymore.
        if accum < max_dist {
            self.first_char = 0;
        }
    }

    /// Scrolls down until the gap between the main cursor and the
    /// bottom of the widget is equal to `config.scrolloff.y_gap`.
    fn scroll_down_to_gap<U>(
        &mut self, target: Pos, text: &Text<U>, label: &U::Label, cfg: &PrintCfg
    ) where
        U: Ui
    {
        let max_dist = label.area().height() - cfg.scrolloff.y_gap;

        let mut accum = 0;
        for index in (0..=target.true_row()).rev() {
            let line_char = text.inner.line_to_char(index);
            // After the first line, will always be whole.
            let line = text.iter_line(index).take(target.true_char() - line_char);

            accum += 1 + label.wrap_count(line, cfg);
            if accum >= max_dist {
                // `accum - gap` is the amount of wraps that should be offscreen.
                let line = text.iter_line(index).take((target.true_char() - line_char).max(1));
                self.first_char = label.char_at_wrap(line, accum - max_dist, cfg).unwrap();
                break;
            // We have reached the top of the screen before the accum
            // equaled gap. This means that no scrolling
            // actually needs to take place.
            } else if line_char < self.first_char {
                break;
            }
        }
    }

    /// Scrolls the file horizontally, usually when no wrapping is
    /// being used.
    fn scroll_hor_to_gap<U>(
        &mut self, target: Pos, text: &Text<U>, label: &U::Label, cfg: &PrintCfg
    ) where
        U: Ui
    {
        let max_dist = label.area().width() - cfg.scrolloff.x_gap;

        let line_char = text.inner.line_to_char(target.true_row());
        let line = text.iter_line(target.true_row()).take(target.true_char() - line_char);

        let target_dist = label.get_width(line, cfg);
        self.x_shift = target_dist.saturating_sub(max_dist);
    }

    /// Updates the print info, according to a [`Config`]'s
    /// specifications.
    pub fn update<U>(&mut self, target: Pos, text: &Text<U>, label: &U::Label, cfg: &PrintCfg)
    where
        U: Ui
    {
        if let WrapMethod::NoWrap = cfg.wrap_method {
            self.scroll_hor_to_gap::<U>(target, text, label, cfg);
        }

        if target < self.last_main {
            self.scroll_up_to_gap::<U>(target, text, label, cfg);
        } else if target > self.last_main {
            self.scroll_down_to_gap::<U>(target, text, label, cfg);
        }

        self.last_main = target;
    }

    pub fn first_char(&self) -> usize {
        self.first_char
    }

    pub fn x_shift(&self) -> usize {
        self.x_shift
    }

    pub fn last_main(&self) -> Pos {
        self.last_main
    }
}

fn cursor_tags(is_main: bool) -> (Tag, Tag, Tag) {
    if is_main {
        (Tag::MainCursor, Tag::PushForm(MAIN_SEL), Tag::PopForm(MAIN_SEL))
    } else {
        (Tag::MainCursor, Tag::PushForm(EXTRA_SEL), Tag::PopForm(EXTRA_SEL))
    }
}

/// If and how to wrap lines at the end of the screen.
#[derive(Default, Debug, Copy, Clone)]
pub enum WrapMethod {
    Width,
    Capped(usize),
    Word,
    #[default]
    NoWrap
}

impl WrapMethod {
    /// Returns `true` if the wrap method is [`NoWrap`].
    ///
    /// [`NoWrap`]: WrapMethod::NoWrap
    #[must_use]
    pub fn is_no_wrap(&self) -> bool {
        matches!(self, Self::NoWrap)
    }
}

/// Where the tabs are placed on screen, can be regular or varied.
#[derive(Debug, Clone)]
pub enum TabStops {
    /// The same lenght for every tab.
    Regular(usize),
    /// Varying lenghts for different tabs.
    Varied(Vec<usize>)
}

impl TabStops {
    pub fn spaces_at(&self, x: usize) -> usize {
        match self {
            TabStops::Regular(step) => step - (x % step),
            TabStops::Varied(steps) => steps.iter().find(|&s| *s > x).expect("Not enough steps") - x
        }
    }
}

impl Default for TabStops {
    fn default() -> Self {
        TabStops::Regular(4)
    }
}

/// Wheter to show the new line or not.
#[derive(Default, Debug, Clone, Copy)]
pub enum NewLine {
    #[default]
    /// Never show new lines.
    Blank,
    /// Show the given character on every new line.
    AlwaysAs(char),
    /// Show the given character only when there is whitespace at end
    /// of the line.
    AfterSpaceAs(char)
}

impl NewLine {
    pub fn new_line_char(&self, last_ch: char) -> char {
        match self {
            NewLine::Blank => ' ',
            NewLine::AlwaysAs(char) => *char,
            NewLine::AfterSpaceAs(char) => {
                if last_ch.is_whitespace() {
                    *char
                } else {
                    ' '
                }
            }
        }
    }
}

// Pretty much only exists because i wanted one of these with
// usize as its builtin type.
#[derive(Debug, Copy, Clone)]
pub struct ScrollOff {
    pub y_gap: usize,
    pub x_gap: usize
}

impl Default for ScrollOff {
    fn default() -> Self {
        ScrollOff { y_gap: 3, x_gap: 3 }
    }
}

#[derive(Clone)]
pub struct WordChars(Vec<RangeInclusive<char>>);

impl WordChars {
    pub fn new(ranges: Vec<RangeInclusive<char>>) -> Self {
        let word_chars = WordChars(ranges);

        assert!(
            ![' ', '\t', '\n'].into_iter().any(|char| word_chars.contains(char)),
            "WordChars cannot contain ' ', '\\n' or '\\t'."
        );

        word_chars
    }

    pub fn contains(&self, char: char) -> bool {
        self.0.iter().any(|chars| chars.contains(&char))
    }
}

/// Configuration options for printing.
#[derive(Clone)]
pub struct PrintCfg {
    /// How to wrap the file.
    pub wrap_method: WrapMethod,
    /// Wether to indent wrapped lines or not.
    pub indent_wrap: bool,
    /// Which places are considered a "tab stop".
    pub tab_stops: TabStops,
    /// Wether (and how) to show new lines.
    pub new_line: NewLine,
    /// The horizontal and vertical gaps between the main
    /// cursor and the edges of a [`Label`][crate::ui::Label].
    pub scrolloff: ScrollOff,
    // NOTE: This is relevant for printing with `WrapMethod::Word`.
    /// Characters that are considered to be part of a word.
    pub word_chars: WordChars
}

impl Default for PrintCfg {
    fn default() -> Self {
        PrintCfg {
            wrap_method: WrapMethod::default(),
            indent_wrap: true,
            new_line: NewLine::default(),
            tab_stops: TabStops::Regular(4),
            scrolloff: ScrollOff::default(),
            word_chars: WordChars::new(vec!['A'..='Z', 'a'..='z', '_'..='_'])
        }
    }
}
