use std::{
    cmp::max,
    fmt::{Alignment, Write},
    sync::{Arc, Mutex},
};

use super::{file_widget::FileWidget, NormalWidget, Widget};
use crate::{
    config::{RoData, RwData},
    tags::form::{DEFAULT, LINE_NUMBERS, MAIN_LINE_NUMBER},
    text::{Text, TextLineBuilder},
    ui::{Area, EndNode, Label, Side, Ui},
};

pub struct LineNumbers<U>
where
    U: Ui,
{
    file: RoData<FileWidget<U>>,
    text: Text<U>,
    main_line_builder: TextLineBuilder,
    other_line_builder: TextLineBuilder,
    min_width: usize,
    line_numbers_config: LineNumbersConfig,
}

unsafe impl<U> Send for LineNumbers<U> where U: Ui {}

impl<U> LineNumbers<U>
where
    U: Ui + 'static,
{
    /// Returns a new instance of `LineNumbersWidget`.
    pub fn new(
        file_widget: RwData<FileWidget<U>>, line_numbers_config: LineNumbersConfig,
    ) -> Widget<U> {
        let file = RoData::from(&file_widget);

        let line_numbers = LineNumbers {
            file,
            text: Text::default(),
            main_line_builder: TextLineBuilder::from([MAIN_LINE_NUMBER, DEFAULT]),
            other_line_builder: TextLineBuilder::from([LINE_NUMBERS, DEFAULT]),
            min_width: 1,
            line_numbers_config,
        };

        Widget::Normal(Arc::new(Mutex::new(line_numbers)))
    }

    pub fn new_with_default_config(file_widget: RwData<FileWidget<U>>) -> Widget<U> {
        let file = RoData::from(&file_widget);

        let line_numbers = LineNumbers {
            file,
            text: Text::default(),
            main_line_builder: TextLineBuilder::from([MAIN_LINE_NUMBER, DEFAULT]),
            other_line_builder: TextLineBuilder::from([LINE_NUMBERS, DEFAULT]),
            min_width: 1,
            line_numbers_config: LineNumbersConfig::default(),
        };

        Widget::Normal(Arc::new(Mutex::new(line_numbers)))
    }

    fn calculate_width(&self) -> usize {
        let mut width = 1;
        let mut num_exp = 10;
        // "+ 1" because we index from 1, not from 0.
        let len = self.file.read().text().lines().len() + 1;

        while len > num_exp {
            num_exp *= 10;
            width += 1;
        }
        max(width, self.min_width)
    }
}

impl<U> NormalWidget<U> for LineNumbers<U>
where
    U: Ui + 'static,
{
    fn identifier(&self) -> &str {
        "parsec-line-numbers"
    }

    fn update(&mut self, end_node: &mut EndNode<U>) {
        let file = self.file.read();
        let width = self.calculate_width();
        end_node.label.area_mut().request_len(width, Side::Right).unwrap();

        let lines = file.printed_lines();
        let main_line = file.main_cursor().true_row();

        self.text.lines.clear();

        for line in lines.iter() {
            let mut line_number = String::with_capacity(width + 5);
            let number = match self.line_numbers_config.numbering {
                Numbering::Absolute => *line + 1,
                Numbering::Relative => usize::abs_diff(*line + 1, main_line),
                Numbering::Hybrid => {
                    if *line != main_line {
                        usize::abs_diff(*line, main_line) + 1
                    } else {
                        *line + 1
                    }
                }
            };
            match self.line_numbers_config.alignment {
                Alignment::Left => write!(&mut line_number, "[]{:<width$}[]\n", number).unwrap(),
                Alignment::Right => write!(&mut line_number, "[]{:>width$}[]\n", number).unwrap(),
                Alignment::Center => write!(&mut line_number, "[]{:^width$}[]\n", number).unwrap(),
            }
            if *line == main_line {
                self.text.lines.push(self.main_line_builder.form_text_line(line_number));
            } else {
                self.text.lines.push(self.other_line_builder.form_text_line(line_number));
            }
        }
    }

    fn needs_update(&self) -> bool {
        self.file.has_changed()
    }

    fn text(&self) -> &Text<U> {
        &self.text
    }
}

/// How to show the line numbers on screen.
#[derive(Default, Debug, Copy, Clone)]
pub enum Numbering {
    #[default]
    /// Line numbers relative to the beginning of the file.
    Absolute,
    /// Line numbers relative to the main cursor's line, including that line.
    Relative,
    /// Relative line numbers on every line, except the main cursor's.
    Hybrid,
}

#[derive(Clone, Copy)]
pub struct LineNumbersConfig {
    pub numbering: Numbering,
    pub alignment: Alignment,
}

impl Default for LineNumbersConfig {
    fn default() -> Self {
        Self { alignment: Alignment::Left, numbering: Numbering::default() }
    }
}
