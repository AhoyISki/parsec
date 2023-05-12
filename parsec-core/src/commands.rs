//! Creation and execution of [`Command`]s.
//!
//! The [`Command`] struct, in Parsec, is function that's supposed to
//! be ran from any part of the program asynchronously, being able to
//! mutate data through reference counting and internal mutability.
//!
//! [`Command`]s must act on 2 specific parameters, [`Flags`], and
//! arguments coming from an [`Iterator<Item = &str>`].
//!
//! The [`Flags`] struct contains both `unit` (e.g `"--force"`,
//! `"--read"`, etc), and `blob` (e.g. `"-abcde"`, `"-amongus"`, etc)
//! flags inside of it, and they stop being considered after the first
//! empty `"--"` sequence, for example, in the command:
//!
//! `"my-command --flag1 --flag2 -blob -- --not-flag more-args"`
//!
//! `"--not-a-flag"` would not be treated as a flag, and would instead
//! be treated as an argument in conjunction with `"more-args"`. This
//! interface is heavily inspired by commands in UNIX like operating
//! systems.
//!
//! The [`Command`]'s function will have the type
//! `
//! Box<dyn FnMut(&Flags, &mut dyn Iterator<Item = &str>) ->
//! Result<Option<String>, String>> `
//! which seems complex, but in most cases, a [`Command`] will just be
//! created from a closure like so:
//!
//! ```rust
//! # use parsec_core::commands::{Command, Flags};
//! # use std::sync::{
//! #     atomic::{AtomicBool, Ordering},
//! #     Arc
//! # };
//! #
//! let internally_mutable = Arc::new(AtomicBool::default());
//! let callers = vec!["my-command", "mc"];
//! let my_command = Command::new(callers, move |flags, args| {
//!     todo!();
//! });
//! ```
//!
//! In this case, a [`Command`] is created that can be called with
//! both `"my-command"` and `"mc"`.
//!
//! Here's a simple command that uses the [`Flags`] struct:
//!
//! ```rust
//! # use parsec_core::commands::{Command, Flags};
//! # use std::sync::{
//! #     atomic::{AtomicU32, Ordering},
//! #     Arc
//! # };
//! #
//! let expression = Arc::new(AtomicU32::default());
//! let callers = vec!["my-command", "mc"];
//! let my_command = Command::new(callers, move |flags, args| {
//!     if flags.unit("happy") {
//!         expression.store('😁' as u32, Ordering::Relaxed)
//!     } else if flags.unit("sad") {
//!         expression.store('😢' as u32, Ordering::Relaxed)
//!     } else {
//!         expression.store('😶' as u32, Ordering::Relaxed)
//!     }
//!     Ok(None)
//! });
//! ```
//!
//! The calling of [`Command`]s is done through the [`Commands`]
//! struct, shared around with an
//! [`RwData<Commands>`][crate::data::RwData].
//!
//! This struct is found in the [`Session<U>`][crate::Session], either
//! through the `constructor_hook` via a
//! [`ModNode`][crate::ui::ModNode], or directly:
//!
//! ```
//! # use parsec_core::{
//! #     commands::{Command, Commands},
//! #     data::RwData,
//! #     tags::form::FormPalette,
//! #     text::PrintCfg,
//! #     ui::{ModNode, PushSpecs, Side, Split, Ui},
//! #     widgets::CommandLine,
//! #     Session
//! # };
//! # fn test_fn<U>(ui: U, print_cfg: PrintCfg, palette: FormPalette)
//! # where
//! #     U: Ui
//! # {
//! let constructor_hook = move |mod_node: ModNode<U>, file| {
//!     let commands = mod_node.manager().commands();
//!     commands.write().try_exec("lol");
//!
//!     let push_specs = PushSpecs::new(Side::Bottom, Split::Locked(1));
//!     mod_node.push_widget(CommandLine::default_fn(), push_specs);
//! };
//!
//! let session =
//!     Session::new(ui, print_cfg, palette, constructor_hook);
//!
//! let my_callers = vec!["lol", "lmao"];
//! let lol_cmd = Command::new(my_callers, |_, _| {
//!     Ok(Some(String::from("😜")))
//! });
//!
//! session.manager().commands().write().try_add(lol_cmd);
//! # }
//! ```
//!
//! The [`Commands`] struct, in this snippet, is chronologically first
//! accessed through the
//! `session.manager().commands().write().try_add(lol_cmd);` line. In
//! this line, the [`Command`] `lol_cmd` is added to the list of
//! [`Command`]s, with 2 callers, simply returning a `"😜"`, which can
//! be chained by other [`Command`]s or used in some other way.
//!
//! It is then accessed by the `constructor_hook`, where the `"lol"`
//! caller is called upon, executing the `lol_cmd`. Also in the
//! `constructor_hook`, a
//! [`CommandLine<U>`][crate::widgets::CommandLine] is pushed below
//! the [`FileWidget<U>`][crate::widgets::FileWidget], this pushing
//! takes the form of a function, with signature
//! [`FnOnce(&Manager<U>, PushSpecs) -> Widget<U>`]. The
//! [`Manager<U>`][crate::Manager] parameter is then used to grant the
//! [`CommandLine<U>`][crate::widgets::CommandLine] access to the
//! [`Commands`] struct, allowing it to take user input to run.

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
/// let command =
///     "my-command --foo --bar -abcde --foo --baz -abfgh arg1";
/// let mut command_args = command.split_whitespace().skip(1);
/// let (flags, args) = split_flags(command_args);
///
/// let cmp = Flags {
///     blob: String::from("abcdefgh"),
///     units: vec!["foo", "bar", "baz"]
/// };
///
/// assert!(flags.blob == cmp.blob);
/// assert!(flags.units == cmp.units);
/// ```
///
/// If you have any arguments that start with `'-'` or `"--"`, but are
/// not supposed to be flags, you can insert an empty `"--"` after the
/// flags, in order to distinguish them.
///
/// ```rust
/// # use parsec_core::commands::{split_flags, Flags};
/// let command = "my-command --foo --bar -abcde -- --not-a-flag \
///                -also-not-flags";
/// let mut command_args = command.split_whitespace().skip(1);
/// let (flags, args) = split_flags(command_args);
///
/// let cmp = Flags {
///     blob: String::from("abcde"),
///     units: vec!["foo", "bar"]
/// };
///
/// assert!(flags.blob == cmp.blob);
/// assert!(flags.units == cmp.units);
/// ```
pub struct Flags<'a> {
    pub blob: String,
    pub units: Vec<&'a str>
}

impl<'a> Flags<'a> {
    /// Returns `true` if the [`char`] flag was passed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use parsec_core::commands::{split_flags, Flags};
    /// let command = "run -abcdefgh -ablk args -wz";
    /// let mut command_args = command.split_whitespace().skip(1);
    /// let (flags, args) = split_flags(command_args);
    ///
    /// assert!(flags.blob('k'));
    /// assert!(!flags.blob('w'));
    /// ```
    pub fn blob(&self, flag: char) -> bool {
        self.blob.contains(flag)
    }

    /// Returns `true` if the `unit` ([`impl AsRef<str>`][str] flag
    /// was passed.
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
}

/// A function to be used on hooks or in the [`CommandLine<U>`].
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
/// As an example, let's say you're creating a [`NormalWidget<U>`],
/// represented by the struct below.
///
/// ```rust
/// # use parsec_core::{text::Text, ui::Ui};
/// # use std::sync::{atomic::AtomicBool, Arc};
/// struct MyWidget<U>
/// where
///     U: Ui
/// {
///     text: Text<U>,
///     other_field: String,
///     relevant_field: Arc<AtomicBool>
/// }
/// ```
///
/// In this case, assuming that you will add the [`Command`]s through
/// a function that takes, as parameters, one `RwData<MyWidget<U>` and
/// one [`&mut Commands`][Commands], the function should look like
/// this:
///
/// ```rust
/// # use parsec_core::{
/// #     commands::{Command, CommandErr, Commands, Flags},
/// #     data::RwData,
/// #     text::Text,
/// #     ui::Ui
/// # };
/// # use std::sync::{
/// #     atomic::{AtomicBool, Ordering},
/// #     Arc
/// # };
/// #
/// # struct MyWidget<U>
/// # where
/// #     U: Ui
/// # {
/// #     text: Text<U>,
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
/// fn add_commands<U>(
///     widget: RwData<MyWidget<U>>, commands: &mut Commands
/// ) -> Result<(), CommandErr>
/// where
///     U: Ui
/// {
///     let relevant_field = widget.read().relevant_field.clone();
///     let callers = vec!["my-function-caller"];
///     let command = Command::new(callers, move |flags, args| {
///         command_function(
///             relevant_field.load(Ordering::Relaxed),
///             flags,
///             args
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
/// #     data::RwData,
/// #     text::Text,
/// #     ui::Ui
/// # };
/// # use std::sync::{
/// #     atomic::{AtomicBool, Ordering},
/// #     Arc
/// # };
/// #
/// # struct MyWidget<U>
/// # where
/// #     U: Ui
/// # {
/// #     text: Text<U>,
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
/// fn add_commands<U>(
///     widget: RwData<MyWidget<U>>, commands: &mut Commands
/// ) -> Result<(), CommandErr>
/// where
///     U: Ui
/// {
///     let callers = vec!["my-function-caller"];
///     let command = Command::new(callers, move |flags, args| {
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
/// [`RwData`] will get used very often, and by running the
/// [`Command`], you may cause a deadlock, which is really annoying to
/// diagnose.
///
/// As an example, there is the [`CommandLine<U>`] widget. If its
/// [`Command`]s moved an entire [`RwData<CommandLine<U>>`] to the
/// closures, every single one of them, when triggered through the
/// widget, would result in a deadlock, since they're writing to an
/// [`RwData<T>`] that was already being written to.
pub struct Command {
    /// A closure to trigger when any of the `callers` are called.
    ///
    /// # Arguments
    ///
    /// - 1: A [`&[String]`][String] representing the flags that have
    ///   been passed to the function.
    /// - 2: A [`&[String]`][String] representing the arguments to be
    ///   read by the function.
    ///
    /// # Returns
    ///
    /// A [`Result<Option<String>, String>`]. [`Ok(Some(String))`] is
    /// an outcome that could be used to chain multiple commands.
    /// The [`String`] in [`Err(String)`] is used to tell the user
    /// what went wrong while running the command, and possibly to
    /// show a message somewhere on Parsec.
    function: Box<dyn FnMut(&Flags, &mut dyn Iterator<Item = &str>) -> CommandResult>,
    /// A list of [`String`]s that act as callers for this `Command`.
    callers: Vec<String>
}

impl Command {
    /// Returns a new instance of [`Command`].
    ///
    /// The first parameter is the function that will be triggered
    /// through any of the keywords in the second parameter.
    pub fn new(
        callers: Vec<impl ToString>,
        function: impl FnMut(&Flags, &mut dyn Iterator<Item = &str>) -> CommandResult + 'static
    ) -> Self {
        let callers = callers.iter().map(|caller| caller.to_string()).collect::<Vec<String>>();
        if let Some(wrong_caller) =
            callers.iter().find(|caller| caller.split_whitespace().count() != 1)
        {
            panic!("Command caller \"{wrong_caller}\" is not a singular word.");
        }
        Self {
            function: Box::new(function),
            callers
        }
    }

    /// Executes the inner function if the `caller` matches any of the
    /// callers in [`self`].
    fn try_exec<'a>(
        &mut self, caller: &str, flags: &Flags, args: &mut impl Iterator<Item = &'a str>
    ) -> Result<Option<String>, CommandErr> {
        if self.callers.iter().any(|name| name == caller) {
            return (self.function)(flags, args).map_err(|err| CommandErr::Failed(err));
        } else {
            return Err(CommandErr::NotFound(String::from(caller)));
        }
    }

    /// The list of callers that will trigger this command.
    fn callers(&self) -> &[String] {
        &self.callers
    }
}

/// A list of [`Command`]s, meant to be used in a
/// [`CommandLine<U>`][crate::widgets::CommandLine].
#[derive(Default)]
pub struct Commands(Vec<Command>);

impl Commands {
    /// Returns a new instance of [`Commands`].
    pub fn new() -> Self {
        Commands(Vec::new())
    }

    /// Parses a [`String`] and tries to execute a [`Command`].
    ///
    /// The [`ToString`] will be parsed by separating the first word
    /// as the caller, while the rest of the words are treated as
    /// args.
    pub fn try_exec(&mut self, command: impl ToString) -> Result<Option<String>, CommandErr> {
        let command = command.to_string();
        let mut command = command.split_whitespace();

        let caller = command.next().ok_or(CommandErr::Empty)?;
        let (flags, mut args) = split_flags(command);

        for command in &mut self.0 {
            let result = command.try_exec(caller, &flags, &mut args);
            let Err(CommandErr::NotFound(_)) = result else {
                return result;
            };
        }

        Err(CommandErr::NotFound(String::from(caller)))
    }

    /// Tries to add a new [`Command`].
    ///
    /// Returns an [`Err`] if any of the callers of the [`Command`]
    /// are already callers for other commands.
    pub fn try_add(&mut self, command: Command) -> Result<(), CommandErr> {
        let mut new_callers = command.callers().iter();

        for caller in self.0.iter().map(|cmd| cmd.callers()).flatten() {
            if new_callers.any(|new_caller| new_caller == caller) {
                return Err(CommandErr::AlreadyExists(caller.clone()));
            }
        }

        self.0.push(command);

        Ok(())
    }
}

/// An error representing a failure in executing a [`Command`].
#[derive(Debug)]
pub enum CommandErr {
    AlreadyExists(String),
    NotFound(String),
    Failed(String),
    Empty
}

impl std::fmt::Display for CommandErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandErr::AlreadyExists(caller) => {
                f.write_fmt(format_args!("The caller \"{}\" already exists.", caller))
            }
            CommandErr::NotFound(caller) => {
                f.write_fmt(format_args!("The caller \"{}\" was not found.", caller))
            }
            CommandErr::Failed(failure) => f.write_str(failure),
            CommandErr::Empty => f.write_str("No caller supplied.")
        }
    }
}

impl std::error::Error for CommandErr {}

pub fn split_flags<'a>(
    mut args: impl Iterator<Item = &'a str>
) -> (Flags<'a>, impl Iterator<Item = &'a str>) {
    let mut blob = String::new();
    let mut units = Vec::new();

    while let Some(arg) = args.next() {
        if arg.starts_with("--") {
            if arg.len() > 2 {
                if !units.contains(&&arg[2..]) {
                    units.push(&arg[2..])
                }
            } else {
                break;
            }
        } else if arg.starts_with("-") {
            for char in arg[1..].chars() {
                if !blob.contains(char) {
                    blob.push(char)
                }
            }
        } else {
            break;
        }
    }

    (Flags { blob, units }, args)
}

type CommandResult = Result<Option<String>, String>;