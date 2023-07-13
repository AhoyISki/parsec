//! APIs for the construction of widgets, and a few common ones.
//!
//! This module describes two types of widget, [`NormalWidget`]s and
//! [`ActionableWidget`]s. [`NormalWidget`]s simply show information,
//! and cannot receive input or be focused. [`ActionableWidget`] is a
//! superset of [`NormalWidget`], capable of receiving input,
//! focusing, unfocusing, and showing cursors.
//!
//! The module also provides 4 native widgets, [`StatusLine<U>`] and
//! [`LineNumbers<U>`], which are [`NormalWidget`]s, and
//! [`FileWidget<U>`] and [`CommandLine<U>`] which are
//! [`ActionableWidget`]s.
//!
//! These widgets are supposed to be universal, not needing a specific
//! [`Ui`] implementation to work. As an example, the
//! [`parsec-term`](https://docs.rs/parsec-term) crate, which is a ui
//! implementation for Parsec, defines "rule" widgets, which are
//! separators that only really make sense in the context of a
//! terminal.
mod command_line;
mod file_widget;
mod line_numbers;
mod status_line;

#[cfg(not(feature = "deadlock-detection"))]
use std::sync::RwLock;
use std::{cmp::Ordering, ops::Range, sync::Arc};

#[cfg(feature = "deadlock-detection")]
use no_deadlocks::RwLock;

use crate::{
    data::{DownCastableData, RawReadableData, ReadableData, RwData},
    position::{Cursor, Editor, Mover},
    tags::form::FormPalette,
    text::{PrintCfg, Text},
    ui::{Area, Ui}
};

// TODO: Maybe set up the ability to print images as well.
/// An area where text will be printed to the screen.
pub trait Widget<U>: DownCastableData + 'static
where
    U: Ui + 'static
{
    /// Updates the widget, allowing the modification of its
    /// [`Area`][Ui::Area].
    ///
    /// This function will be called when Parsec determines that the
    /// [`WidgetNode`]
    ///
    /// [`Session<U>`]: crate::session::Session
    fn update(&mut self, area: &U::Area);

    /// The text that this widget prints out.
    fn text(&self) -> &Text;

    /// Scrolls the text vertically by an amount.
    fn scroll_vertically(&mut self, _d_y: i32) {}

    fn print_info(&self) -> U::PrintInfo {
        U::PrintInfo::default()
    }

    fn print_cfg(&self) -> PrintCfg {
        PrintCfg::default()
    }

    fn print(&self, area: &U::Area, palette: &FormPalette) {
        area.print(self.text(), self.print_info(), self.print_cfg(), palette)
    }
}

pub enum WidgetType<U>
where
    U: Ui
{
    NoInput(RwData<dyn Widget<U>>),
    SchemeInput(RwData<dyn SchemeInputWidget<U>>),
    DirectInput(RwData<dyn DirectInputWidget<U>>)
}

impl<U> WidgetType<U>
where
    U: Ui
{
    pub fn no_input(widget: impl Widget<U>) -> Self {
        WidgetType::NoInput(RwData::new_unsized(Arc::new(RwLock::new(widget))))
    }

    pub fn scheme_input(widget: impl SchemeInputWidget<U>) -> Self {
        WidgetType::SchemeInput(RwData::new_unsized(Arc::new(RwLock::new(widget))))
    }

    pub fn direct_input(widget: impl DirectInputWidget<U>) -> Self {
        WidgetType::DirectInput(RwData::new_unsized(Arc::new(RwLock::new(widget))))
    }

    pub fn update(&self, area: &U::Area) {
        match self {
            WidgetType::NoInput(widget) => widget.write().update(area),
            WidgetType::SchemeInput(widget) => widget.write().update(area),
            WidgetType::DirectInput(widget) => widget.write().update(area)
        }
    }

    pub fn print(&self, area: &U::Area, palette: &FormPalette) {
        match self {
            WidgetType::NoInput(widget) => widget.read().print(area, palette),
            WidgetType::SchemeInput(widget) => widget.read().print(area, palette),
            WidgetType::DirectInput(widget) => widget.read().print(area, palette)
        }
    }

    /// Returns the downcast ref of this [`WidgetType<U>`].
    pub fn downcast_ref<W>(&self) -> Option<RwData<W>>
    where
        U: Ui,
        W: Widget<U> + 'static
    {
        match self {
            WidgetType::NoInput(widget) => widget.clone().try_downcast::<W>().ok(),
            WidgetType::SchemeInput(widget) => widget.clone().try_downcast::<W>().ok(),
            WidgetType::DirectInput(widget) => widget.clone().try_downcast::<W>().ok()
        }
    }

    pub fn data_is<W>(&self) -> bool
    where
        W: Widget<U>
    {
        match self {
            WidgetType::NoInput(widget) => widget.data_is::<W>(),
            WidgetType::SchemeInput(widget) => widget.data_is::<W>(),
            WidgetType::DirectInput(widget) => widget.data_is::<W>()
        }
    }

    pub fn data_is_and<W>(&self, f: impl FnOnce(&W) -> bool) -> bool
    where
        W: Widget<U>
    {
        match self {
            WidgetType::NoInput(widget) => widget.inspect_as::<W, bool>(f).is_some_and(|ret| ret),
            WidgetType::SchemeInput(widget) => {
                widget.inspect_as::<W, bool>(f).is_some_and(|ret| ret)
            }
            WidgetType::DirectInput(widget) => {
                widget.inspect_as::<W, bool>(f).is_some_and(|ret| ret)
            }
        }
    }

    pub fn as_scheme_input(&self) -> Option<&RwData<dyn SchemeInputWidget<U>>> {
        match self {
            WidgetType::SchemeInput(widget) => Some(widget),
            _ => None
        }
    }

    pub fn scheme_ptr_eq(&self, other: &RwData<dyn SchemeInputWidget<U>>) -> bool {
        match self {
            WidgetType::SchemeInput(widget) => widget.ptr_eq(other),
            _ => false
        }
    }

    pub(crate) fn raw_inspect<B>(&self, f: impl FnOnce(&dyn Widget<U>) -> B) -> B {
        match self {
            WidgetType::NoInput(widget) => {
                let widget = widget.raw_read();
                f(&*widget)
            }
            WidgetType::SchemeInput(widget) => {
                let widget = widget.raw_read();
                f(&*widget)
            }
            WidgetType::DirectInput(widget) => {
                let widget = widget.raw_read();
                f(&*widget)
            }
        }
    }

    pub fn has_changed(&self) -> bool {
        match self {
            WidgetType::SchemeInput(widget) => widget.has_changed(),
            WidgetType::DirectInput(widget) => widget.has_changed(),
            WidgetType::NoInput(_) => false
        }
    }
}

/// A widget that can receive input and show [`Cursor`]s.
pub trait SchemeInputWidget<U>: Widget<U>
where
    U: Ui + 'static
{
    /// Returns an [`Editor<U>`], which uses a cursor to input text to
    /// [`self`].
    fn editor<'a>(&'a mut self, index: usize, edit_accum: &'a mut EditAccum) -> Editor<U>;

    /// Returns a [`Mover<U>`], which can move a cursor's position
    /// over [`self`].
    fn mover<'a>(&'a mut self, index: usize, area: &'a U::Area) -> Mover<U>;

    /// This is used specifically to remove and add the [`Cursor`]
    /// [`Tag`][crate::tags::Tag]s to [`self`]
    fn members_for_cursor_tags(&mut self) -> (&mut Text, &[Cursor], usize);

    /// The list of active [`Cursor`]s on [`self`].
    fn cursors(&self) -> &[Cursor];

    /// A mutable list of active [`Cursor`]s
    ///
    /// As this is an [`Option`], the widget may or may not pass this
    /// reference.
    fn mut_cursors(&mut self) -> Option<&mut Vec<Cursor>>;

    /// The index of the main cursor of [`self`].
    fn main_cursor_index(&self) -> usize;

    /// A mutable reference to the main cursor index of [`self`].
    ///
    /// As this is an [`Option`], the widget may or may not pass this
    /// reference.
    fn mut_main_cursor_index(&mut self) -> Option<&mut usize>;

    /// Starts a new [`Moment`][crate::history::Moment].
    ///
    /// Will panic by default, assuming that the [`ActionableWidget`]
    /// does not have a [`History`][crate::history::History].
    fn new_moment(&mut self) {
        panic!("This ActionableWidget does not have a History of its own.")
    }

    /// Undoes the last [`Moment`][crate::history::Moment].
    ///
    /// Will panic by default, assuming that the [`ActionableWidget`]
    /// does not have a [`History`][crate::history::History].
    fn undo(&mut self, _area: &U::Area) {
        panic!("This ActionableWidget does not have a History of its own.")
    }

    /// Redoes the last [`Moment`][crate::history::Moment].
    ///
    /// Will panic by default, assuming that the [`ActionableWidget`]
    /// does not have a [`History`][crate::history::History].
    fn redo(&mut self, _area: &U::Area) {
        panic!("This ActionableWidget does not have a History of its own.")
    }

    /// Actions to do whenever this [`ActionableWidget`] is focused.
    fn on_focus(&mut self, _area: &U::Area) {}

    /// Actions to do whenever this [`ActionableWidget`] is unfocused.
    fn on_unfocus(&mut self, _area: &U::Area) {}
}

pub trait DirectInputWidget<U>: Widget<U>
where
    U: Ui
{
    /// Actions to do whenever this [`ActionableWidget`] is focused.
    fn on_focus(&mut self, _area: &U::Area) {}

    /// Actions to do whenever this [`ActionableWidget`] is unfocused.
    fn on_unfocus(&mut self, _area: &U::Area) {}
}

/// An accumulator used specifically for editing with [`Editor<U>`]s.
#[derive(Default)]
pub struct EditAccum {
    pub chars: isize,
    pub changes: isize
}

/// A struct used by [`InputMethod`][crate::input::InputScheme]s to
/// edit [`Text`].
pub struct WidgetActor<'a, U, Sw>
where
    U: Ui + 'static,
    Sw: SchemeInputWidget<U> + ?Sized
{
    clearing_needed: bool,
    widget: &'a RwData<Sw>,
    area: &'a U::Area
}

impl<'a, U, Sw> WidgetActor<'a, U, Sw>
where
    U: Ui,
    Sw: SchemeInputWidget<U> + ?Sized + 'static
{
    /// Returns a new instace of [`WidgetActor<U, AW>`].
    pub(crate) fn new(widget: &'a RwData<Sw>, area: &'a U::Area) -> Self {
        WidgetActor {
            clearing_needed: false,
            widget,
            area
        }
    }

    /// Removes all intersecting [`Cursor`]s from the list, keeping
    /// only the last from the bunch.
    fn clear_intersections(&mut self) {
        let mut widget = self.widget.write();
        let Some(cursors) = widget.mut_cursors() else {
            return
        };

        let (mut start, mut end) = cursors[0].pos_range();
        let mut last_index = 0;
        let mut to_remove = Vec::new();

        for (index, cursor) in cursors.iter_mut().enumerate().skip(1) {
            if cursor.try_merge(start, end).is_ok() {
                to_remove.push(last_index);
            }
            (start, end) = cursor.pos_range();
            last_index = index;
        }

        for index in to_remove.iter().rev() {
            cursors.remove(*index);
        }
    }

    /// Edits on every cursor selection in the list.
    pub fn edit_on_each_cursor<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut Editor<U>)
    {
        self.clear_intersections();
        let mut widget = self.widget.write();
        let mut edit_accum = EditAccum::default();
        let cursors = widget.cursors();

        for index in 0..cursors.len() {
            let mut editor = widget.editor(index, &mut edit_accum);
            f(&mut editor);
        }
    }

    /// Alters every selection on the list.
    pub fn move_each_cursor<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut Mover<U>)
    {
        let mut widget = self.widget.write();
        for index in 0..widget.cursors().len() {
            let mut mover = widget.mover(index, self.area);
            f(&mut mover);
        }

        // TODO: Figure out a better way to sort.
        widget.mut_cursors().map(|cursors| {
            cursors.sort_unstable_by(|j, k| at_start_ord(&j.range(), &k.range()));
        });
        self.clearing_needed = true;
    }

    /// Alters the nth cursor's selection.
    pub fn move_nth<F>(&mut self, mut f: F, index: usize)
    where
        F: FnMut(&mut Mover<U>)
    {
        let mut widget = self.widget.write();
        let mut mover = widget.mover(index, self.area);
        f(&mut mover);

        if let Some(cursors) = widget.mut_cursors() {
            let cursor = cursors.remove(index);
            let range = cursor.range();
            let new_index = match cursors.binary_search_by(|j| at_start_ord(&j.range(), &range)) {
                Ok(index) => index,
                Err(index) => index
            };
            cursors.insert(new_index, cursor);

            if let Some(main_cursor) = widget.mut_main_cursor_index() {
                if index == *main_cursor {
                    *main_cursor = new_index;
                }
            }
        };

        self.clearing_needed = true;
    }

    /// Alters the main cursor's selection.
    pub fn move_main<F>(&mut self, f: F)
    where
        F: FnMut(&mut Mover<U>)
    {
        self.move_nth(f, self.main_cursor_index());
    }

    /// Alters the last cursor's selection.
    pub fn move_last<F>(&mut self, f: F)
    where
        F: FnMut(&mut Mover<U>)
    {
        let len = self.cursors_len();
        if len > 0 {
            self.move_nth(f, len - 1);
        }
    }

    /// Edits on the nth cursor's selection.
    pub fn edit_on_nth<F>(&mut self, mut f: F, index: usize)
    where
        F: FnMut(&mut Editor<U>)
    {
        let mut widget = self.widget.write();
        assert!(index < widget.cursors().len(), "Index {index} out of bounds.");
        if self.clearing_needed {
            self.clear_intersections();
            self.clearing_needed = false;
        }

        let mut edit_accum = EditAccum::default();
        let mut editor = widget.editor(index, &mut edit_accum);
        f(&mut editor);

        for index in (index + 1)..(widget.cursors().len() - 1) {
            // A bit hacky, but the creation of an `Editor` automatically
            // calibrates the cursor's position.
            widget.editor(index, &mut edit_accum);
        }
    }

    /// Edits on the main cursor's selection.
    pub fn edit_on_main<F>(&mut self, f: F)
    where
        F: FnMut(&mut Editor<U>)
    {
        self.edit_on_nth(f, self.main_cursor_index());
    }

    /// Edits on the last cursor's selection.
    pub fn edit_on_last<F>(&mut self, f: F)
    where
        F: FnMut(&mut Editor<U>)
    {
        let len = self.cursors_len();
        if len > 0 {
            self.edit_on_nth(f, len - 1);
        }
    }

    /// The main cursor index.
    pub fn main_cursor_index(&self) -> usize {
        self.widget.read().main_cursor_index()
    }

    /// Rotates the main cursor index forward.
    pub fn rotate_main_forward(&mut self) {
        let cursors_len = self.cursors_len();
        if cursors_len == 0 {
            return;
        }

        self.widget.write().mut_main_cursor_index().map(|main_index| {
            *main_index = if *main_index == cursors_len - 1 {
                0
            } else {
                *main_index + 1
            }
        });
    }

    /// Rotates the main cursor index backwards.
    pub fn rotate_main_backwards(&mut self) {
        let cursors_len = self.cursors_len();
        if cursors_len == 0 {
            return;
        }

        self.widget.write().mut_main_cursor_index().map(|main_index| {
            *main_index = if *main_index == 0 {
                cursors_len - 1
            } else {
                *main_index - 1
            }
        });
    }

    /// The amount of active [`Cursor`]s in the [`Text`].
    pub fn cursors_len(&self) -> usize {
        self.widget.read().cursors().len()
    }

    /// Starts a new [`Moment`][crate::history::Moment].
    pub fn new_moment(&mut self) {
        self.widget.write().new_moment();
    }

    /// Undoes the last [`Moment`][crate::history::Moment].
    pub fn undo(&mut self) {
        self.widget.write().undo(self.area);
    }

    /// Redoes the last [`Moment`][crate::history::Moment].
    pub fn redo(&mut self) {
        self.widget.write().redo(self.area);
    }

    pub fn main_cursor(&self) -> Cursor {
        self.widget.read().cursors()[self.main_cursor_index()]
    }

    pub fn nth_cursor(&self, index: usize) -> Option<Cursor> {
        self.widget.read().cursors().get(index).copied()
    }
}

/// Comparets the `left` and `right` [`Range`]s, returning an
/// [`Ordering`], based on the intersection at the start.
fn at_start_ord(left: &Range<usize>, right: &Range<usize>) -> Ordering {
    if left.end > right.start && right.start > left.start {
        std::cmp::Ordering::Equal
    } else if left.start > right.end {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Less
    }
}

pub use command_line::CommandLine;
pub use file_widget::FileWidget;
pub use line_numbers::{LineNumbers, LineNumbersCfg};
pub use status_line::{file_parts, status_parts, StatusLine, StatusPart};
