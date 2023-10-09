//! Creation and execution of commands.
//!
//! Commands in Parsec work through the use of functions that don't
//! require references (unless they're `'static`) and return results
//! which may contain a [`Text`] to be displayed, if successful, and
//! *must* contain an error [`Text`] to be displayed if they fail.
//!
//! Commands act on two parameters. which will be provided when ran:
//! [`Flags`] and [`Args`].
//!
//! [`Flags`] will contain a list of all flags that were passed to the
//! command. These flags follow the UNIX conventions, that is `"-"`
//! starts a blob cluster, `"--"` starts a single, larger flag, and
//! `"--"` followed by nothing means that the remaining arguments are
//! not flags. Here's an example:
//!
//! `"my-command --flag1 --flag2 -blob -- --not-flag more-args"`
//!
//! `"--not-flag"` would not be treated as a flag, being instead
//! treated as an argument in conjunction with `"more-args"`.
//!
//! [`Args`] is merely an iterator over the remaining arguments, which
//! are given as `&str`s to be consumed.
//!
//! Here's a simple example of how one would add a command:
//!
//! ```rust
//! # use parsec_core::commands::{self, Flags, Args};
//! # use std::sync::{
//! #     atomic::{AtomicBool, Ordering},
//! #     Arc
//! # };
//! #
//! // Any of these callers will work for running the command.
//! let callers = ["my-command", "mc"];
//!
//! // `commands::add` create a new globally avaliable command.
//! let result = commands::add(callers, move |_flags, _args| {
//!     unimplemented!();
//! });
//!
//! // Adding a command can fail if a command with the same
//! // name already exists.
//! // In this case, the command is brand new, so no the
//! // Result is `Ok`.
//! assert!(result.is_ok());
//! ```
//!
//! In this case, a command has been added that can be called with
//! both `"my-command"` and `"mc"`.
//!
//! Here's a simple command that makes use of [`Flags`]:
//!
//! ```rust
//! # use parsec_core::commands;
//! # use std::sync::{
//! #     atomic::{AtomicU32, Ordering},
//! #     Arc
//! # };
//! #
//! let expression = Arc::new(AtomicU32::default());
//! let callers = ["my-command", "mc"];
//! let my_command = {
//!     let expression = expression.clone();
//!     commands::add(callers, move |flags, _args| {
//!         // `Flags::unit` checks for `--` flags
//!         if flags.unit("happy") {
//!             expression.store('😁' as u32, Ordering::Relaxed)
//!         // `Flags::blob` checks for `-` flags
//!         // They can check for any valid unicode character.
//!         } else if flags.blob("🤯") {
//!             expression.store('🤯' as u32, Ordering::Relaxed)
//!         } else if flags.unit("sad") {
//!             expression.store('😢' as u32, Ordering::Relaxed)
//!         } else {
//!             expression.store('😶' as u32, Ordering::Relaxed)
//!         }
//!         Ok(None)
//!     })
//! };
//!
//! // The order of flags doesn't matter.
//! commands::run("mc --sad -🤯 unused-1 unused-2").unwrap();
//!
//! let num = expression.load(Ordering::Relaxed);
//! assert_eq!(char::from_u32(num), Some('🤯'))
//! ```
//!
//! To run commands, simply call [`commands::run`]:
//!
//! ```rust
//! # use parsec_core::{
//! #     commands,
//! #     session::SessionCfg,
//! #     text::{PrintCfg, text},
//! #     ui::{FileBuilder, Ui},
//! #     widgets::{CommandLine, status},
//! # };
//! # fn test_fn<U: Ui>(ui: U) {
//! let session = SessionCfg::new(ui)
//!     .with_file_fn(|builder: &mut FileBuilder<U>, _file| {
//!         // `commands::run` might return an `Ok(Some(Text))`,
//!         // hence the double unwrap.
//!         let output = commands::run("lol").unwrap().unwrap();
//!         let status = status!("Output of \"lol\": " output);
//!
//!         builder.push(status.build());
//!     });
//!
//! let callers = ["lol", "lmao"];
//! commands::add(callers, |_flags, _args| {
//!     Ok(Some(text!("😜")))
//! }).unwrap();
//! # }
//! ```
//!
//! In the above example, we are creating a new [`SessionCfg`], which
//! will be used to start Parsec. in it, we're changing the
//! "`file_fn`", a file constructor that, among other things, will
//! attach widgets to files that are opened.
//!
//! In that "`file_fn`", the command `"lol"` is being ran. Notice that
//! the command doesn't exist at the time the closure was declared.
//! But since this closure will only be ran after the [`Session`] has
//! started, as long as the command was added before that point,
//! everything will work just fine.
//!
//! Here's an example that makes use of the arguments of a command:
//!
//! ```rust
//! # use parsec_core::{commands, text::text};
//! commands::add(["write", "w"], move |_flags, args| {
//!     let mut count = 0;
//!     for file in args {
//!         count += 1;
//!         unimplemented!("Logic for writing to the files.");
//!     }
//!
//!     // The return message (if there is one) is in the form of
//!     // a `Text`, so it is recommended that you use the
//!     // `parsec_core::text::text` macro to facilitate the
//!     // creation of that message.
//!     Ok(Some(text!(
//!         "Wrote to " [AccentOk] count
//!         [Default] " files successfully."
//!     )))
//! });
//! ```
//!
//! The returned result from a command should make use of 4 specific
//! forms: `"CommandOk"`, `"AccentOk"`, `"CommandErr"` and
//! `"AccentErr"`. When errors are displayed, the `"Default"` [`Form`]
//! gets mapped to `"CommandOk"` if the result is [`Ok`], and to
//! `"CommandErr"` if the result is [`Err`]. . This formatting of
//! result messages allows for more expressive feedback while still
//! letting the end user configure their appearance.
//!
//! In the previous case, we handled a variable number of arguments.
//! But we can also easily handle a static number of arguments:
//!
//! ```rust
//! # use parsec_core::{commands, text::text};
//! commands::add(["copy", "cp"], move |flags, args| {
//!     // You can return custom error messages, to improve the
//!     // feedback of failures when running the command.
//!     let source = args.next().ok_or(text!("No source given."))?;
//!     let target = args.next().ok_or(text!("No target given."))?;
//!
//!     // This is optional, if you feel like your command shouldn't
//!     // allow for more args than are required, you can call this.
//!     if args.next().is_some() {
//!         return Err(text!("Too many arguments."));
//!     }
//!
//!     if flags.unit("link") {
//!         unimplemented!("Logic for linking files.");
//!     } else {
//!         unimplemented!("Logic for copying files.");
//!     }
//!
//!     Ok(Some(text!(
//!         "Copied from " [AccentOk] source [Default]
//!         " to " [AccentOk] target [Default] "."
//!     )))
//! });
//! ```
//!
//! [`SessionCfg`]: crate::session::SessionCfg
//! [`Session`]: crate::session::Session
//! [`commans::run`]: crate::commands::run
//! [`Form`]: crate::forms::Form
#[cfg(not(feature = "deadlock-detection"))]
use std::sync::RwLock;
use std::{
    collections::HashMap,
    mem::MaybeUninit,
    sync::{Arc, LazyLock},
};

pub use global::*;
#[cfg(feature = "deadlock-detection")]
use no_deadlocks::RwLock;

use crate::{
    data::{Data, RwData},
    text::{text, Text},
    ui::{Area, Ui, Window},
    widgets::PassiveWidget,
    BREAK_LOOP, CURRENT_FILE, CURRENT_WIDGET, SHOULD_QUIT,
};

/// Contains all of the functions that are meant to be re-exported for
/// global access.
mod global {
    use std::{iter::Peekable, str::SplitWhitespace, sync::atomic::Ordering};

    use super::{CmdResult, Commands, Error, InnerFlags};
    use crate::{
        data::RwData,
        text::Text,
        ui::{Area, Ui, Window},
        widgets::{ActiveWidget, PassiveWidget},
        BREAK_LOOP, SHOULD_QUIT,
    };

    static COMMANDS: Commands = Commands::new();

    pub type Args<'a, 'b> = &'a mut Peekable<SplitWhitespace<'b>>;
    pub type Flags<'a, 'b> = &'a InnerFlags<'b>;

    /// Adds a command to the global list of commands.
    ///
    /// This command cannot take any arguments beyond the [`Flags`]
    /// and [`Args`], so any mutation of state must be done
    /// through captured variables, usually in the form of
    /// [`RwData<T>`]s.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use parsec_core::{
    /// #     commands,
    /// #     data::RwData,
    /// #     text::{text, Text},
    /// #     widgets::status
    /// # };
    /// // Shared state, which will be displayed in a `StatusLine`.
    /// let var = RwData::new(35);
    ///
    /// commands::add(["set-var"], {
    ///     // A clone is necessary, in order to have one copy of `var`
    ///     // in the closure, while the other is in the `StatusLine`.
    ///     let var = var.clone();
    ///     move |_flags, args| {
    ///         let value: usize = args
    ///             .next()
    ///             .ok_or(text!("No value given."))?
    ///             .parse()
    ///             .map_err(Text::from)?;
    ///
    ///         *var.write() = value;
    ///
    ///         Ok(None)
    ///     }
    /// });
    ///
    /// // A `StatusLineCfg` that can be used to create a `StatusLine`.
    /// let status_cfg = status!("The value is currently " var);
    /// ```
    ///
    /// In the above example, we created a variable that can be
    /// modified by the command `"set-var"`, and then sent it to a
    /// [`StatusLineCfg`], so that it could be displayed in a
    /// [`StatusLine`]. Note that the use of an [`RwData<usize>`]/
    /// [`RoData<usize>`] means that the [`StatusLine`] will
    /// be updated automatically, whenever the command is ran.
    ///
    /// [`StatusLineCfg`]: crate::widgets::StatusLineCfg
    /// [`StatusLine`]: crate::widgets::StatusLine
    /// [`RoData<usize>`]: crate::data::RoData
    pub fn add(
        callers: impl IntoIterator<Item = impl ToString>,
        f: impl FnMut(Flags, Args) -> CmdResult + 'static,
    ) -> std::result::Result<(), Error> {
        COMMANDS.add(callers, f)
    }

    /// Adds a command that can mutate a widget of the given type,
    /// along with its associated [`dyn Area`].
    ///
    /// This command will look for the given [`PassiveWidget`] in the
    /// following order:
    ///
    /// 1. Any instance that is "related" to the currently active
    ///    [`File`], that is, any widgets that were added during the
    ///    [`Session`]'s "`file_fn`".
    /// 2. Other widgets in the currently active window, related or
    ///    not to any given [`File`].
    /// 3. Any instance of the [`PassiveWidget`] that is found in
    ///    other windows, looking first at windows ahead.
    ///
    /// Keep in mind that the search is deterministic, that is, if
    /// there are multiple instances of the widget that fit the
    /// same category, only one of them will ever be used.
    ///
    /// This search algorithm allows a more versatile configuration of
    /// widgets, for example, one may have a [`CommandLine`] per
    /// [`File`], or one singular [`CommandLine`] that acts upon
    /// all files in the window, and both would respond correctly
    /// to the `"set-prompt"` command.
    ///
    /// # Examples
    ///
    /// In this example, we create a simple `Timer` widget, along with
    /// some control commands.
    ///
    /// ```rust
    /// // Required feature for widgets.
    /// #![feature(return_position_impl_trait_in_trait)]
    /// use std::{
    ///    sync::{
    ///        atomic::{AtomicBool, Ordering},
    ///        Arc,
    ///    },
    ///    time::Instant,
    /// };
    /// use parsec_core::{
    ///    commands,
    ///    palette::{self, Form},
    ///    text::{text, Text, AlignCenter},
    ///    ui::{Area, PushSpecs, Ui},
    ///    widgets::{PassiveWidget, Widget},
    /// };
    /// pub struct Timer {
    ///    text: Text,
    ///    instant: Instant,
    ///    running: Arc<AtomicBool>,
    /// }
    ///
    /// impl PassiveWidget for Timer {
    ///     fn build<U: Ui>() -> (Widget<U>, impl Fn() -> bool, PushSpecs) {
    ///         let timer = Self {
    ///             text: text!(AlignCenter [Counter] "0ms"),
    ///             instant: Instant::now(),
    ///             // No need to use an `RwData`, since
    ///             // `RwData::has_changed` is never called.
    ///             running: Arc::new(AtomicBool::new(false)),
    ///         };
    ///
    ///         // The checker should tell the `Timer` to update only
    ///         // if `running` is `true`.
    ///         let checker = {
    ///             // Clone any variables before moving them to
    ///             // the `checker`.
    ///             let running = timer.running.clone();
    ///             move || running.load(Ordering::Relaxed)
    ///         };
    ///
    ///         let specs = PushSpecs::below().with_lenght(1.0);
    ///
    ///         (Widget::passive(timer), checker, specs)
    ///     }
    ///
    ///     fn update(&mut self, _area: &impl Area) {
    ///         if self.running.load(Ordering::Relaxed) {
    ///             let duration = self.instant.elapsed();
    ///             let duration = format!("{:.3?}", duration);
    ///             self.text = text!(
    ///                 AlignCenter [Counter] duration
    ///                 [Default] "elapsed"
    ///             );
    ///         }
    ///     }
    ///
    ///     fn text(&self) -> &Text {
    ///         &self.text
    ///     }
    ///
    ///     // The `once` function of a `PassiveWidget` is only called
    ///     // when that widget is first created.
    ///     // It is generally useful to add commands and set forms
    ///     // in the `palette`.
    ///     fn once() {
    ///         // `palette::set_weak_form` will only set that form if
    ///         // it doesn't already exist.
    ///         // That means that a user of the widget will be able to
    ///         // control that form by changing it before or after this
    ///         // widget is pushed.
    ///         palette::set_weak_form("Counter", Form::new().green());
    ///
    ///         commands::add_for_widget::<Timer>(
    ///             ["play"],
    ///             |timer, _area, _flags, _args| {
    ///                 timer.running.store(true, Ordering::Relaxed);
    ///
    ///                 Ok(None)
    ///             })
    ///             .unwrap();
    ///
    ///         commands::add_for_widget::<Timer>(
    ///             ["pause"],
    ///             |timer, _area, _flags, _args| {
    ///                 timer.running.store(false, Ordering::Relaxed);
    ///
    ///                 Ok(None)
    ///             })
    ///             .unwrap();
    ///
    ///         commands::add_for_widget::<Timer>(
    ///             ["pause"],
    ///             |timer, _area, _flags, _args| {
    ///                 timer.instant = Instant::now();
    ///
    ///                 Ok(None)
    ///             })
    ///             .unwrap();
    ///     }
    /// }
    /// ```
    ///
    /// [`dyn Area`]: crate::ui::Area
    /// [`File`]: crate::widgets::File
    /// [`Session`]: crate::session::Session
    /// [`CommandLine`]: crate::widgets::CommandLine
    pub fn add_for_widget<Widget: PassiveWidget>(
        callers: impl IntoIterator<Item = impl ToString>,
        f: impl FnMut(&mut Widget, &dyn Area, Flags, Args) -> CmdResult + 'static,
    ) -> Result<(), Error> {
        COMMANDS.add_for_widget(callers, f)
    }

    pub fn run(command: impl ToString) -> Result<Option<Text>, Error> {
        COMMANDS.run(command)
    }

    pub fn quit() {
        BREAK_LOOP.store(true, Ordering::Release);
        SHOULD_QUIT.store(true, Ordering::Release);
    }

    pub fn switch_to<W: ActiveWidget>() -> Result<Option<Text>, Error> {
        COMMANDS.run(format!("switch-to {}", W::type_name()))
    }

    pub fn buffer(file: impl AsRef<str>) -> Result<Option<Text>, Error> {
        COMMANDS.run(format!("buffer {}", file.as_ref()))
    }

    pub fn next_file() -> Result<Option<Text>, Error> {
        COMMANDS.run("next-file")
    }

    pub fn prev_file() -> Result<Option<Text>, Error> {
        COMMANDS.run("prev-file")
    }

    pub fn return_to_file() -> Result<Option<Text>, Error> {
        COMMANDS.run("return-to-file")
    }

    pub(crate) fn add_widget_getter<U: Ui>(getter: RwData<Vec<Window<U>>>) {
        COMMANDS.add_widget_getter(getter);
    }

    /// Tries to alias a `caller` to an existing `command`.
    ///
    /// Returns an [`Err`] if the `caller` is already a caller for
    /// another command, or if `command` is not a real caller to an
    /// exisiting [`Command`].
    pub fn alias<'a>(
        alias: impl ToString,
        command: impl IntoIterator<Item = &'a str>,
    ) -> Result<Option<Text>, Error> {
        COMMANDS.inner.write().try_alias(alias, command)
    }
}

/// A struct representing flags passed down to [`Command`]s when
/// running them.
///
/// There are 2 types of flag, the `blob` and `unit` flags.
///
/// `blob` flags represent singular characters passed after a single
/// `'-'` character, they can show up in multiple places, and should
/// represent an incremental addition of features to a command.
///
/// `unit` flags are words that come after any `"--"` sequence, and
/// should represent more verbose, but more readable versions of
/// `blob` flags.
///
/// # Examples
///
/// Both `blob` and `unit` flags can only be counted once, no matter
/// how many times they show up:
///
/// ```rust
/// # use parsec_core::commands::{split_flags, Flags};
/// let command = "my-command --foo --bar -abcde --foo --baz -abfgh arg1";
/// let mut command_args = command.split_whitespace().skip(1);
/// let (flags, args) = split_flags(command_args);
///
/// assert!(flags.blob == String::from("abcdefgh"));
/// assert!(flags.units == vec!["foo", "bar", "baz"]);
/// ```
///
/// If you have any arguments that start with `'-'` or `"--"`, but are
/// not supposed to be flags, you can insert an empty `"--"` after the
/// flags, in order to distinguish them.
///
/// ```rust
/// # use parsec_core::commands::{split_flags, Flags};
/// let command =
///     "my-command --foo --bar -abcde -- --not-a-flag -also-not-flags";
/// let mut command_args = command.split_whitespace().skip(1);
/// let (flags, args) = split_flags(command_args);
///
/// assert!(flags.blob == String::from("abcde"));
/// assert!(flags.units == vec!["foo", "bar"]);
/// ```
pub struct InnerFlags<'a> {
    pub blob: String,
    pub units: Vec<&'a str>,
}

impl<'a> InnerFlags<'a> {
    /// Checks if all of the [`char`]s in the `blob` passed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use parsec_core::commands::{split_flags, Flags};
    /// let command = "run -abcdefgh -ablk args -wz";
    /// let mut command_args = command.split_whitespace().skip(1);
    /// let (flags, args) = split_flags(command_args);
    ///
    /// assert!(flags.blob("k"));
    /// assert!(!flags.blob("w"));
    /// ```
    pub fn blob(&self, blob: impl AsRef<str>) -> bool {
        let mut all_chars = true;
        for char in blob.as_ref().chars() {
            all_chars &= self.blob.contains(char);
        }
        all_chars
    }

    /// Returns `true` if the `unit` flag was passed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use parsec_core::commands::{split_flags, Flags};
    /// let command = "run --foo --bar args -baz";
    /// let mut command_args = command.split_whitespace().skip(1);
    /// let (flags, args) = split_flags(command_args);
    ///
    /// assert!(flags.unit("foo"));
    /// assert!(!flags.unit("baz"));
    /// ```
    pub fn unit(&self, flag: impl AsRef<str>) -> bool {
        self.units.contains(&flag.as_ref())
    }

    /// Returns `true` if no flags have been passed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use parsec_core::commands::{split_flags, Flags};
    /// let command = "run arg1 --foo --bar arg2 -baz";
    /// let mut command_args = command.split_whitespace().skip(1);
    /// let (flags, args) = split_flags(command_args);
    ///
    /// assert!(flags.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.blob.is_empty() && self.units.is_empty()
    }
}

/// A function to be used on hooks or in the [`CommandLine`].
///
/// The [`Command`] takes in two vectors of [`String`]s, the first
/// represents the `flags` passed on to the [`Command`], while the
/// second is a list of `arguments`.
///
/// # Examples
///
/// When creating a [`Command`], one should prioritize the reduction
/// of "data" that the [`Command`] uses when triggering.
///
/// As an example, let's say you're creating a [`Widget`],
/// as seen on the `struct` below.
///
/// ```rust
/// # use parsec_core::{text::Text, ui::Ui};
/// # use std::sync::{atomic::AtomicBool, Arc};
/// struct MyWidget {
///     text: Text,
///     other_field: String,
///     relevant_field: Arc<AtomicBool>,
/// }
/// ```
///
/// In this case, assuming that you will add the [`Command`]s through
/// the closure, capturing one [`RwData<MyWidget<U>>`]. You should do
/// this:
///
/// ```rust
/// # use parsec_core::{
/// #     commands::{Command, CommandErr, Commands, Flags},
/// #     data::{RwData, ReadableData},
/// #     text::Text,
/// #     ui::Ui
/// # };
/// # use std::sync::{
/// #     atomic::{AtomicBool, Ordering},
/// #     Arc
/// # };
/// #
/// # struct MyWidget {
/// #     text: Text,
/// #     other_field: String,
/// #     relevant_field: Arc<AtomicBool>
/// # }
/// #
/// # fn command_function(
/// #     relevant_field: bool, flags: &Flags,
/// #     args: &mut dyn Iterator<Item = &str>
/// # ) -> Result<Option<String>, String> {
/// #     todo!();
/// # }
/// #
/// fn add_commands(
///     widget: RwData<MyWidget>,
///     commands: &mut Commands,
/// ) -> Result<(), CommandErr> {
///     // Cloning the `relevant_field`, the only important part of
///     // `MyWidget` for the command.
///     let relevant_field = widget.read().relevant_field.clone();
///     let callers = vec!["my-function-caller"];
///     let command = Command::new(callers, move |flags, args| {
///         command_function(
///             relevant_field.load(Ordering::Relaxed),
///             flags,
///             args,
///         )
///     });
///     commands.try_add(command)
/// }
/// ```
///
/// Instead of this:
///
/// ```rust
/// # use parsec_core::{
/// #     commands::{Command, CommandErr, Commands, Flags},
/// #     data::{RwData, ReadableData},
/// #     text::Text,
/// #     ui::Ui
/// # };
/// # use std::sync::{
/// #     atomic::{AtomicBool, Ordering},
/// #     Arc
/// # };
/// #
/// # struct MyWidget {
/// #     text: Text,
/// #     other_field: String,
/// #     relevant_field: Arc<AtomicBool>
/// # }
/// #
/// # fn command_function(
/// #     relevant_field: bool, flags: &Flags,
/// #     args: &mut dyn Iterator<Item = &str>
/// # ) -> Result<Option<String>, String> {
/// #     todo!();
/// # }
/// #
/// fn add_commands(
///     widget: RwData<MyWidget>,
///     commands: &mut Commands,
/// ) -> Result<(), CommandErr> {
///     let callers = vec!["my-function-caller"];
///     let command = Command::new(callers, move |flags, args| {
///         // Here, the whole widget got moved, which is bad, for
///         // thread safety reasons.
///         let relevant_field =
///             widget.read().relevant_field.load(Ordering::Relaxed);
///         command_function(relevant_field, flags, args)
///     });
///     commands.try_add(command)
/// }
/// ```
///
/// In the second version, the whole `widget` variable gets moved into
/// the closure. The problem with this is that this specific
/// [`RwData`] will get used very often, and by
/// running the [`Command`], you may cause a deadlock, which is really
/// annoying to diagnose.
///
/// As an example, there is the
/// [`CommandLine`] widget. If its
/// [`Command`]s moved an entire
/// [`RwData<CommandLine<U>>`] to the
/// closures, every single one of them, when triggered through the
/// widget, would result in a deadlock, since they're writing to an
/// [`RwData`] that was already being written
/// to.
///
/// [`Controler`]: crate::Controler
/// [`RwData<MyWidget<U>>`]: crate::data::RwData
/// [`Session`]: crate::session::Session
/// [`Session::commands`]: crate::session::Session::commands
/// [`ModNode`]: crate::ui::ModNode
/// [`ModNode::commands`]: crate::ui::ModNode::commands
/// [`CommandLine`]: crate::widgets::CommandLine
/// [`FileWidget`]: crate::widgets::FileWidget
/// [`Widget`]: crate::widgets::Widget
struct Command {
    /// A closure to trigger when any of the `callers` are called.
    ///
    /// # Arguments
    ///
    /// - 1: A [`&Flags`][Flags] containing the flags that have been
    ///   passed to the function.
    /// - 2: An [`&mut dyn Iterator<Item = &str>`][Iterator]
    ///   representing the arguments to be read by the function.
    ///
    /// # Returns
    ///
    /// A [`Result<Option<String>, String>`]. [`Ok(Some(String))`] is
    /// an outcome that could be used to chain multiple commands.
    /// The [`String`] in [`Err(String)`] is used to tell the user
    /// what went wrong while running the command, and possibly to
    /// show a message somewhere on Parsec.
    f: RwData<dyn FnMut(Flags, Args) -> CmdResult>,
    /// A list of [`String`]s that act as callers for [`self`].
    callers: Vec<String>,
}

impl Command {
    /// Returns a new instance of [`Command`].
    ///
    /// # Arguments
    ///
    /// - `callers`: A list of names to call this function by. In
    ///   other editors, you would see things like `["edit", "e"]`, or
    ///   `["write-quit", "wq"]`, and this is no different.
    /// - `f`: The closure to call when running the command. In order
    ///   for the closure to actually do anything, you would want it
    ///   to capture shared, multi-threaded objects (e.g. [`Arc`]s,
    ///   [`RwData`]s, e.t.c.).
    /// ```rust
    /// # use parsec_core::{
    /// #     commands::{Command},
    /// #     data::{RwData, ReadableData},
    /// # };
    /// # use std::sync::atomic::AtomicBool;
    /// let my_var = RwData::new(AtomicBool::new(false));
    /// let clone_of_my_var = my_var.clone();
    /// let callers = ["foo", "bar"];
    /// let command = Command::new(callers, move |flags, args| {
    ///     let read_var = my_var.read();
    ///     todo!();
    /// });
    /// ```
    ///
    ///   This way, `my_var` will be accessible, both to the command,
    ///   and to any reader that is interested in its value.
    ///
    /// # Panics
    ///
    /// This function will panic if any of the callers contain more
    /// than one word, as commands are only allowed to be one word
    /// long.
    fn new<F>(callers: impl IntoIterator<Item = impl ToString>, f: F) -> Self
    where
        F: FnMut(Flags, Args) -> CmdResult + 'static,
    {
        let callers = callers
            .into_iter()
            .map(|caller| caller.to_string())
            .collect::<Vec<String>>();
        if let Some(caller) = callers
            .iter()
            .find(|caller| caller.split_whitespace().count() != 1)
        {
            panic!("Command caller \"{caller}\" contains more than one word.");
        }
        Self {
            f: RwData::new_unsized::<F>(Arc::new(RwLock::new(f))),
            callers,
        }
    }

    /// Executes the inner function if the `caller` matches any of the
    /// callers in [`self`].
    fn try_exec(
        &self,
        caller: &str,
        flags: Flags,
        args: Args<'_, '_>,
    ) -> Result<Option<Text>, Error> {
        if self.callers.iter().any(|name| name == caller) {
            (self.f.write())(flags, args).map_err(Error::Failed)
        } else {
            Err(Error::CallerNotFound(String::from(caller)))
        }
    }

    /// The list of callers that will trigger this command.
    fn callers(&self) -> &[String] {
        &self.callers
    }
}

unsafe impl Send for Command {}
unsafe impl Sync for Command {}

/// A list of [`Command`]s, meant to be used in a
/// [`CommandLine`].
///
/// [`Command`]s can be added through the [`Commands::try_add`]
/// function, they can have multiple callers, and return
/// [`Result<Option<String>, String>`]
///
/// ```rust
/// # use parsec_core::{
/// #     commands::{Command, Commands},
/// #     data::RwData
/// # };
/// #
/// # fn test_fn(commands: &mut Commands) {
/// let callers = vec!["run-cmd"];
/// let command = Command::new(callers, |flags, _| {
///     if flags.unit("success") {
///         Ok(Some(String::from("✅")))
///     } else if flags.unit("failure") {
///         Err(String::from("❎"))
///     } else {
///         Ok(None)
///     }
/// });
/// commands.try_add(command).unwrap();
/// # }
/// ```
///
/// This is a [`Command`] that analyzes the [`Flags`] that were
/// passed, checking for the `"success"` and `"failure"` flags.
///
/// By default, one command will always be available, that
/// being the `"alias"` command This is its calling syntax:
///
/// `
/// "alias command-alias command-caller --flag1 --flag2 -blobs --flag1
/// -abc args" `
///
/// This call will alias the `"alias-name"` keyword to the
/// `"aliased-command"` caller. If the caller doesn't exist, the
/// `"alias"` [`Command`] will return an [`Err`].
///
/// When running the `"command-alias"`, this alias will also pass
/// the following [`Flags`]:
///
/// ```rust
/// # use parsec_core::commands::Flags;
/// # fn test_fn<'a>() -> Flags<'a> {
/// Flags {
///     blob: String::from("blosac"),
///     units: vec!["flag1", "flag2"],
/// }
/// # }
/// ```
///
/// And will also pass `"args"` as an argument for the
/// `"command-caller"` command.
///
/// [`CommandLine`]: crate::widgets::CommandLine
/// [`Commands::try_add`]: Commands::try_add
struct Commands {
    inner: LazyLock<RwData<InnerCommands>>,
    widget_getter: RwLock<MaybeUninit<RwData<dyn WidgetGetter>>>,
}

impl Commands {
    /// Returns a new instance of [`Commands`].
    ///
    /// This function is primarily used in the creation of a
    /// [`Controler`], for bookkeeping of the available
    /// [`Command`]s in an accessible place.
    ///
    /// It also has the purpose of preloading [`self`] with the
    /// `"alias"` command, allowing the end user to alias callers
    /// into full commands.
    ///
    /// [`Controler`]: crate::Controler
    const fn new() -> Self {
        Self {
            inner: LazyLock::new(|| {
                let inner = RwData::new(InnerCommands {
                    list: Vec::new(),
                    aliases: HashMap::new(),
                });

                let alias = {
                    let inner = inner.clone();
                    Command::new(["alias"], move |flags, args| {
                        if !flags.is_empty() {
                            Err(text!(
                                "An alias cannot take any flags, try moving them after the \
                                 command, like \"alias my-alias my-caller --foo --bar\", instead \
                                 of \"alias --foo --bar my-alias my-caller\""
                            ))
                        } else {
                            let alias = args.next().ok_or(text!("No alias was supplied."))?;

                            inner
                                .write()
                                .try_alias(alias, args)
                                .map_err(Error::into_text)
                        }
                    })
                };

                let quit = {
                    Command::new(["quit", "q"], move |_, _| {
                        BREAK_LOOP.store(true, std::sync::atomic::Ordering::Relaxed);
                        SHOULD_QUIT.store(true, std::sync::atomic::Ordering::Relaxed);

                        Ok(None)
                    })
                };

                inner.write().try_add(quit).unwrap();
                inner.write().try_add(alias).unwrap();

                inner
            }),
            widget_getter: RwLock::new(MaybeUninit::uninit()),
        }
    }

    /// Parses the `command`, dividing it into a caller, flags, and
    /// args, and then tries to run it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use parsec_core::{
    /// #     commands::{Command, Commands},
    /// #     data::{ReadableData, RwData}
    /// # };
    /// # fn test_fn(commands: &mut Commands) {
    /// let my_var = RwData::new(String::new());
    /// let my_var_clone = my_var.clone();
    ///
    /// let command = Command::new(["foo", "bar", "baz"], move |_flags, args| {
    ///     if let Some(arg) = args.next() {
    ///         *my_var_clone.write() = String::from(arg);
    ///     } else {
    ///         *my_var_clone.write() = String::from("😿");
    ///     }
    ///
    ///     Ok(None)
    /// });
    ///
    /// commands.try_add(command).unwrap();
    ///
    /// assert!(&*my_var.read() == "");
    ///
    /// commands.try_exec("foo hello-pardner!");
    /// assert!(&*my_var.read() == "hello-pardner!");
    ///
    /// commands.try_exec("bar --my-flag-here -my-blobs-here arrrgh!");
    /// assert!(&*my_var.read() == "arrrgh!");
    ///
    /// commands.try_exec("baz --args=none-lol");
    /// assert!(&*my_var.read() == "😿");
    /// # }
    /// ```
    fn run(&self, command: impl ToString) -> Result<Option<Text>, Error> {
        self.inner.read().run(command)
    }

    /// Tries to add a new [`Command`].
    ///
    /// Returns an [`Err`] if any of the callers of the [`Command`]
    /// are already callers for other commands.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use parsec_core::{
    /// #     commands::{Command, Commands},
    /// #     data::{ReadableData, RwData}
    /// # };
    /// # fn test_fn(commands: &mut Commands) {
    /// let capitalize = Command::new(["capitalize", "cap"], |_flags, args| {
    ///     Ok(Some(
    ///         args.map(|arg| arg.to_string().to_uppercase())
    ///             .collect::<String>(),
    ///     ))
    /// });
    ///
    /// assert!(commands.try_add(capitalize).is_ok());
    ///
    /// let capitals = Command::new(["capitals", "cap"], |_flags, args| {
    ///     if let Some("USA!!1!1!") = args.next() {
    ///         Ok(Some(String::from("Washington")))
    ///     } else {
    ///         Ok(Some(String::from("idk")))
    ///     }
    /// });
    ///
    /// let result = format!("{:?}", commands.try_add(capitals));
    /// assert!(result == "The caller \"cap\" already exists.");
    /// # }
    /// ```
    fn add(
        &self,
        callers: impl IntoIterator<Item = impl ToString>,
        f: impl FnMut(Flags, Args) -> CmdResult + 'static,
    ) -> Result<(), Error> {
        let command = Command::new(callers, f);
        self.inner.write().try_add(command)
    }

    fn add_for_widget<W: PassiveWidget>(
        &self,
        callers: impl IntoIterator<Item = impl ToString>,
        mut f: impl FnMut(&mut W, &dyn Area, Flags, Args) -> CmdResult + 'static,
    ) -> Result<(), Error> {
        let widget_getter = unsafe { self.widget_getter.read().unwrap().assume_init_ref().clone() };

        let command = Command::new(callers, move |flags, args| {
            let widget_getter = widget_getter.read();

            CURRENT_FILE
                .mutate_related_widget::<W, CmdResult>(|widget, area| f(widget, area, flags, args))
                .unwrap_or_else(|| {
                    CURRENT_WIDGET.inspect_data(|widget, _| {
                        let widget = widget.clone().to_passive();
                        if let Some((w, a)) = widget_getter.get_from_name(W::type_name(), &widget) {
                            w.mutate_as::<W, CmdResult>(|w| f(w, a, flags, args))
                                .unwrap()
                        } else {
                            Err(text!("No widget of type \"" {W::type_name()} "\" found"))
                        }
                    })
                })
        });

        self.inner.write().try_add(command)
    }

    fn add_widget_getter<U: Ui>(&self, getter: RwData<Vec<Window<U>>>) {
        let inner_arc = getter.inner_arc().clone() as Arc<RwLock<dyn WidgetGetter>>;
        let getter = RwData::new_unsized::<Window<U>>(inner_arc);
        let mut lock = self.widget_getter.write().unwrap();
        *lock = MaybeUninit::new(getter)
    }
}

struct InnerCommands {
    list: Vec<Command>,
    aliases: HashMap<String, String>,
}

impl InnerCommands {
    /// Tries to add the given [`Command`] to the list.
    fn try_add(&mut self, command: Command) -> Result<(), Error> {
        let mut new_callers = command.callers().iter();

        let commands = self.list.iter();
        for caller in commands.flat_map(|cmd| cmd.callers().iter()) {
            if new_callers.any(|new_caller| new_caller == caller) {
                return Err(Error::AlreadyExists(caller.clone()));
            }
        }

        self.list.push(command);

        Ok(())
    }

    /// Tries to execute a command or alias with the given argument.
    fn run(&self, command: impl ToString) -> Result<Option<Text>, Error> {
        let command = command.to_string();
        let caller = command
            .split_whitespace()
            .next()
            .ok_or(Error::Empty)?
            .to_string();

        let command = self.aliases.get(&caller).cloned().unwrap_or(command);
        let mut command = command.split_whitespace();
        let caller = command.next().unwrap();

        let (flags, mut args) = split_flags(command);
        for cmd in &self.list {
            let result = cmd.try_exec(caller, &flags, &mut args);
            let Err(Error::CallerNotFound(_)) = result else {
                return result;
            };
        }

        Err(Error::CallerNotFound(String::from(caller)))
    }

    /// Tries to alias a full command (caller, flags, and arguments)
    /// to an alias.
    fn try_alias<'a>(
        &mut self,
        alias: impl ToString,
        command: impl IntoIterator<Item = &'a str>,
    ) -> Result<Option<Text>, Error> {
        let alias = alias.to_string();
        let mut command = command.into_iter();

        if alias.split_whitespace().count() != 1 {
            return Err(Error::NotSingleWord(alias));
        }
        let caller = String::from(command.next().ok_or(Error::Empty)?);

        let mut callers = self.list.iter().flat_map(|cmd| cmd.callers.iter());

        if callers.any(|name| *name == caller) {
            let command = caller + command.collect::<String>().as_str();
            match self.aliases.insert(alias.clone(), command.clone()) {
                Some(prev) => Ok(Some(text!(
                    "Aliased \"" [CmdOkHighlight] alias
                    "\" from \"" [CmdOkHighlight] prev
                    "\" to \"" [CmdOkHighlight] command "\"."
                ))),
                None => Ok(Some(text!(
                     "Aliased \"" [CmdOkHighlight] alias
                     "\" to \"" [CmdOkHighlight] command "\"."
                ))),
            }
        } else {
            Err(Error::CallerNotFound(caller))
        }
    }
}

trait WidgetGetter: Send + Sync {
    fn get_from_name(
        &self,
        type_id: &'static str,
        arc: &dyn Data<dyn PassiveWidget>,
    ) -> Option<(&RwData<dyn PassiveWidget>, &dyn Area)>;
}

impl<U> WidgetGetter for Vec<Window<U>>
where
    U: Ui,
{
    fn get_from_name(
        &self,
        type_name: &'static str,
        widget: &dyn Data<dyn PassiveWidget>,
    ) -> Option<(&RwData<dyn PassiveWidget>, &dyn Area)> {
        let window = self
            .iter()
            .position(|w| w.widgets().any(|(cmp, _)| cmp.ptr_eq(widget)))
            .unwrap();

        let on_window = self[window].widgets();
        let previous = self.iter().take(window).flat_map(|w| w.widgets());
        let following = self.iter().skip(window + 1).flat_map(|w| w.widgets());

        on_window
            .chain(following)
            .chain(previous)
            .find(|(w, _)| w.type_name() == type_name)
            .map(|(w, a)| (w.as_passive(), a as &dyn Area))
    }
}

/// Takes the [`Flags`] from an [`Iterator`] of `args`.
///
/// # Examples
///
/// ```rust
/// # use parsec_core::commands::{split_flags, Flags};
/// let command = "my-command --foo --bar -abcde --foo --baz -abfgh arg1";
/// let mut command_args = command.split_whitespace().skip(1);
/// let (flags, args) = split_flags(command_args);
///
/// assert!(flags.blob == String::from("abcdefgh"));
/// assert!(flags.units == vec!["foo", "bar", "baz"]);
/// ```
///
/// If you have any arguments that start with `'-'` or `"--"`, but are
/// not supposed to be flags, you can insert an empty `"--"` after the
/// flags, in order to distinguish them.
///
/// ```rust
/// # use parsec_core::commands::{split_flags, Flags};
/// let command =
///     "my-command --foo --bar -abcde -- --not-a-flag -also-not-flags";
/// let mut command_args = command.split_whitespace().skip(1);
/// let (flags, args) = split_flags(command_args);
///
/// assert!(flags.blob == String::from("abcde"));
/// assert!(flags.units == vec!["foo", "bar"]);
/// ```
fn split_flags(
    args: std::str::SplitWhitespace<'_>,
) -> (
    InnerFlags<'_>,
    std::iter::Peekable<std::str::SplitWhitespace<'_>>,
) {
    let mut blob = String::new();
    let mut units = Vec::new();

    let mut args = args.peekable();
    while let Some(arg) = args.peek() {
        if let Some(flag_arg) = arg.strip_prefix("--") {
            if !flag_arg.is_empty() {
                if !units.contains(&flag_arg) {
                    units.push(flag_arg)
                }
                args.next();
            } else {
                break;
            }
        } else if let Some(blob_arg) = arg.strip_prefix('-') {
            for char in blob_arg.chars() {
                if !blob.contains(char) {
                    blob.push(char)
                }
            }
            args.next();
        } else {
            break;
        }
    }

    (InnerFlags { blob, units }, args)
}

/// An failure in executing or adding a [`Command`].
#[derive(Debug)]
pub enum Error {
    NotSingleWord(String),
    AlreadyExists(String),
    CallerNotFound(String),
    Failed(Text),
    Empty,
}

impl Error {
    fn into_text(self) -> Text {
        match self {
            Error::NotSingleWord(caller) => text!(
                "The caller \""
                [CmdErrHighlight] caller
                "is not a single word."
            ),
            Error::AlreadyExists(caller) => text!(
                "The caller \""
                [CmdErrHighlight] caller
                "already exists."
            ),
            Error::CallerNotFound(caller) => text!(
                "The caller \""
                [CmdErrHighlight] caller
                "was not found."
            ),
            Error::Failed(failure) => failure,
            Error::Empty => text!("No text or arguments were provided."),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotSingleWord(caller) => {
                write!(f, "The caller \"{caller}\" is not a single word")
            }
            Error::AlreadyExists(caller) => {
                write!(f, "The caller \"{caller}\" already exists.")
            }
            Error::CallerNotFound(caller) => {
                write!(f, "The caller \"{caller}\" was not found.")
            }
            Error::Failed(failure) => f.write_str(&failure.chars().collect::<String>()),
            Error::Empty => f.write_str("No caller supplied."),
        }
    }
}

impl std::error::Error for Error {}

pub type CmdResult = Result<Option<Text>, Text>;
