use crossterm::style::{
    StyledContent,
    ContentStyle
};

use std::{
    ops,
    fmt::Display,
    cmp::{
        self,
        max,
        min
    }
};

use crate::file::CursorPos;

/// A relative position where text is printed.
///
/// These should only be used to move the cursor responsible for printing
/// to the output, not the user's actual cursor. As they only print, they
/// cannot be negative. The postition is relative to a given `OutputArea`.
#[derive(Copy, Clone, Debug)]
pub struct OutputPos {
    pub x: u16,
    pub y: u16,
}

impl ops::Add for OutputPos {
    type Output = OutputPos;

    fn add(self, rhs: Self) -> Self::Output {
        OutputPos { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl ops::Add<CursorPos> for OutputPos {
    type Output = OutputPos;

    fn add(self, rhs: CursorPos) -> Self::Output {
        OutputPos { x: self.x + rhs.x as u16, y: self.y + rhs.y as u16 }
    }
}

impl cmp::PartialEq for OutputPos {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }

    fn ne(&self, other: &Self) -> bool {
        self.x != other.x || self.y != other.y
    }
}

impl cmp::PartialOrd for OutputPos {
    fn ge(&self, other: &Self) -> bool {
        self.x >= other.x && self.y >= other.y
    }

    fn gt(&self, other: &Self) -> bool {
        self.x > other.x && self.y > other.y
    }

    fn le(&self, other: &Self) -> bool {
        self.x <= other.x && self.y <= other.y
    }

    fn lt(&self, other: &Self) -> bool {
        self.x < other.x && self.y < other.y
    }

    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        if self > other {
            Some(cmp::Ordering::Greater)
        } else if self < other {
            Some(cmp::Ordering::Less)
        } else {
            Some(cmp::Ordering::Equal)
        }
    }
}

impl From<CursorPos> for OutputPos {
    fn from(pos: CursorPos) -> Self {
        OutputPos { x: pos.x as u16, y: pos.y as u16 }
    }
}

impl Display for OutputPos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

// It's a string because of multi byte UTF-8 and graphemes, which can be several
// characters long.
/// A character containing text and a style which applies to said text.
#[derive(Clone)]
pub struct StyledChar {
    pub text: StyledContent<String>,
    pub width: usize,

    pub is_wrapping: bool,
}

impl StyledChar {
    pub fn new(grapheme: &str, width: usize) -> StyledChar {
        StyledChar {
            text: StyledContent::new(ContentStyle::new(), grapheme.to_string()),
            width,
            is_wrapping: false
        }
    }
}

/// An area in the output (terminal or GUI).
///
/// Examples include: The file buffer, status line, etc.
pub trait OutputArea {
    fn new(origin: OutputPos, end: OutputPos) -> Self;

    /// Allocates an origin and end to an area.
    ///
    /// Should not be called directly.
    fn allocate_area(
        &self, origin: OutputPos, end: OutputPos) -> (OutputPos, OutputPos)  {
        let (width, height) = (self.width(), self.height());
        let origin = OutputPos { x: max(origin.x, 0), y: max(origin.y, 0) };
        let end = OutputPos { x: min(end.x, width), y: min(end.y, height) };
        (origin, end)
    }

    /// Prints styled text.
    /// 
    /// Prints content that has colors, italics, bold, etc.
    fn print_styled(&mut self, text: StyledChar);

    /// Prints plain text.
    /// 
    /// Will print to output without any styling whatsoever.
    fn print_string(&mut self, text: String);

    /// Moves the relative printing cursor.
    /// 
    /// Will change where the next characters will be printed, without wrapping.
    fn move_cursor(&mut self, pos: OutputPos);

    /// Moves the relative printing cursor to the origin.
    /// 
    /// Will change where the next characters will be printed, without wrapping.
    fn move_cursor_to_origin(&mut self);

    /// Returns the width of the area.
    fn width(&self) -> u16;

    /// Returns the height of the area
    fn height(&self) -> u16;

    /// Refreshes the area
    fn flush(&mut self);
}
