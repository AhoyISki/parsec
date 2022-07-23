use std::{fs, path::PathBuf};

use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers},
    style::{ContentStyle, Stylize},
};
use regex::Regex;

use crate::{
    action::TextRange,
    config::{FileOptions, LineNumbers},
    file::File,
    impl_input_handler,
    input::{InputHandler, ModeList},
    map_actions,
    output::OutputArea,
    tags::Form,
};

// NOTE: This struct should strive to be completely UI agnostic, i.e., it should work wether the
// app is used in a terminal or in a GUI.
pub struct Buffer<T: OutputArea> {
    /// The contents of the file
    pub file: File<T>,

    // Where exactly on the screen the origin and end of the area are placed is not important here.
    /// The area allocated to the status line.
    pub status_line_area: T,
    /// The area allocated to the line numbers.
    pub line_num_area: T,

    /// List of mapped modes for file editing.
    mappings: ModeList<Buffer<T>>,

    /// List of forms.
    forms: Vec<Form>,
}

impl<T: OutputArea> Buffer<T> {
    /// Returns a new instance of ContentArea
    pub fn new(mut area: T, path: PathBuf, options: FileOptions) -> Buffer<T> {
        // TODO: In the future, this will not panic!
        let file = fs::read_to_string(path).expect("file not found");

        let line_num_area;

        let bracket_regex = Regex::new(r"\{|\}|\(|\)|\[|\]").unwrap();
        let string_regex = Regex::new(r#"""#).unwrap();
        let patterns = vec![bracket_regex, string_regex];

        let mut file_handler = Buffer {
            file: {
                // The lines must be '\n' terminated for better compatibility with tree-sitter.
                let lines: Vec<&str> = file.split_inclusive('\n').collect();

                let mut file_area = area.partition_y((area.height() - 2) as u16);

                let mut line_num_width = 3;
                let mut num_exp = 10;

                while lines.len() > num_exp {
                    num_exp *= 10;
                    line_num_width += 1;
                }
                line_num_area = file_area.partition_x(line_num_width);

                File::new(lines, options, file_area, patterns)
            },

            status_line_area: area,
            line_num_area,

            mappings: ModeList::new(),

            forms: {
                let bracket = ContentStyle::new().red();
                let string = ContentStyle::new().green();

                vec![Form::new(bracket, false), Form::new(string, false)]
            },
        };

        map_actions! {
            file_handler: FileHandler<T>, mappings;
            "insert" => [
                // Move all cursors up.
                key: (KeyCode::Up, KeyModifiers::NONE) => {
                    |h: &mut Buffer<T>| {
                        h.file.cursors.iter_mut().for_each(|c| {
                            c.unset_anchor();
                            c.move_ver(-1, &h.file.lines, &h.file.options.tabs);
                        });
                        h.refresh_screen(false);
                    }
                },
                // Move all cursors down.
                key: (KeyCode::Down, KeyModifiers::NONE) => {
                    |h: &mut Buffer<T>| {
                        h.file.cursors.iter_mut().for_each(|c| {
                            c.unset_anchor();
                            c.move_ver(1, &h.file.lines, &h.file.options.tabs);
                        });
                        h.refresh_screen(false);
                    }
                },
                // Move all cursors left.
                key: (KeyCode::Left, KeyModifiers::NONE) => {
                    |h: &mut Buffer<T>| {
                        h.file.cursors.iter_mut().for_each(|c| {
                            c.unset_anchor();
                            c.move_hor(-1, &h.file.lines, &h.file.options.tabs);
                        });
                        h.refresh_screen(false);
                    }
                },
                // Move all cursors right.
                key: (KeyCode::Right, KeyModifiers::NONE) => {
                    |h: &mut Buffer<T>| {
                        h.file.cursors.iter_mut().for_each(|c| {
                            c.unset_anchor();
                            c.move_hor(1, &h.file.lines, &h.file.options.tabs);
                        });
                        h.refresh_screen(false);
                    }
                },
                // Place anchor and move all cursors up.
                key: (KeyCode::Up, KeyModifiers::SHIFT) => {
                    |h: &mut Buffer<T>| {
                        h.file.cursors.iter_mut().for_each(|c| {
                            if let None = c.anchor() { c.set_anchor(); }
                            c.move_ver(-1, &h.file.lines, &h.file.options.tabs);
                        });
                        h.refresh_screen(false);
                    }
                },
                // Place anchor and move all cursors down.
                key: (KeyCode::Down, KeyModifiers::SHIFT) => {
                    |h: &mut Buffer<T>| {
                        h.file.cursors.iter_mut().for_each(|c| {
                            if let None = c.anchor() { c.set_anchor(); }
                            c.move_ver(1, &h.file.lines, &h.file.options.tabs);
                        });
                        h.refresh_screen(false);
                    }
                },
                // Place anchor and move all cursors left.
                key: (KeyCode::Left, KeyModifiers::SHIFT) => {
                    |h: &mut Buffer<T>| {
                        h.file.cursors.iter_mut().for_each(|c| {
                            if let None = c.anchor() { c.set_anchor(); }
                            c.move_hor(-1, &h.file.lines, &h.file.options.tabs);
                        });
                        h.refresh_screen(false);
                    }
                },
                // Place anchor and move all cursors right.
                key: (KeyCode::Right, KeyModifiers::SHIFT) => {
                    |h: &mut Buffer<T>| {
                        h.file.cursors.iter_mut().for_each(|c| {
                            if let None = c.anchor() { c.set_anchor(); }
                            c.move_hor(1, &h.file.lines, &h.file.options.tabs);
                        });
                        h.refresh_screen(true);
                    }
                },
                // Deletes either the character in front, or the selection.
                key: (KeyCode::Delete, KeyModifiers::NONE) => {
                    |h: &mut Buffer<T>| {
                        let cursor = &mut h.file.cursors[h.file.main_cursor];

                        if cursor.anchor().is_none() {
                            cursor.set_anchor();
                            cursor.move_hor(1, &h.file.lines, &h.file.options.tabs);
                        };

						let range = cursor.range();
                        let edit = vec![""];

                        let refresh_needed = h.file.splice_edit(edit, range);
                        h.refresh_screen(refresh_needed);
                    }
                },
                // Deletes either the character behind, or the selection.
                key: (KeyCode::Backspace, KeyModifiers::NONE) => {
                    |h: &mut Buffer<T>| {
                        let cursor = &mut h.file.cursors[h.file.main_cursor];

                        if cursor.anchor().is_none() {
                            cursor.set_anchor();
                            cursor.move_hor(-1, &h.file.lines, &h.file.options.tabs);
                        };

						let range = cursor.range();
                        let edit = vec![""];

                        let refresh_needed = h.file.splice_edit(edit, range);
                        h.refresh_screen(refresh_needed);
                    }
                },
                key: (KeyCode::Tab, KeyModifiers::NONE) => {
                    |h: &mut Buffer<T>| {
                        let cursor = &mut h.file.cursors[h.file.main_cursor];

                        let edit = if h.file.options.tabs_as_spaces {
                            let tab_len = h.file.options.tabs.get_tab_len(cursor.target().col);
                            vec![" ".repeat(tab_len)]
                        } else {
                            vec!["\t".to_string()]
                        };

						let range = cursor.range();

                        let refresh_needed = h.file.splice_edit(edit, range);
                        h.refresh_screen(refresh_needed);
                    }
                },
                key: (KeyCode::Char('z'), KeyModifiers::CONTROL) => {
                    |h: &mut Buffer<T>| {
                        h.file.undo();
                        h.refresh_screen(true);
                    }
                },
                key: (KeyCode::Char('y'), KeyModifiers::CONTROL) => {
                    |h: &mut Buffer<T>| {
                        h.file.redo();
                        h.refresh_screen(true);
                    }
                },
                key: (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    |_: &mut Buffer<T>| {
                        unsafe { crate::FOR_TEST = true }
                    }
                },
                key: (KeyCode::Enter, KeyModifiers::NONE) => {
                    |h: &mut Buffer<T>| {
                        let cursor = &h.file.cursors[h.file.main_cursor];

                        let edit = vec![""; 2];

                        let refresh_needed = h.file.splice_edit(edit, cursor.range());
                        h.refresh_screen(refresh_needed);
                    }
                },
                key: (KeyCode::Char(' '), KeyModifiers::NONE) => {
                    |h: &mut Buffer<T>| {
                        h.file.history.new_moment(h.file.print_info());

                        let cursor = &h.file.cursors[h.file.main_cursor];

                        let edit = vec![' '];

                        let refresh_needed = h.file.splice_edit(edit, cursor.range());
                        h.refresh_screen(refresh_needed);
                    }
                },
                _ => {
                    |h: &mut Buffer<T>, c: char| {
                        let cursor = &h.file.cursors[h.file.main_cursor];

                        let edit = vec![c];

                        let refresh_needed = h.file.splice_edit(edit, cursor.range());
                        h.refresh_screen(refresh_needed);
                    }
                }
            ]
        }

        file_handler.refresh_screen(true);

        file_handler
    }

    /* TODO: Finish this function */
    /// Prints the contents of the file from line on the file.
    #[inline]
    fn refresh_screen(&mut self, force: bool) {
        self.file.print_file(force, &self.forms);

        // Printing the line numbers
        // NOTE: Might move to a separate function, but idk.
        let mut pos = crate::output::OutputPos { x: 0, y: 0 };
        let top_line = self.file.top_line();

        self.line_num_area.move_cursor(pos);

        'a: for (index, line) in self.file.lines.iter().skip(top_line).enumerate() {
            let wraps = if index == 0 {
                self.file.top_wraps()..(line.wrap_iter().count() + 1)
            } else {
                0..(line.wrap_iter().count() + 1)
            };

            for _ in wraps {
                if pos.y as usize > self.line_num_area.height() {
                    break 'a;
                }
                let width = self.line_num_area.width() as usize;

                let text = match self.file.options.line_numbers {
                    LineNumbers::Absolute => format!("{:>4}│", index + top_line),
                    LineNumbers::Relative => {
                        let main = self.file.cursors.get(self.file.main_cursor).unwrap().current();
                        format!("{:>w$}│", usize::abs_diff(index + top_line, main.line), w = width)
                    }
                    LineNumbers::Hybrid => {
                        let main = self.file.cursors.get(self.file.main_cursor).unwrap().current();
                        if index + top_line == main.line {
                            format!("{:>w$}│", index + top_line, w = width)
                        } else {
                            format!(
                                "{:>w$}│",
                                usize::abs_diff(index + top_line, main.line),
                                w = width
                            )
                        }
                    }
                    _ => "".to_string(),
                };
                self.line_num_area.print(text);
                pos.y += 1;
                self.line_num_area.move_cursor(pos);
            }
        }

        for _ in (pos.y as usize)..(self.line_num_area.height() + 1) {
            self.line_num_area.print("     ");
            pos.y += 1;
            self.line_num_area.move_cursor(pos);
        }

        self.file.area.flush();
    }
}

impl_input_handler!(Buffer<T>, mappings);
