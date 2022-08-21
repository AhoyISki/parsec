use std::cmp::min;

use crossterm::style::{ContentStyle, Stylize};
use regex::Regex;
use unicode_width::UnicodeWidthChar;

use crate::{
    action::{get_byte, History, TextRange},
    config::{FileOptions, PrintOptions, TabPlaces, WrapMethod},
    cursor::{get_byte_distance, FileCursor, TextPos},
    layout::PrintInfo,
    tags::{CharTag, Form, LineFlags, LineInfo, Matcher, TagManager},
    ui::{Area, ChildNode},
};

// TODO: move this to a more general file.
/// A line in the text file.
pub struct TextLine {
    /// The text on the line.
    text: String,

    /// Information about a line.
    pub(crate) info: LineInfo,
}

impl TextLine {
    /// Returns a new instance of `TextLine`.
    pub fn new(text: &str) -> TextLine {
        let info = LineInfo::default();

        TextLine { text: String::from(text), info }
    }

    /// Returns the line's indentation.
    pub fn indent(&self, tabs: &TabPlaces) -> usize {
        let mut indent_sum = 0;

        for ch in self.text.chars() {
            if ch == ' ' || ch == '\t' {
                indent_sum += get_char_width(ch, indent_sum, tabs);
            } else {
                break;
            }
        }

        indent_sum as usize
    }

    /// Returns the byte index of a given column.
    pub fn get_line_byte_at(&self, col: usize) -> usize {
        if self.info.line_flags.contains(LineFlags::PURE_ASCII) {
            col
        } else {
            self.text.char_indices().nth(col).unwrap_or((self.text.len(), ' ')).0
        }
    }

    /// Returns the visual distance to a certain column.
    pub fn get_distance_to_col(&self, col: usize, tabs: &TabPlaces) -> usize {
        let mut width = 0;

        if self.info.line_flags.contains(LineFlags::PURE_1_COL) {
            width = col
        } else {
            for ch in self.text.chars().take(col) {
                width += if ch == '\t' {
                    tabs.get_tab_len(width)
                } else {
                    UnicodeWidthChar::width(ch).unwrap_or(1)
                };
            }
        }

        width
    }

    /// Returns the column and byte found at visual distance from 0. Also returns any leftovers.
    ///
    /// The leftover number is positive if the width of the characters is greater (happens if the
    /// last checked character has a width greater than 1), and 0 otherwise.
    pub fn get_col_at_distance(&self, min_dist: usize, tabs: &TabPlaces) -> (usize, usize) {
        if self.info.line_flags.contains(LineFlags::PURE_1_COL) {
            if self.info.line_flags.contains(LineFlags::PURE_ASCII) {
                let byte = min(min_dist, self.text.len() - 1);

                (byte, 0)
            } else {
                match self.text.chars().enumerate().nth(min_dist) {
                    Some((col, _)) => (col, 0),
                    None => {
                        let count = self.text.chars().count();

                        (count - 1, 0)
                    }
                }
            }
        } else {
            let (mut col, mut distance) = (0, 0);

            let mut text_iter = self.text.chars().enumerate();

            while let Some((new_col, ch)) = text_iter.next() {
                col = new_col;

                if distance >= min_dist {
                    break;
                }

                distance += get_char_width(ch, distance, tabs);
            }

            (col, distance.saturating_sub(min_dist))
        }
    }

    // TODO: Eventually will include syntax highlighting, hover info, etc.
    /// Updates the information for a line in the file.
    ///
    /// Returns `true` if the screen needs a full refresh.
    pub fn update_line_info(&mut self, options: &FileOptions, width: usize) {
        self.info.line_flags.set(LineFlags::PURE_ASCII, self.text.is_ascii());
        self.info.line_flags.set(
            LineFlags::PURE_1_COL,
            !self.text.chars().any(|c| UnicodeWidthChar::width(c).unwrap_or(1) > 1 || c == '\t'),
        );

        if !matches!(options.wrap_method, WrapMethod::NoWrap) {
            let indent = self.indent(&options.tabs);
            let indent = if options.wrap_indent && indent < width { indent } else { 0 };

            // Clear all `WrapppingChar`s from `char_tags`.
            self.info.char_tags.retain(|(_, t)| !matches!(t, CharTag::WrapppingChar));

            let mut distance = 0;
            let mut indent_wrap = 0;

            // TODO: Add an enum parameter signifying the wrapping type.
            // Wrapping at the final character at the width of the area.
            if self.info.line_flags.contains(LineFlags::PURE_1_COL | LineFlags::PURE_ASCII) {
                distance = width;
                while distance < self.text.len() {
                    self.info.char_tags.insert((distance as u32, CharTag::WrapppingChar));

                    indent_wrap = indent;

                    distance += width - indent_wrap;
                }
            } else {
                for (index, ch) in self.text.char_indices() {
                    distance += get_char_width(ch, distance, &options.tabs);

                    if distance > width - indent_wrap {
                        distance = get_char_width(ch, distance, &options.tabs);

                        self.info.char_tags.insert((index as u32, CharTag::WrapppingChar));

                        indent_wrap = indent;
                    }
                }
            }
        };
    }

    /// Returns an iterator over the wrapping bytes of the line.
    pub fn wrap_iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.info
            .char_tags
            .vec()
            .iter()
            .filter(|(_, t)| matches!(t, CharTag::WrapppingChar))
            .map(|(c, _)| *c)
    }

    /// Returns how many characters are in the line.
    pub fn char_count(&self) -> usize {
        if self.info.line_flags.contains(LineFlags::PURE_ASCII) {
            self.text.len()
        } else {
            self.text.chars().count()
        }
    }

    // NOTE: It always prints at `x = 0`, `x` in pos is treated here as an `x_shift`.
    /// Prints a line in a given position, skipping `skip` characters.
    ///
    /// Returns the amount of wrapped lines that were printed.
    #[inline]
    pub(crate) fn print<A>(
        &self, node: &mut ChildNode<A>, x_shift: usize, skip: usize, options: &PrintOptions,
        forms: &[Form],
    ) -> bool
    where
        A: Area,
    {
        let (skip, d_x) = if let WrapMethod::NoWrap = options.wrap_method {
            // The leftover here represents the amount of characters that should not be printed,
            // for example, complex emoji may occupy several cells that should be empty, in the
            // case that part of the emoji is located before the first column.
            self.get_col_at_distance(x_shift, &options.tab_places)
        } else {
            (skip, 0)
        };
        let mut d_x = d_x as usize;

        (0..d_x).for_each(|_| node.area.print(' '));

        let char_width = |c, x| {
            if self.info.line_flags.contains(LineFlags::PURE_1_COL) {
                1
            } else {
                get_char_width(c, x, &options.tab_places)
            }
        };

        let text_iter = self.text.char_indices().skip_while(|&(b, _)| b < skip);

        let mut wraps = self.wrap_iter();
        let tags = &self.info.char_tags;

        // In the case where the amount of skipped characters is greater than the placement of
        // the first wrapped one, if `options.wrap_indent`, we need to indent the text
        // immediately, in order to print the text in the correct place. This will happen if
        // the top line wraps and has indentation.
        if let Some(col) = wraps.next() {
            if skip >= col as usize && options.wrap_indent {
                (0..self.indent(&options.tab_places)).for_each(|c| node.area.print(' '));
            }
        }

        // NOTE: This is a freakishly large number of tags to be in a single line.
        // NOTE: If a line you wrote has this many tags, frankly, you're a bad programmer.
        let pre_skip = if tags.vec().len() < 300 {
            0
        // If, somehow, `len >= 300`, we look back at 100 lines back, to complete any forms
        // that could possibly show up.
        } else {
            match tags.vec().iter().enumerate().find(|(_, (c, _))| (*c as usize) >= skip) {
                Some((first_shown_tag, _)) => first_shown_tag.saturating_sub(100),
                None => tags.vec().len().saturating_sub(100),
            }
        };

        // Iterating from 10 character tags back, until the first tag is printed.
        let tags_iter = tags.vec().iter().skip(pre_skip).take_while(|(c, _)| (*c as usize) < skip);

        for (_, tag) in tags_iter {
            if let &CharTag::PushForm(id) = tag {
                node.push_form(forms, id);
            } else if let &CharTag::PopForm(id) = tag {
                node.pop_form(id);
            }
        }

        // Every other tag will be iterated with the text.
        let mut tags_iter = tags.vec().iter().skip_while(|(c, _)| (*c as usize) < skip);
        let mut current_char_tag = tags_iter.next();

        let wrap_indent = self.indent(&options.tab_places);
        // If `wrap_indent >= area.width()`, indenting on wraps becomes impossible.
        let wrap_indent =
            if options.wrap_indent && wrap_indent < node.area.width() { wrap_indent } else { 0 };

        'a: for (byte, ch) in text_iter {
            let char_width = char_width(ch, d_x + x_shift);

            while let Some(&(tag_byte, tag)) = current_char_tag {
                if byte == tag_byte as usize {
                    current_char_tag = tags_iter.next();

                    if let CharTag::WrapppingChar = tag {
                        // If this is the first printed character of `top_line`, we don't wrap.
                        if d_x == 0 {
                            continue;
                        }

                        if !node.area.next_line() {
                            node.area.clear_form();
                            return false;
                        }

                        d_x = 0;

                        (0..wrap_indent).for_each(|_| node.area.print(' '));
                    } else if let CharTag::PrimaryCursor = tag {
                        node.area.place_primary_cursor();
                    } else if let CharTag::SecondaryCursor = tag {
                        node.area.place_secondary_cursor();
                    } else if let CharTag::PushForm(id) = tag {
                        node.push_form(forms, id);
                    } else if let CharTag::PopForm(id) = tag {
                        node.pop_form(id);
                    }
                } else {
                    break;
                }
            }

            d_x += char_width;
            if let WrapMethod::NoWrap = options.wrap_method {
                if d_x > node.area.width() {
                    break;
                }
            }

            if ch == '\t' {
                // `repeat()` would use string allocation (I think).
                (0..char_width).for_each(|_| node.area.print(' '));
            } else if ch == '\n' {
                node.area.print(' ');
            } else {
                node.area.print(ch);
            }
        }

        node.area.clear_form();
        if !node.area.next_line() { false } else { true }
    }

    ////////////////////////////////
    // Getters
    ////////////////////////////////
    pub fn text(&self) -> &str {
        &self.text.as_str()
    }
}

/// File text and cursors.
pub struct File<A>
where
    A: Area,
{
    /// The lines of the file.
    pub lines: Vec<TextLine>,

    /// Where on the file to start printing.
    print_info: PrintInfo,

    /// The area allocated to the file.
    pub area: A,

    /// The options related to files.
    pub options: FileOptions,

    /// The edtiting cursors on the file.
    pub cursors: Vec<FileCursor>,
    /// The index of the main cursor. The file "follows it".
    pub main_cursor: usize,

    /// The history of edits on this file.
    pub history: History,

    /// The manager for character tags and file flag mutation.
    tag_manager: TagManager,
}

impl<A> File<A>
where
    A: Area,
{
    /// Returns a new instance of `File<T>`, given a `Vec<FileLine>`.
    pub fn new(lines: Vec<&str>, options: FileOptions, area: A) -> File<A> {
        let lines = lines.iter().map(|l| TextLine::new(l)).collect();

        let mut tag_manager = TagManager::new();
        let matcher = Matcher::Regex(Regex::new(r"\{|\}|\[|\]|\(|\)").unwrap());
        tag_manager.push_word(matcher, Some(0), false, 0);

        let matcher = Matcher::Regex(Regex::new(r"filesystem").unwrap());
        let id = tag_manager.push_word(matcher, Some(1), false, 0);

        let matcher = Matcher::Regex(Regex::new(r"sys").unwrap());
        tag_manager.push_word(matcher, Some(2), true, id);

        let matcher_start = Matcher::Regex(Regex::new(r"<").unwrap());
        let matcher_end = Matcher::Regex(Regex::new(r">").unwrap());
        let id = tag_manager.push_bounds([matcher_start, matcher_end], Some(3), true, 0);

        let matcher = Matcher::Regex(Regex::new(r"asd").unwrap());
        tag_manager.push_word(matcher, Some(4), false, id);

        let matcher = Matcher::Regex(Regex::new(r"tem").unwrap());
        tag_manager.push_word(matcher, Some(1), false, 0);

        let matcher = Matcher::Regex(Regex::new(r"y").unwrap());
        tag_manager.push_word(matcher, Some(0), false, 0);

        tag_manager.push_form(ContentStyle::new().red(), false);
        tag_manager.push_form(ContentStyle::new().green(), false);
        tag_manager.push_form(ContentStyle::new().on_white(), false);
        tag_manager.push_form(ContentStyle::new().blue(), false);
        tag_manager.push_form(ContentStyle::new().on_yellow(), false);

        let mut file = File {
            lines,
            // TODO: Remember last session.
            print_info: PrintInfo { top_line: 0, top_wraps: 0, x_shift: 0 },
            area,
            options,
            cursors: Vec::new(),
            main_cursor: 0,
            history: History::new(),
            tag_manager,
        };

        file.cursors.push(FileCursor::new(
            TextPos { col: 0, byte: 0, line: 0 },
            &file.lines,
            &file.options.tabs,
        ));

        let start = TextPos { line: 0, col: 0, byte: 0 };

        let col = file.lines.last().unwrap().text.chars().count();
        let byte = file.lines.iter().map(|l| l.text.len()).sum();
        let end = TextPos { line: file.lines.len() - 1, col, byte };

        let range = TextRange { start, end };

        let line_infos = file.tag_manager.match_text_range(file.lines.as_slice(), range, end, true);

        for (line_info, line_num) in line_infos {
            file.lines[line_num].info = line_info;
            file.lines[line_num].update_line_info(&file.options, file.area.width());
        }

        file
    }

    /// Updates the file's scrolling and checks if it has scrolled.
    pub fn update_print_info(&mut self) {
        let info = &mut self.print_info;
        let scrolloff = self.options.scrolloff;

        let main_cursor = self.cursors.get(self.main_cursor).unwrap();
        let current = main_cursor.current();
        let target = main_cursor.target();

        // Vertical scroll check:
        if let WrapMethod::NoWrap = self.options.wrap_method {
            // If there is no wrapping, the check is much simpler, just check if the distance to
            // `info.top_line` is within `scrolloff.d_y` and `self.area.height() + scrolloff.d_y`,
            // If it's not, subtract the difference and add/subtract it from `info.top_line`.
            if target.line > info.top_line + self.area.height() - scrolloff.d_y {
                info.top_line += target.line + scrolloff.d_y - info.top_line - self.area.height();
            } else if target.line < info.top_line + scrolloff.d_y && info.top_line != 0 {
                info.top_line -= (info.top_line + scrolloff.d_y) - target.line;
            }
        } else {
            let line = &self.lines[current.line];
            let current_byte = line.get_line_byte_at(current.col);
            let current_wraps = line.wrap_iter().take_while(|&c| c <= current_byte as u32).count();

            let line = &self.lines[target.line];
            let target_byte = line.get_line_byte_at(target.col);
            let target_wraps = line.wrap_iter().take_while(|&c| c <= target_byte as u32).count();

            let lines_iter = self.lines[..=target.line].iter_mut();

            let mut d_y = target_wraps;

            // Case where we're moving down.
            if target.line > current.line
                || (target.line == current.line && target_wraps > current_wraps)
            {
                let mut top_offset = 0;
                for (index, line) in lines_iter.enumerate().rev() {
                    if index != target.line {
                        d_y += 1 + line.wrap_iter().count();
                    }

                    if index == info.top_line {
                        top_offset = info.top_wraps
                    };

                    // If this happens first, that means the distance between `target.line` and
                    // `info.top_line` is greater than allowed height of the cursor.
                    if d_y >= self.area.height() + top_offset - scrolloff.d_y {
                        info.top_line = index;
                        // If this equals 0, that means the distance has matched up perfectly,
                        // i.e. the distance between the new `info.top_line` is exactly what's
                        // needed for the full height. If it's greater than 0, `info.top_wraps`
                        // needs to adjust where the line actually begins to match up.
                        info.top_wraps = d_y + scrolloff.d_y - self.area.height();

                        break;
                    }

                    // If this happens first, we're in the middle of the screen, and don't need
                    // to change `info.top_line`.
                    if index == info.top_line {
                        break;
                    }
                }
            // Case where we're moving up.
            // `info.top_line` after implementing line folding.
            } else {
                // Set this flag immediately in this case, because the first line that checks out
                // will definitely be `info.top_line`.
                let mut needs_new_top_line = target.line < info.top_line;
                for (index, line) in lines_iter.enumerate().rev() {
                    // Add the vertical distance, as 1 line plus the times it wraps around.
                    // `target.line` was already added as `target_wraps`.
                    if index != target.line {
                        d_y += 1 + line.wrap_iter().count();
                    };

                    if index == info.top_line {
                        // This means we ran into the top line too early, and must scroll up.
                        // `info.top_wraps` is here because the top line might be partially off
                        // screen, and we'd be "comparing" only against the shown wraps, which is
                        // incorrect
                        if d_y < scrolloff.d_y + info.top_wraps {
                            needs_new_top_line = true;
                        // If this happens, we ran into `info.top_line` while below `scrolloff.y`,
                        // this means we're in the "middle" of the screen, and don't need to
                        // scroll.
                        } else if !needs_new_top_line {
                            break;
                        }
                    }

                    // In this case, we have either passed through `info.top_line` while too close,
                    // or not passed through, so a new `info.top_line` is behind the old one.
                    if needs_new_top_line && (d_y >= scrolloff.d_y || index == 0) {
                        info.top_line = index;
                        info.top_wraps = d_y.saturating_sub(scrolloff.d_y);

                        break;
                    }
                }
            }
        }

        // Horizontal scroll check, done only when the screen can scroll horizontally:
        if let WrapMethod::NoWrap = self.options.wrap_method {
            let target_line = &self.lines[target.line];
            let distance = target_line.get_distance_to_col(target.col, &self.options.tabs);

            // If the distance is greater, it means that the cursor is out of bounds.
            if distance > info.x_shift + self.area.width() - scrolloff.d_x {
                // Shift by the amount required to keep the cursor in bounds.
                info.x_shift = distance + scrolloff.d_x - self.area.width();
            // Check if `info.x_shift` is already at 0, if it is, no scrolling is dones.
            } else if distance < info.x_shift + scrolloff.d_x {
                info.x_shift = distance.saturating_sub(scrolloff.d_x);
            }
        }
    }

    /// Applies a splice to the file.
    pub fn splice_edit<S>(&mut self, edit: Vec<S>, old_range: TextRange)
    where
        S: ToString,
    {
        let (edits, new_range) = self.history.add_change(&mut self.lines, edit, old_range);

        let edits: Vec<TextLine> = edits.iter().map(|l| TextLine::new(l)).collect();
        self.lines.splice(old_range.lines(), edits);

        match_range(&mut self.lines, new_range, &self.area, &self.options, &mut self.tag_manager);

        for cursor in &mut self.cursors {
            let new_pos = if cursor.target() >= old_range.end {
                let mut new_pos = cursor.target();
                new_pos = new_pos.col_add(new_range.end).col_sub(old_range.end);
                new_pos.line += new_range.end.line;
                new_pos.line -= old_range.end.line;

                new_pos
            } else if cursor.target() > old_range.start {
                min(cursor.target(), new_range.end)
            } else {
                continue;
            };

            cursor.move_to(new_pos, &self.lines, &self.options)
        }
    }

    /// Undoes the last moment in history.
    pub fn undo(&mut self) {
        let (splices, print_info) = match self.history.undo(&mut self.lines) {
            Some((changes, print_info)) => (changes, print_info),
            None => return,
        };
        self.print_info = print_info.unwrap_or(self.print_info);

        let mut cursors = self.cursors.iter_mut();
        let mut new_cursors = Vec::new();

        for splice in &splices {
            if let Some(cursor) = cursors.next() {
                cursor.move_to(splice.taken_end(), &self.lines, &self.options);
            } else {
                new_cursors.push(FileCursor::new(
                    splice.taken_end(),
                    &self.lines,
                    &self.options.tabs,
                ));
            }

            let range = TextRange { start: splice.start(), end: splice.taken_end() };
            match_range(&mut self.lines, range, &self.area, &self.options, &mut self.tag_manager);
        }

        self.cursors.extend(new_cursors);
    }

    /// Re-does the last moment in history.
    pub fn redo(&mut self) {
        let (splices, print_info) = match self.history.redo(&mut self.lines) {
            Some((changes, print_info)) => (changes, print_info),
            None => return,
        };
        self.print_info = print_info.unwrap_or(self.print_info);

        let mut cursors = self.cursors.iter_mut();
        let mut new_cursors = Vec::new();

        for splice in &splices {
            if let Some(cursor) = cursors.next() {
                cursor.move_to(splice.added_end(), &self.lines, &self.options);
            } else {
                new_cursors.push(FileCursor::new(
                    splice.added_end(),
                    &self.lines,
                    &self.options.tabs,
                ));
            }

            let range = TextRange { start: splice.start(), end: splice.added_end() };
            match_range(&mut self.lines, range, &self.area, &self.options, &mut self.tag_manager);
        }

        self.cursors.extend(new_cursors);
    }

    /// Prints the file, according to its current position.
    pub fn print_file(&mut self) {
        // Checks if the main cursor's position change has caused the line to scroll.
        self.update_print_info();

        let main_cursor = self.cursors.get(self.main_cursor).unwrap();
        let limit_line = min(main_cursor.target().line + self.area.height(), self.lines.len() - 1);
        let mut start = TextPos { line: limit_line, col: 0, byte: main_cursor.target().byte };
        start.byte += get_byte_distance(&self.lines, main_cursor.target(), start) as usize;
        let target_line = &self.lines[limit_line];
        let range = TextRange {
            start,
            end: TextPos {
                byte: start.byte + target_line.text.len(),
                col: target_line.char_count(),
                ..start
            },
        };

        match_range(&mut self.lines, range, &self.area, &self.options, &mut self.tag_manager);

        let current = self.cursors.get(self.main_cursor).unwrap().current();
        let char_tags = &mut self.lines.get_mut(current.line).unwrap().info.char_tags;
        char_tags.retain(|(_, t)| !matches!(t, CharTag::PrimaryCursor));

        let target = self.cursors.get(self.main_cursor).unwrap().target();

        let line = &mut self.lines[target.line];
        let byte = line.get_line_byte_at(target.col);
        line.info.char_tags.insert((byte as u32, CharTag::PrimaryCursor));

        // Updates the information for each cursor in the file.
        self.cursors.iter_mut().for_each(|c| c.update());

        let info = self.print_info;
        let forms = self.tag_manager.forms();

        // The line at the top of the screen and the amount of hidden columns.
        let skip = if info.top_wraps > 0 {
            let line = self.lines.get(info.top_line).unwrap();
            line.wrap_iter().nth(info.top_wraps - 1).unwrap() as usize
        } else {
            0
        };

        // If the file has scrolled, reprint the whole screen.
        let mut y = 0;

        // Prints the first line and updates where to print next.
        let mut lines_iter = self.lines.iter();
        let top_line = lines_iter.nth(info.top_line).unwrap();
        y += top_line.print(&mut self.area, info.x_shift, y, skip, &self.options, forms);

        // Prints the remaining lines
        while let Some(line) = lines_iter.next() {
            if y as usize > self.area.height() {
                break;
            }
            y += line.print(&mut self.area, info.x_shift, y, 0, &self.options, forms);
        }

        self.area.clear_form();
    }

    ////////////////////////////////
    // Getters
    ////////////////////////////////
    pub fn print_info(&self) -> PrintInfo {
        self.print_info
    }
}

pub fn get_char_width(ch: char, col: usize, tabs: &TabPlaces) -> usize {
    if ch == '\t' { tabs.get_tab_len(col) } else { UnicodeWidthChar::width(ch).unwrap_or(1) }
}

fn match_range<A>(
    lines: &mut Vec<TextLine>, range: TextRange, area: &A, options: &FileOptions,
    tag_manager: &mut TagManager,
) where
    A: Area,
{
    let max_line_num = min(range.end.line + area.height(), lines.len() - 1);
    let max_line = &lines[max_line_num];
    let mut max_pos =
        TextPos { line: max_line_num, col: max_line.char_count(), byte: range.end.byte };

    max_pos.byte = (max_pos.byte as isize + get_byte_distance(lines, range.end, max_pos)) as usize;

    let start = TextPos {
        line: range.start.line,
        col: 0,
        byte: range.start.byte - get_byte(&lines[range.start.line].text, range.start.col),
    };

    let len = lines[range.end.line].text().len();
    let end = TextPos {
        line: range.end.line,
        col: lines[range.end.line].char_count(),
        byte: range.start.byte - get_byte(&lines[range.start.line].text, range.start.col) + len,
    };

    let range = TextRange { start, end };

    let line_infos = tag_manager.match_text_range(lines.as_slice(), range, max_pos, true);

    for (line_info, line_num) in line_infos {
        lines[line_num].info = line_info;
        lines[line_num].update_line_info(options, area.width());
    }
}
