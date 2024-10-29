use core::str;

pub use crossterm::event::{KeyCode, KeyEvent, KeyModifiers as KeyMod};

pub use self::{
    commander::Command,
    default::Regular,
    helper::{Cursor, Cursors, EditHelper, Editor, Mover},
    inc_search::{Fwd, IncSearcher},
    remap::*,
};
use crate::{data::RwData, ui::Ui, widgets::Widget};

mod commander;
mod default;
mod helper;
mod inc_search;
mod remap;

/// An input method for a [`Widget`]
///
/// Input methods are the way that Duat decides how keys are going to
/// modify widgets.
///
/// In principle, there are two types of `Mode`, the ones which use
/// [`Cursors`], and the ones which don't. In [`Mode::send_key`], you
/// receive an [`&mut Cursors`], and if you're not using cursors, you
/// should run [`Cursors::clear`], in order to make sure there are no
/// cursors.
///
/// If a [`Mode`] has cursors, it _must_ use the [`EditHelper`] struct
/// in order to modify of the widget's [`Text`].
///
/// If your widget/input method combo is not based on cursors. You get
/// more freedom to modify things as you wish, but you should refrain
/// from using [`Cursor`]s in order to prevent bugs.
///
/// For this example, I will create a `Menu` widget, which is not
/// supposed to have [`Cursor`]s. For an example with [`Cursor`]s, see
/// [`EditHelper`]:
///
/// ```rust
/// # use duat_core::text::Text;
/// #[derive(Default)]
/// struct Menu {
///     text: Text,
///     selected_entry: usize,
///     active_etry: Option<usize>,
/// }
/// ```
/// In this widget, I will create a menu whose entries can be selected
/// by an [`Mode`].
///
/// Let's say that said menu has five entries, and one of them can be
/// active at a time:
///
/// ```rust
/// # #![feature(let_chains)]
/// # use duat_core::text::{Text, text, AlignCenter};
/// # struct Menu {
/// #     text: Text,
/// #     selected_entry: usize,
/// #     active_entry: Option<usize>,
/// # }
/// impl Menu {
///     pub fn shift_selection(&mut self, shift: i32) {
///         let selected = self.selected_entry as i32 + shift;
///         self.selected_entry = if selected < 0 {
///             4
///         } else if selected > 4 {
///             0
///         } else {
///             selected as usize
///         };
///     }
///
///     pub fn toggle(&mut self) {
///         self.active_entry = match self.active_entry {
///             Some(entry) if entry == self.selected_entry => None,
///             Some(_) | None => Some(self.selected_entry),
///         };
///     }
///
///     fn build_text(&mut self) {
///         let mut builder = Text::builder();
///         text!(builder, AlignCenter);
///
///         for i in 0..5 {
///             if let Some(active) = self.active_entry
///                 && active == i
///             {
///                 if self.selected_entry == i {
///                     text!(builder, [MenuSelActive])
///                 } else {
///                     text!(builder, [MenuActive])
///                 }
///             } else if self.selected_entry == i {
///                 text!(builder, [MenuSelected]);
///             } else {
///                 text!(builder, [MenuInactive]);
///             }
///
///             text!(builder, "Entry " i);
///         }
///
///         self.text = builder.finish();
///     }
/// }
/// ```
///
/// By making `shift_selection` and `toggle` `pub`, I can allow an end
/// user to create their own [`Mode`] for this widget.
///
/// Let's say that I have created an [`Mode`] `MenuInput` for
/// the `Menu`. This input method is actually the one that is
/// documented on the documentation entry for [`Mode`], you can
/// check it out next, to see how that was handled.
///
/// Now I'll implement [`Widget`]:
///
/// ```rust
/// # use std::marker::PhantomData;
/// # use duat_core::{
/// #     data::RwData, input::{Mode, KeyEvent}, forms::{self, Form},
/// #     text::{text, Text}, ui::{PushSpecs, Ui}, widgets::{Widget, WidgetCfg},
/// # };
/// # #[derive(Default)]
/// # struct Menu {
/// #     text: Text,
/// #     selected_entry: usize,
/// #     active_entry: Option<usize>,
/// # }
/// # impl Menu {
/// #     fn build_text(&mut self) {
/// #         todo!();
/// #     }
/// # }
/// struct MenuCfg<U>(PhantomData<U>);
///
/// impl<U: Ui> WidgetCfg<U> for MenuCfg<U> {
///     type Widget = Menu;
///
///     fn build(self, on_file: bool) -> (Menu, impl Fn() -> bool + 'static, PushSpecs) {
///         let checker = || false;
///
///         let mut widget = Menu::default();
///         widget.build_text();
///
///         let specs = PushSpecs::left().with_hor_len(10.0).with_ver_len(5.0);
///
///         (widget, checker, specs)
///     }
/// }
///
/// impl<U: Ui> Widget<U> for Menu {
///     type Cfg = MenuCfg<U>;
///
///     fn cfg() -> Self::Cfg {
///         MenuCfg(PhantomData)
///     }
///
///     fn text(&self) -> &Text {
///         &self.text
///     }
///   
///     fn text_mut(&mut self) -> &mut Text {
///         &mut self.text
///     }
///
///     fn once() {
///         forms::set_weak("MenuInactive", "Inactive");
///         forms::set_weak("MenuSelected", "Inactive");
///         forms::set_weak("MenuActive", Form::blue());
///         forms::set_weak("MenuSelActive", Form::blue());
///     }
/// }
/// ```
///
/// We can use `let checker = || false` here, since active [`Widget`]s
/// get automatically updated whenever they are focused or a key is
/// sent.
///
/// Now, let's take a look at some [`Widget`] methods that are unique
/// to widgets that can take input. Those are the [`on_focus`] and
/// [`on_unfocus`] methods:
///
/// ```rust
/// # use std::marker::PhantomData;
/// # use duat_core::{
/// #     data::RwData, forms::{self, Form}, text::{text, Text}, ui::{PushSpecs, Ui},
/// #     widgets::{Widget, WidgetCfg},
/// # };
/// # #[derive(Default)]
/// # struct Menu {
/// #     text: Text,
/// #     selected_entry: usize,
/// #     active_entry: Option<usize>,
/// # }
/// # struct MenuCfg<U>(PhantomData<U>);
/// # impl<U: Ui> WidgetCfg<U> for MenuCfg<U> {
/// #     type Widget = Menu;
/// #     fn build(self, on_file: bool) -> (Menu, impl Fn() -> bool + 'static, PushSpecs) {
/// #         (Menu::default(), || false, PushSpecs::left())
/// #     }
/// # }
/// impl<U: Ui> Widget<U> for Menu {
/// #     type Cfg = MenuCfg<U>;
/// #     fn cfg() -> Self::Cfg {
/// #         MenuCfg(PhantomData)
/// #     }
/// #     fn text(&self) -> &Text {
/// #         &self.text
/// #     }
/// #     fn once() {}
/// #     fn text_mut(&mut self) -> &mut Text {
/// #         &mut self.text
/// #     }
///     // ...
///     fn on_focus(&mut self, _area: &U::Area) {
///         forms::set_weak("MenuInactive", "Default");
///         forms::set_weak("MenuSelected", Form::new().on_grey());
///         forms::set_weak("MenuSelActive", Form::blue().on_grey());
///     }
///
///     fn on_unfocus(&mut self, _area: &U::Area) {
///         forms::set_weak("MenuInactive", "Inactive");
///         forms::set_weak("MenuSelected", "Inactive");
///         forms::set_weak("MenuSelActive", Form::blue());
///     }
/// }
/// ```
///
/// These methods can do work when the wiget is focused or unfocused.
///
/// In this case, I chose to replace the [`Form`]s with "inactive"
/// variants, to visually show when the widget is not active.
///
/// Do also note that [`on_focus`] and [`on_unfocus`] are optional
/// methods, since a change on focus is not always necessary.
///
/// Now, all that is left to do is  the `MenuInput` [`Mode`]. We just
/// need to create an empty struct and call the methods of the `Menu`:
///
/// ```rust
/// # #![feature(let_chains)]
/// # use std::marker::PhantomData;
/// # use duat_core::{
/// #     data::RwData, input::{key, Cursors, Mode, KeyCode, KeyEvent}, forms::{self, Form},
/// #     text::{text, Text}, ui::{PushSpecs, Ui}, widgets::{Widget, WidgetCfg},
/// # };
/// # #[derive(Default)]
/// # struct Menu {
/// #     text: Text,
/// #     selected_entry: usize,
/// #     active_entry: Option<usize>,
/// # }
/// # impl Menu {
/// #     pub fn shift_selection(&mut self, shift: i32) {}
/// #     pub fn toggle(&mut self) {}
/// #     fn build_text(&mut self) {}
/// # }
/// # struct MenuCfg<U>(PhantomData<U>);
/// # impl<U: Ui> WidgetCfg<U> for MenuCfg<U> {
/// #     type Widget = Menu;
/// #     fn build(self, on_file: bool) -> (Menu, impl Fn() -> bool + 'static, PushSpecs) {
/// #         (Menu::default(), || false, PushSpecs::left())
/// #     }
/// # }
/// # impl<U: Ui> Widget<U> for Menu {
/// #     type Cfg = MenuCfg<U>;
/// #     fn cfg() -> Self::Cfg {
/// #         MenuCfg(PhantomData)
/// #     }
/// #     fn text(&self) -> &Text {
/// #         &self.text
/// #     }
/// #     fn once() {}
/// #     fn text_mut(&mut self) -> &mut Text {
/// #         &mut self.text
/// #     }
/// # }
/// #[derive(Clone)]
/// struct MenuInput;
///
/// impl<U: Ui> Mode<U> for MenuInput {
///     type Widget = Menu;
///
///     fn send_key(
///         &mut self,
///         key: KeyEvent,
///         widget: &RwData<Menu>,
///         area: &U::Area,
///         cursors: &mut Cursors,
///     ) {
///         cursors.clear();
///         use KeyCode::*;
///         let mut menu = widget.write();
///         
///         match key {
///             key!(Down) => menu.shift_selection(1),
///             key!(Up) => menu.shift_selection(-1),
///             key!(Enter | Tab | Char(' ')) => menu.toggle(),
///             _ => {}
///         }
///     }
/// }
/// ```
/// Notice the [`key!`] macro. This macro is useful for pattern
/// matching [`KeyEvent`]s on [`Mode`]s.
///
/// [`Cursor`]: crate::input::Cursor
/// [`print`]: Widget::print
/// [`on_focus`]: Widget::on_focus
/// [`on_unfocus`]: Widget::on_unfocus
/// [resizing]: Area::constrain_ver
/// [`Form`]: crate::forms::Form
/// [default]: default::KeyMap
/// [`duat-kak`]: https://docs.rs/duat-kak/latest/duat_kak/index.html
/// [Kakoune]: https://github.com/mawww/kakoune
/// [`Text`]: crate::Text
/// [`&mut Cursors`]: Cursors
pub trait Mode<U: Ui>: Sized + Clone + Send + Sync + 'static {
    type Widget: Widget<U>;

    fn send_key(
        &mut self,
        key: KeyEvent,
        widget: &RwData<Self::Widget>,
        area: &U::Area,
        cursors: &mut Cursors,
    );

    #[allow(unused)]
    fn on_focus(&mut self, area: &U::Area) {}

    #[allow(unused)]
    fn on_unfocus(&mut self, area: &U::Area) {}
}

/// This is a macro for matching keys in patterns:
///
/// Use this for quickly matching a [`KeyEvent`], probably inside an
/// [`Mode`]:
///
/// ```rust
/// # use duat_core::input::{KeyEvent, KeyCode, KeyMod, key};
/// # fn test(key: KeyEvent) {
/// if let key!(KeyCode::Char('o'), KeyMod::NONE) = key { /* Code */ }
/// // as opposed to
/// if let KeyEvent {
///     code: KeyCode::Char('c'),
///     modifiers: KeyMod::NONE,
///     ..
/// } = key
/// { /* Code */ }
/// # }
/// ```
///
/// You can also assign while matching:
///
/// ```rust
/// # use duat_core::input::{KeyEvent, KeyCode, KeyMod, key};
/// # fn test(key: KeyEvent) {
/// if let key!(code, KeyMod::SHIFT | KeyMod::ALT) = key { /* Code */ }
/// // as opposed to
/// if let KeyEvent {
///     code,
///     modifiers: KeyMod::SHIFT | KeyMod::ALT,
///     ..
/// } = key
/// { /* Code */ }
/// # }
/// ```
pub macro key {
    ($code:pat) => {
        KeyEvent { code: $code, modifiers: KeyMod::NONE, .. }
    },

    ($code:pat, $modifiers:pat) => {
        KeyEvent { code: $code, modifiers: $modifiers, .. }
    }
}

/// Return the lenght of a strin in chars
#[allow(dead_code)]
#[doc(hidden)]
pub const fn len_chars(s: &str) -> usize {
    let mut i = 0;
    let b = s.as_bytes();
    let mut nchars = 0;
    while i < b.len() {
        if crate::text::utf8_char_width(b[i]) > 0 {
            nchars += 1;
        }
        i += 1;
    }
    nchars
}

/// Maps each [`char`] in an `&str` to a [`KeyEvent`]
#[allow(dead_code)]
#[doc(hidden)]
pub fn key_events<const LEN: usize>(str: &str, modif: KeyMod) -> [KeyEvent; LEN] {
    let mut events = [KeyEvent::new(KeyCode::Esc, KeyMod::NONE); LEN];

    for (event, char) in events.iter_mut().zip(str.chars()) {
        *event = KeyEvent::new(KeyCode::Char(char), modif)
    }

    events
}

/// Converts an `&str` to a sequence of [`KeyEvent`]s
///
/// The conversion follows the same rules as remaps in Vim, that is:
pub fn str_to_keys(str: &str) -> Vec<KeyEvent> {
    const MODS: [(char, KeyMod); 4] = [
        ('C', KeyMod::CONTROL),
        ('A', KeyMod::ALT),
        ('S', KeyMod::SHIFT),
        ('M', KeyMod::META),
    ];

    let mut keys = Vec::new();
    let mut on_special = false;

    for seq in str.split_inclusive(['<', '>']) {
        if !on_special {
            let end = if seq.ends_with('<') {
                on_special = true;
                seq.len() - 1
            } else {
                seq.len()
            };

            keys.extend(seq[..end].chars().map(|c| KeyEvent::from(KeyCode::Char(c))));
        } else if seq.ends_with('>') {
            let trimmed = seq.trim_end_matches('>');
            let mut parts = trimmed.split('-');

            let modifs = if trimmed.contains('-')
                && let Some(seq) = parts.next()
            {
                let mut modifs = KeyMod::empty();

                for (str, modif) in MODS {
                    if seq.contains(str) {
                        modifs.set(modif, true);
                    }
                }

                let doubles = ['C', 'A', 'S', 'M']
                    .iter()
                    .any(|c| seq.chars().filter(|char| char == c).count() > 1);

                let not_modifs = seq.chars().any(|c| !['C', 'A', 'S', 'M'].contains(&c));

                if modifs.is_empty() || doubles || not_modifs {
                    keys.push(KeyEvent::from(KeyCode::Char('<')));
                    keys.extend(seq.chars().map(|c| KeyEvent::from(KeyCode::Char(c))));
                    on_special = false;
                    continue;
                }

                modifs
            } else {
                KeyMod::empty()
            };

            let code = match parts.next() {
                Some("Enter") => KeyCode::Enter,
                Some("Tab") => KeyCode::Tab,
                Some("Backspace") => KeyCode::Backspace,
                Some("Del") => KeyCode::Delete,
                Some("Esc") => KeyCode::Esc,
                Some("Up") => KeyCode::Up,
                Some("Down") => KeyCode::Down,
                Some("Left") => KeyCode::Left,
                Some("Right") => KeyCode::Right,
                Some("PageUp") => KeyCode::PageUp,
                Some("PageDown") => KeyCode::PageDown,
                Some("Home") => KeyCode::Home,
                Some("End") => KeyCode::End,
                Some("Insert") => KeyCode::Insert,
                Some("F1") => KeyCode::F(1),
                Some("F2") => KeyCode::F(2),
                Some("F3") => KeyCode::F(3),
                Some("F4") => KeyCode::F(4),
                Some("F5") => KeyCode::F(5),
                Some("F6") => KeyCode::F(6),
                Some("F7") => KeyCode::F(7),
                Some("F8") => KeyCode::F(8),
                Some("F9") => KeyCode::F(9),
                Some("F10") => KeyCode::F(10),
                Some("F11") => KeyCode::F(11),
                Some("F12") => KeyCode::F(12),
                Some(seq)
                    if let Some(char) = seq.chars().next()
                        && (char.is_lowercase()
                            || (char.is_uppercase() && modifs.contains(KeyMod::SHIFT)))
                        && seq.chars().count() == 1 =>
                {
                    KeyCode::Char(char)
                }
                _ => {
                    keys.push(KeyEvent::from(KeyCode::Char('<')));
                    keys.extend(seq.chars().map(|c| KeyEvent::from(KeyCode::Char(c))));
                    on_special = false;
                    continue;
                }
            };

            on_special = false;
            keys.push(KeyEvent::new(code, modifs));
        }
    }

    keys
}
