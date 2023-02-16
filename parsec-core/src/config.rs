use std::{
    ops::Deref,
    sync::{Arc, RwLock, RwLockReadGuard, TryLockError, RwLockWriteGuard},
};

use crate::{
    text::TextLine,
    ui::{Area, Label, Ui},
};

/// If and how to wrap lines at the end of the screen.
#[derive(Default, Debug, Copy, Clone)]
pub enum WrapMethod {
    /// Wrap at the end of the screen.
    Width,
    /// Wrap at a given width.
    Capped(u16),
    /// Wrap at the end of the screen, on word boundaries.
    Word,
    /// Don't wrap at all.
    #[default]
    NoWrap,
}

// Pretty much only exists because i wanted one of these with usize as its builtin type.
#[derive(Debug, Copy, Clone)]
pub struct ScrollOff {
    pub d_y: usize,
    pub d_x: usize,
}

impl Default for ScrollOff {
    fn default() -> Self {
        ScrollOff { d_y: 5, d_x: 3 }
    }
}

/// Where the tabs are placed on screen, can be regular or varied.
#[derive(Debug, Clone)]
pub enum TabPlaces {
    /// The same lenght for every tab.
    Regular(usize),
    /// Varying lenghts for different tabs.
    Varied(Vec<usize>),
}

/// Wheter to show the new line or not.
#[derive(Default, Debug, Clone, Copy)]
pub enum ShowNewLine {
    #[default]
    /// Never show new lines.
    Never,
    /// Show the given character on every new line.
    Always(char),
    /// Show the given character only when there is whitespace at end of the line.
    AfterSpace(char),
}

impl ShowNewLine {
    pub fn get_new_line_ch(&self, last_ch: char) -> char {
        match self {
            ShowNewLine::Never => ' ',
            ShowNewLine::Always(ch) => *ch,
            ShowNewLine::AfterSpace(ch) => {
                if last_ch.is_whitespace() {
                    *ch
                } else {
                    ' '
                }
            }
        }
    }
}

impl Default for TabPlaces {
    fn default() -> Self {
        TabPlaces::Regular(4)
    }
}

impl TabPlaces {
    /// Returns the amount of spaces between a position and the next tab place.
    pub(crate) fn get_tab_len<L, A>(&self, x: usize, label: &L) -> usize
    where
        L: Label<A>,
        A: Area,
    {
        let space_len = label.get_char_len(' ');
        match self {
            TabPlaces::Regular(step) => (step - (x % step)) * space_len,
            TabPlaces::Varied(steps) => {
                (steps.iter().find(|&s| *s > x).expect("not enough tabs") - x) * space_len
            }
        }
    }
}

// TODO: Move options to a centralized option place.
// TODO: Make these private.
/// Some standard parsec options.
#[derive(Default, Debug, Clone)]
pub struct Config {
    pub line_numbers_separator: Option<&'static str>,
    /// How to wrap the file.
    pub wrap_method: WrapMethod,
    /// The distance between the cursor and the edges of the screen when scrolling.
    pub scrolloff: ScrollOff,
    /// How to indent.
    pub tab_places: TabPlaces,
    /// Wether to indent wrapped lines or not.
    pub wrap_indent: bool,
    /// Wether to convert tabs to spaces.
    pub tabs_as_spaces: bool,
    /// Wether (and how) to show new lines.
    pub show_new_line: ShowNewLine,
}

impl Config {
    pub fn usable_indent<U>(&self, line: &TextLine, label: &U::Label) -> usize
    where
        U: Ui,
    {
        let indent = line.indent::<U>(label, self);
        if self.wrap_indent && indent < label.area().width() {
            indent
        } else {
            0
        }
    }
}

/// A read-write reference to information, and can tell readers if said information has changed.
pub struct RwData<T>
where
    T: ?Sized,
{
    data: Arc<RwLock<T>>,
    updated_state: Arc<RwLock<usize>>,
    last_read_state: RwLock<usize>,
}

impl<T> RwData<T> {
    /// Returns a new instance of `RwState`.
    pub fn new(data: T) -> Self {
        // It's 1 here so that any `RoState`s created from this will have `has_changed()` return
        // `true` at least once, by copying the second value - 1.
        RwData {
            data: Arc::new(RwLock::new(data)),
            updated_state: Arc::new(RwLock::new(1)),
            last_read_state: RwLock::new(1),
        }
    }
}

impl<T> RwData<T>
where
    T: ?Sized,
{
    /// Reads the information.
    ///
    /// Also makes it so that `has_changed()` returns false.
    pub fn read(&self) -> RwLockReadGuard<T> {
        let updated_version = self.updated_state.read().unwrap();
        let mut last_read_state = self.last_read_state.write().unwrap();

        if *updated_version > *last_read_state {
            *last_read_state = *updated_version;
        }

        self.data.read().unwrap()
    }

    /// Tries to read the data immediately and returns a `Result`.
    pub fn try_read(&self) -> Result<RwLockReadGuard<T>, TryLockError<RwLockReadGuard<T>>> {
        let updated_version = self.updated_state.read().unwrap();
        let mut last_read_state = self.last_read_state.write().unwrap();

        if *updated_version > *last_read_state {
            *last_read_state = *updated_version;
        }

        self.data.try_read()
    }

    /// Returns a writeable reference to the state.
    ///
    /// Also makes it so that `has_changed()` on it or any of its clones returns `true`.
    pub fn write(&mut self) -> RwLockWriteGuard<T> {
        *self.updated_state.write().unwrap() += 1;
        self.data.write().unwrap()
    }

    /// Wether or not it has changed since it was last read.
    pub fn has_changed(&self) -> bool {
        let last_version = self.updated_state.read().unwrap();
        let mut current_version = self.last_read_state.write().unwrap();
        let has_changed = *last_version > *current_version;
        *current_version = *last_version;

        has_changed
    }
}

impl<T> Clone for RwData<T>
where
    T: ?Sized,
{
    fn clone(&self) -> Self {
        RwData {
            data: self.data.clone(),
            updated_state: self.updated_state.clone(),
            last_read_state: RwLock::new(*self.updated_state.read().unwrap() - 1),
        }
    }
}

impl<D> Default for RwData<D>
where
    D: Default,
{
    fn default() -> Self {
        RwData {
            data: Arc::new(RwLock::new(D::default())),
            updated_state: Arc::new(RwLock::new(1)),
            last_read_state: RwLock::new(1),
        }
    }
}

unsafe impl<T> Sync for RwData<T> where T: ?Sized {}

/// A read-only reference to information.
pub struct RoData<T>
where
    T: ?Sized,
{
    data: Arc<RwLock<T>>,
    updated_state: Arc<RwLock<usize>>,
    last_read_state: RwLock<usize>,
}

impl<T> RoData<T>
where
    T: ?Sized,
{
    /// Reads the information.
    ///
    /// Also makes it so that `has_changed()` returns false.
    pub fn read(&self) -> RwLockReadGuard<T> {
        let updated_version = self.updated_state.read().unwrap();
        let mut last_read_state = self.last_read_state.write().unwrap();

        if *updated_version > *last_read_state {
            *last_read_state = *updated_version;
        }

        self.data.read().unwrap()
    }

    /// Checks if the state within has changed.
    ///
    /// If you have called `has_changed()` or `read()`, without any changes, it will return false.
    pub fn has_changed(&self) -> bool {
        let updated_version = self.updated_state.read().unwrap();
        let mut current_version = self.last_read_state.write().unwrap();

        if *updated_version > *current_version {
            *current_version = *updated_version;

            true
        } else {
            false
        }
    }
}

impl<T> From<&RwData<T>> for RoData<T>
where
    T: ?Sized,
{
    fn from(state: &RwData<T>) -> Self {
        RoData {
            data: state.data.clone(),
            updated_state: state.updated_state.clone(),
            last_read_state: RwLock::new(*state.updated_state.read().unwrap() - 1),
        }
    }
}

// NOTE: Each `RoState` of a given state will have its own internal update counter.
impl<T> Clone for RoData<T>
where
    T: ?Sized,
{
    fn clone(&self) -> Self {
        RoData {
            data: self.data.clone(),
            updated_state: self.updated_state.clone(),
            last_read_state: RwLock::new(*self.last_read_state.read().unwrap()),
        }
    }
}
