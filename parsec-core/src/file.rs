use std::cmp::{max, min};

use regex::Regex;
use unicode_width::UnicodeWidthChar;

use crate::cursor::{FileCursor, TextPos};
use crate::tags::{Form, LineFlags};
use crate::{
    action::{History, TextRange},
    config::{FileOptions, TabPlaces, WrapMethod},
    output::{OutputArea, OutputPos, PrintInfo},
    tags::{CharTag, CharTags},
};

// TODO: move this to a more general file.
/// A line in the text file.
pub struct TextLine {
    /// Which columns on the line should wrap around.
    char_tags: CharTags,

    /// The text on the line.
    text: String,
    line_flags: LineFlags,
}

impl TextLine {
    /// Returns a new instance of `TextLine`.
    pub fn new(text: &str) -> TextLine {
        let char_tags = CharTags::new();

        TextLine { char_tags, text: String::from(text), line_flags: LineFlags::empty() }
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
    pub fn get_byte_at(&self, col: usize) -> usize {
        if self.line_flags.contains(LineFlags::PURE_ASCII) {
            col
        } else {
            self.text.char_indices().nth(col).unwrap_or((self.text.len(), ' ')).0
        }
    }

    /// Returns the visual distance to a certain column.
    pub fn get_distance_to_col(&self, col: usize, tabs: &TabPlaces) -> usize {
        let mut width = 0;

        if self.line_flags.contains(LineFlags::PURE_1_COL) {
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

    /// Returns the column found at a certain visual distance from 0. Also returns any leftovers.
    ///
    /// The leftover number is positive if the width of the characters is greater (happens if the
    /// last checked character has a width greater than 1), and 0 otherwise.
    pub fn get_col_at_distance(&self, min_dist: usize, tabs: &TabPlaces) -> (usize, usize) {
        let (mut col, mut distance) = (0, 0);

        if self.line_flags.contains(LineFlags::PURE_1_COL) {
            (col, distance) = if self.line_flags.contains(LineFlags::PURE_ASCII) {
                // The second one is `min()` because if `self.text.len() < min_dist`, it will
                // overflow.
                (min(min_dist, self.text.len()), min(self.text.len() - min_dist, 0))
            } else {
                match self.text.chars().enumerate().nth(min_dist) {
                    Some((col, _)) => (col, 0),
                    None => {
                        let count = self.text.chars().count();
                        (count, min_dist - count)
                    }
                }
            }
        } else {
            let mut text_iter = self.text.chars().enumerate();

            // NOTE: This looks really stupid.
            while let (Some((new_col, ch)), true) = (text_iter.next(), distance < min_dist) {
                distance += get_char_width(ch, distance, tabs);
                col = new_col + 1;
            }
        }

        (col, distance.saturating_sub(min_dist))
    }

    /// Parses the wrapping of a single line.
    ///
    /// Returns `true` if the amount of wrapped lines has changed.
    pub fn parse_wrapping(&mut self, width: usize, options: &FileOptions) -> bool {
        let indent = self.indent(&options.tabs);
        let indent = if options.wrap_indent && indent < width { indent } else { 0 };

        // Clear the `WrappingChar`s off of the vector or create a new vector if it didn't exist.
        let prev_len = self.char_tags.vec().len();
        self.char_tags.retain(|(_, t)| !matches!(t, CharTag::WrapppingChar));


        let mut distance = 0;
        let mut indent_wrap = 0;
        let mut additions = Vec::new();

        // TODO: Add an enum parameter signifying the wrapping type.
        // Wrapping at the final character at the width of the area.
        if self.line_flags.contains(LineFlags::PURE_1_COL | LineFlags::PURE_ASCII) {
            distance = width;
            while distance < self.text.len() + 1 {
                additions.push((distance as u32, CharTag::WrapppingChar));

                indent_wrap = indent;

                // `width` goes to the first character of the next line, so `n * width` would be
                // off by `n - 1` characters, which is why the `- 1` is there.
                distance += width - indent_wrap;
            }
        } else {
            // If the line reaches the capped limit, it should wrap, even if on the last character.
            for (index, ch) in self.text.char_indices() {
                distance += get_char_width(ch, distance, &options.tabs);

                if distance > width - indent_wrap {
                    distance = get_char_width(ch, distance, &options.tabs);

                    additions.push((index as u32, CharTag::WrapppingChar));

                    indent_wrap = indent;
                }
            }
        }

        // The insertion operation is more efficient if I insert already sorted slices.
        self.char_tags.insert_slice(additions.as_slice());

        self.char_tags.vec().len() != prev_len
    }

    /// Returns an iterator over the wrapping columns of the line.
    pub fn wrap_iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.char_tags.vec().iter().filter(|(_, t)| matches!(t, CharTag::WrapppingChar)).map(|(c, _)| *c)
    }

    /// Returns how many characters are in the line.
    pub fn char_count(&self) -> usize {
        if self.line_flags.contains(LineFlags::PURE_ASCII) {
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
    fn print<T>(
        &self, area: &mut T, x_shift: usize, y: u16, skip: usize, options: &FileOptions,
        forms: &Vec<Form>,
    ) -> u16
    where
        T: OutputArea,
    {
        // Moves the printing cursor to the beginning of the line.
        let mut printing_pos = OutputPos { x: 0, y };
        area.move_cursor(printing_pos);

        let mut printed_lines = 1;

        let (skip, d_x) = if let WrapMethod::NoWrap = options.wrap_method {
            // Knowing this code, this would seem to overwrite `top_wraps`. But since this value is
            // always 0 when wrapping is disabled, it doesn't matter.
            // The leftover here represents the amount of characters that should not be printed,
            // for example, complex emoji may occupy several cells that should be empty, in the
            // case that part of the emoji is located before the first column.
            self.get_col_at_distance(x_shift, &options.tabs)
        } else {
            (skip, 0)
        };
        let mut d_x = d_x as usize;

        (0..d_x).for_each(|_| area.print(' '));

        let char_width = |c, x| {
            if self.line_flags.contains(LineFlags::PURE_1_COL) {
                1
            } else {
                get_char_width(c, x, &options.tabs)
            }
        };

        let mut text_iter = self.text.char_indices().skip_while(|&(b, _)| b < skip);

		if unsafe { crate::FOR_TEST } { panic!("{}, {}", text_iter.next().unwrap().1, skip); }

        let mut wraps = self.wrap_iter();
        let tags = &self.char_tags;

        // In the case where the amount of skipped characters is greater than the placement of
        // the first wrapped one, if `options.wrap_indent`, we need to indent the text
        // immediately, in order to print the text in the correct place. This will happen if
        // the top line wraps and has indentation.
        if let Some(col) = wraps.next() {
            if skip >= col as usize && options.wrap_indent {
                area.print(" ".repeat(self.indent(&options.tabs)));
            } else {
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
            if let &CharTag::AppendForm { index, identifier } = tag {
                area.push_form(&forms[index as usize], identifier);
            } else if let &CharTag::RemoveForm(identifier) = tag {
                area.remove_form(identifier);
            }
        }

        // Every other tag will be iterated with the text.
        // NOTE: Not the most efficient way of doing this.
        let mut tags_iter = tags.vec().iter().skip_while(|(c, _)| (*c as usize) < skip);
        let mut current_char_tag = tags_iter.next();

        let wrap_indent = self.indent(&options.tabs);
        // If `wrap_indent >= area.width()`, indenting on wraps becomes impossible.
        let wrap_indent =
            if options.wrap_indent && wrap_indent < area.width() { wrap_indent } else { 0 };

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

                        printed_lines += 1;
                        printing_pos.y += 1;

                        if printing_pos.y as usize > area.height() {
                            break 'a;
                        }

                        // If the character is wide, fill the rest of the terminal line with
                        // spaces.
                        (d_x..(area.width() - wrap_indent)).for_each(|_| area.print(' '));

                        d_x = 0;

                        area.move_cursor(printing_pos);

                        (0..wrap_indent).for_each(|_| area.print(' '));
                    } else if let CharTag::PrimaryCursor = tag {
                        area.place_cursor(tag);
                    } else if let CharTag::SecondaryCursor = tag {
                        if area.can_place_secondary_cursor() {
                            area.place_cursor(tag);
                        }
                    } else if let CharTag::AppendForm { index, identifier } = tag {
                        area.push_form(&forms[index as usize], identifier);
                    } else if let CharTag::RemoveForm(identifier) = tag {
                        area.remove_form(identifier);
                    }
                } else {
                    break;
                }
            }

            d_x += char_width;
            if let WrapMethod::NoWrap = options.wrap_method {
                if d_x > area.width() {
                    break;
                }
            }

            if ch == '\t' {
                // `repeat()` would use string allocation (I think).
                (0..char_width).for_each(|_| area.print(' '));
            } else if ch == '\n' {
                area.print(' ');
            } else {
                area.print(ch);
            }
        }

        area.clear_form_stack();

        // Erasing anything that is leftover
        let width = area.width();
        if printing_pos.y as usize <= area.height() {
            // Most forms (with the exceptions of strings and comments) are not allowed to carry
            // over lines.
            // NOTE: Eventually will be improved when issue #53667 on rust-lang gets closed.
            if let WrapMethod::Width = options.wrap_method {
                area.print(" ".repeat(width));
            } else if d_x < width {
                area.print(" ".repeat(width));
            }
        }

        printed_lines
    }

    ////////////////////////////////
    // Getters
    ////////////////////////////////
    pub fn text(&self) -> &str {
        &self.text.as_str()
    }
}

/// File text and cursors.
pub struct File<T> {
    /// The lines of the file.
    pub lines: Vec<TextLine>,

    /// Where on the file to start printing.
    print_info: PrintInfo,

    /// The area allocated to the file.
    pub area: T,

    /// The options related to files.
    pub options: FileOptions,

    /// The edtiting cursors on the file.
    pub cursors: Vec<FileCursor>,
    /// The index of the main cursor. The file "follows it".
    pub main_cursor: usize,

    /// The history of edits on this file.
    pub history: History,

	/// Patterns for syntax highlighting.
	patterns: Vec<Regex>,
}

impl<T: OutputArea> File<T> {
    /// Returns a new instance of `File<T>`, given a `Vec<FileLine>`.
    pub fn new(lines: Vec<&str>, options: FileOptions, area: T, patterns: Vec<Regex>) -> File<T> {
        let lines = lines.iter().map(|l| TextLine::new(l)).collect();

        let mut file = File {
            lines,
            // TODO: Remember last session.
            print_info: PrintInfo { top_line: 0, top_wraps: 0, x_shift: 0 },
            area,
            options,
            cursors: Vec::new(),
            main_cursor: 0,
            history: History::new(),
            patterns,
        };

        file.cursors.push(FileCursor::new(
            TextPos { col: 0, byte: 0, line: 0 },
            &file.lines,
            &file.options.tabs,
        ));

        for line in 0..file.lines.len() {
            file.update_line_info(line);
        }

        file
    }

    /// Updates the file's scrolling and checks if it has scrolled.
    pub fn update_print_info(&mut self) -> bool {
        let info = &mut self.print_info;
        let scrolloff = self.options.scrolloff;

        let main_cursor = self.cursors.get(self.main_cursor).expect("cursor not found");
        let current = main_cursor.current();
        let target = main_cursor.target();

        // Scroll if the cursor surpasses the soft cap imposed by `scrolloff`.
        let mut has_scrolled = false;

        // Vertical scroll check:
        if let WrapMethod::NoWrap = self.options.wrap_method {
            // If there is no wrapping, the check is much simpler, just check if the distance to
            // `info.top_line` is within `scrolloff.d_y` and `self.area.height() + scrolloff.d_y`,
            // If it's not, subtract the difference and add/subtract it from `info.top_line`.
            if target.line > info.top_line + self.area.height() - scrolloff.d_y {
                info.top_line += target.line + scrolloff.d_y - info.top_line - self.area.height();
                has_scrolled = true;
            } else if target.line < info.top_line + scrolloff.d_y && info.top_line != 0 {
                info.top_line -= (info.top_line + scrolloff.d_y) - target.line;
                has_scrolled = true;
            }
        } else {
            let (current_wraps, target_wraps, lines_iter) = unsafe {
                let wraps = self.lines.get_unchecked(current.line).wrap_iter();
                let cur = wraps.filter(|&c| c <= current.byte as u32).count();

                let wraps = self.lines.get_unchecked(target.line).wrap_iter();
                let tar = wraps.filter(|&c| c <= target.byte as u32).count();

                (cur, tar, self.lines.get_unchecked_mut(..=target.line).iter_mut())
            };

            let mut d_y = target_wraps;

            // Case where we're moving down.
            if target.line > current.line
                || (target.line == current.line && target_wraps > current_wraps)
            {
                let mut top_offset = 0;
                for (index, line) in lines_iter.enumerate().rev() {
                    // Add the vertical distance, as 1 line plus the times it wraps around.
                    // `target.line` was already added as `target_wraps`.
                    if index != target.line {
                        d_y += line.wrap_iter().count() + 1;
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
                        has_scrolled = true;

                        break;
                    }

                    // If this happens first, we're in the middle of the screen, and don't need
                    // to change `info.top_line`.
                    if index == info.top_line {
                        break;
                    }
                }
            // Case where we're moving up.
            // TODO: Ignore cases where the line is at least `scrolloff.d_y` lines above
            // `info.top_line` after implementing line folding.
            } else if target.line < current.line
                || (target.line == current.line && target_wraps < current_wraps)
            {
                // Set this flag immediately in this case, because the first line that checks out
                // will definitely be `info.top_line`.
                let mut needs_new_print_info = target.line < info.top_line;
                for (index, line) in lines_iter.enumerate().rev() {
                    // Add the vertical distance, as 1 line plus the times it wraps around.
                    // `target.line` was already added as `target_wraps`.
                    if index != target.line {
                        d_y += line.wrap_iter().count() + 1;
                    };

                    if index == info.top_line {
                        // This means we ran into the top line too early, and must scroll up.
                        // `info.top_wraps` is here because the top line might be partially off
                        // screen, and we'd be "comparing" only against the shown wraps, which is
                        // incorrect
                        if d_y < scrolloff.d_y + info.top_wraps {
                            needs_new_print_info = true;
                        // If this happens, we ran into `info.top_line` while below `scrolloff.y`,
                        // this means we're in the "middle" of the screen, and don't need to
                        // scroll.
                        } else if !needs_new_print_info {
                            break;
                        }
                    }

                    // In this case, we have either passed through `info.top_line` while too close,
                    // or not passed through, so a new `info.top_line` is behind the old one.
                    if needs_new_print_info && (d_y >= scrolloff.d_y || index == 0) {
                        info.top_line = index;
                        info.top_wraps = d_y.saturating_sub(scrolloff.d_y);
                        has_scrolled = true;

                        break;
                    }
                }
            }
        }
        let target = &self.cursors[self.main_cursor].target();
        let line = &self.lines[target.line];

        // Horizontal scroll check, done only when the screen can scroll horizontally:
        if let WrapMethod::NoWrap = self.options.wrap_method {
            let distance = line.get_distance_to_col(target.col, &self.options.tabs);

            // If the distance is greater, it means that the cursor is out of bounds.
            if distance > info.x_shift + self.area.width() - scrolloff.d_x {
                // Shift by the amount required to keep the cursor in bounds.
                info.x_shift = distance + scrolloff.d_x - self.area.width();
                has_scrolled = true;
            // Check if `info.x_shift` is already at 0, if it is, no scrolling is dones.
            } else if distance < info.x_shift + scrolloff.d_x {
                info.x_shift = distance.saturating_sub(scrolloff.d_x);
                has_scrolled = true;
            }
        }

        has_scrolled
    }

    // TODO: Eventually will include syntax highlighting, hover info, etc.
    /// Updates the information for a line in the file.
    ///
    /// Returns `true` if the screen needs a full refresh.
    pub fn update_line_info(&mut self, line: usize) -> bool {
        let line = &mut self.lines[line];

        for (index, reg) in self.patterns.iter().enumerate() {
            let mut forms = Vec::new();

            for (num, range) in reg.find_iter(&line.text).enumerate() {
                forms.push((
                    range.start() as u32,
                    CharTag::AppendForm { index: index as u16, identifier: num as u8 },
                ));

                forms.push((range.end() as u32, CharTag::RemoveForm(num as u8)))
            }

            line.char_tags.insert_slice(forms.as_slice());
        }


        line.line_flags.set(LineFlags::PURE_ASCII, line.text.is_ascii());
        line.line_flags.set(
            LineFlags::PURE_1_COL,
            !line.text.chars().any(|c| UnicodeWidthChar::width(c).unwrap_or(1) > 1 || c == '\t'),
        );

        if !matches!(self.options.wrap_method, WrapMethod::NoWrap) {
            line.parse_wrapping(self.area.width(), &self.options)
        } else {
            false
        }
    }

    /// Applies a splice to the file.
    pub fn splice_edit<S>(&mut self, edit: Vec<S>, old_range: TextRange) -> bool
    where
        S: ToString,
    {
        let old_lines_len = self.lines.len();

        let (edits, new_range) = self.history.add_change(&mut self.lines, edit, old_range);
        let edits: Vec<TextLine> = edits.iter().map(|l| TextLine::new(l)).collect();

        self.lines.splice(old_range.lines(), edits);

        let mut full_refresh_needed = self.lines.len() != old_lines_len;

        for line in new_range.lines() {
            full_refresh_needed |= self.update_line_info(line);
        }

        for cursor in &mut self.cursors {
            let new_pos = if cursor.current().line == old_range.end.line {
                new_range.end + cursor.current() - old_range.end
            } else if cursor.current().line > old_range.end.line {
                cursor.current().move_line(new_range.end.line - old_range.end.line)
            } else {
                continue;
            };

            cursor.move_to(new_pos, &self.lines, &self.options)
        }

        full_refresh_needed
    }

    /// Undoes the last moment in history.
    pub fn undo(&mut self) {
        let (splices, print_info) = match self.history.undo(&mut self.lines) {
            Some((changes, print_info)) => (changes, print_info),
            None => return,
        };
        self.print_info = print_info.unwrap_or(self.print_info);

		let mut changed_lines = Vec::new();

        for splice in &splices {
            let taken_range = TextRange { start: splice.start(), end: splice.taken_end() };

            for line in taken_range.lines() {
                if !changed_lines.contains(&line) {
                    self.update_line_info(line);

                    changed_lines.push(line)
                }
            }
        }

        let mut cursors = self.cursors.iter_mut();
        let mut new_cursors = Vec::new();

		for splice in splices.iter() {
            if let Some(cursor) = cursors.next() {
                cursor.move_to(splice.taken_end(), &self.lines, &self.options);
            } else {
                new_cursors.push(FileCursor::new(
                    splice.taken_end(),
                    &self.lines,
                    &self.options.tabs,
                ));
            }
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

		let mut changed_lines = Vec::new();

        for splice in &splices {
            let added_range = TextRange { start: splice.start(), end: splice.added_end() };

            for line in added_range.lines() {
                if !changed_lines.contains(&line) {
                    self.update_line_info(line);

                    changed_lines.push(line)
                }
            }
        }

        let mut cursors = self.cursors.iter_mut();
        let mut new_cursors = Vec::new();

		for splice in splices {
            if let Some(cursor) = cursors.next() {
                cursor.move_to(splice.added_end(), &self.lines, &self.options);
            } else {
                new_cursors.push(FileCursor::new(
                    splice.added_end(),
                    &self.lines,
                    &self.options.tabs,
                ));
            }
		}

        self.cursors.extend(new_cursors);
    }

    /// Prints the file, according to its current position.
    pub fn print_file(&mut self, force: bool, forms: &Vec<Form>) {
        // Saving the current cursor lines, in case the case that the whole screen doesn't need to
        // be reprinted.
        let mut cursor_lines: Vec<usize> =
            self.cursors.iter().map(|c| [c.current().line, c.target().line]).flatten().collect();

        // Checks if the main cursor's position change has caused the line to scroll.
        let has_scrolled = self.update_print_info();

        let current = self.cursors.get(self.main_cursor).unwrap().current();
        let char_tags = &mut self.lines.get_mut(current.line).unwrap().char_tags;
        char_tags.retain(|(_, t)| !matches!(t, CharTag::PrimaryCursor));

        let target = self.cursors.get(self.main_cursor).unwrap().target();

        let line = &mut self.lines[target.line];
        // If the cursor is at the end of the line, it's syntax will be placed at a virtual ' '.
        // The same goes for any type of syntax highlighting.
        let byte = line.text.char_indices().nth(target.col).unwrap_or((line.text.len(), ' ')).0;
        line.char_tags.insert((byte as u32, CharTag::PrimaryCursor)); 

        // Updates the information for each cursor in the file.
        self.cursors.iter_mut().for_each(|c| c.update());

        let info = self.print_info;

        // The line at the top of the screen and the amount of hidden columns.
        let skip = if info.top_wraps > 0 {
            let line = self.lines.get(info.top_line).unwrap();
            line.wrap_iter().nth(info.top_wraps - 1).unwrap() as usize
        } else {
            0
        };

        // If the file has scrolled, reprint the whole screen.
        if has_scrolled || force {
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

            // Clears the lines where nothing has been printed.
            for _ in (y as usize)..=self.area.height() {
                self.area.move_cursor(OutputPos { x: 0, y });
                (0..self.area.width()).for_each(|_| self.area.print(' '));
                y += 1;
            }
        // If it hasn't, only reprint the lines where cursors have been in.
        } else {
            cursor_lines.sort_unstable();
            cursor_lines.dedup();

            let mut last_counted_line = info.top_line;

            let mut y = 0;

            for index in cursor_lines {
                if index < info.top_line {
                    continue;
                }

                // Do this to not count lines multiple times unnecessarily.
                let lines_iter = self.lines.get(last_counted_line..index).unwrap().iter();

                // Calculating the vertical distance between the last printed line and the new line.
                // If there's no wrapping, we can just take the amount of lines in between.
                y += if let WrapMethod::NoWrap = self.options.wrap_method {
                    lines_iter.count() as u16
                // If there is wrapping, we need to add it to the calculations.
                } else {
                    lines_iter.map(|l| 1 + l.wrap_iter().count() as u16).sum()
                };

                if y as usize > self.area.height() + info.top_wraps {
                    break;
                }

                last_counted_line = index;
                let line = &self.lines[index];

                if index == info.top_line {
                    // The top line will be printed at the top, no matter what.
                    line.print(&mut self.area, info.x_shift, 0, skip, &self.options, forms);
                } else {
                    let y = y - info.top_wraps as u16;
                    line.print(&mut self.area, info.x_shift, y, 0, &self.options, forms);
                }
            }
        }
    }

    ////////////////////////////////
    // Getters
    ////////////////////////////////
    pub fn top_line(&self) -> usize {
        self.print_info.top_line
    }

    pub fn top_wraps(&self) -> usize {
        self.print_info.top_wraps
    }

    pub fn x_shift(&self) -> usize {
        self.print_info.x_shift
    }

    pub fn print_info(&self) -> PrintInfo {
        self.print_info
    }
}

pub fn get_char_width(ch: char, col: usize, tabs: &TabPlaces) -> usize {
    if ch == '\t' { tabs.get_tab_len(col) } else { UnicodeWidthChar::width(ch).unwrap_or(1) }
}
