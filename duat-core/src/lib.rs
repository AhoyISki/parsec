#![feature(
    extract_if,
    iter_intersperse,
    iter_order_by,
    trait_upcasting,
    let_chains,
    control_flow_enum,
    decl_macro,
    step_trait,
    type_alias_impl_trait,
    result_flattening,
    is_none_or
)]
#![doc = include_str!("../README.md")]

use std::{
    any::type_name,
    marker::PhantomData,
    sync::{LazyLock, Mutex, Once},
};

use text::{err, hint, Text};

pub mod commands;
pub mod data;
pub mod history;
pub mod hooks;
pub mod input;
pub mod palette;
pub mod position;
pub mod session;
pub mod text;
pub mod ui;
pub mod widgets;

// Debugging objects.
pub static DEBUG_TIME_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
pub static HOOK: Once = Once::new();
pub static LOG: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

pub trait DuatError {
    fn into_text(self) -> Text;
}

/// Error for failures in Duat
pub enum Error<E> {
    /// # Command related errors:

    /// An alias wasn't just a single word
    AliasNotSingleWord(String),
    /// The caller for a command already pertains to another
    CallerAlreadyExists(String),
    /// No commands have the given caller as one of their own
    CallerNotFound(String),
    /// The command failed internally
    CommandFailed(Text),
    /// There was no caller and no arguments
    Empty,

    /// # Context related errors:

    /// The [`Ui`] still hasn't created the first file
    NoFileYet,
    /// Since the [`Ui`] has no file, widgets can't relate to it
    NoFileForRelated,
    /// The [`Ui`] still hasn't created the first widget (a file)
    NoWidgetYet,
    /// The checked widget is not of the type given
    WidgetIsNot,
    /// The checked input is not of the type given
    InputIsNot(PhantomData<E>),
}

impl<E> DuatError for Error<E> {
    /// Turns the [`Error`] into formatted [`Text`]
    fn into_text(self) -> Text {
        let early = hint!(
            "Try this after " [*a] "OnUiStart" []
            ", maybe by using hooks::add::<OnUiStart>"
        );

        match self {
            Error::AliasNotSingleWord(caller) => err!(
                "The caller " [*a] caller [] "is not a single word."
            ),
            Error::CallerAlreadyExists(caller) => err!(
                "The caller " [*a] caller [] "already exists."
            ),
            Error::CallerNotFound(caller) => err!("The caller " [*a] caller [] "was not found."),
            Error::CommandFailed(failure) => failure,
            Error::Empty => err!("The command is empty."),
            Error::NoFileYet => err!("There is no file yet. " early),
            Error::NoFileForRelated => err!(
                "There is no file for a related " [*a] { type_name::<E>() } [] "to exist. " early
            ),
            Error::NoWidgetYet => err!("There can be no widget yet. " early),
            Error::WidgetIsNot => err!(
                "The widget is not " [*a] { type_name::<E>() } [] ". " early
            ),
            Error::InputIsNot(..) => err!(
                "This file's input is not " [*a] { type_name::<E>() } [] ". " early
            ),
        }
    }
}

impl<E> std::fmt::Debug for Error<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_tuple(match self {
            Error::AliasNotSingleWord(_) => "Error::AliasNotSingleWord",
            Error::CallerAlreadyExists(_) => "Error::CallerAlreadyExists",
            Error::CallerNotFound(_) => "Error::CallerNotFound",
            Error::CommandFailed(_) => "Error::CommandFailed",
            Error::Empty => "Error::Empty ",
            Error::NoFileYet => "Error::NoFileYet ",
            Error::NoFileForRelated => "Error::NoFileForRelated ",
            Error::NoWidgetYet => "Error::NoWidgetYet ",
            Error::WidgetIsNot => "Error::WidgetIsNot ",
            Error::InputIsNot(_) => "Error::InputIsNot",
        });

        match self {
            Error::AliasNotSingleWord(str)
            | Error::CallerAlreadyExists(str)
            | Error::CallerNotFound(str) => debug.field(&str),
            Error::CommandFailed(text) => debug.field(&text),
            Error::Empty
            | Error::NoFileYet
            | Error::NoFileForRelated
            | Error::NoWidgetYet
            | Error::WidgetIsNot
            | Error::InputIsNot(_) => &mut debug,
        }
        .finish()
    }
}

pub type Result<T, E> = std::result::Result<T, Error<E>>;

/// Internal macro used to log information.
pub macro log_info($($text:tt)*) {{
    #[cfg(not(debug_assertions))] {
    	compile_error!("You are not supposed to use log_info on release profiles!");
    }

    use std::{fmt::Write, time::Instant};

    use crate::{HOOK, LOG};

    let mut text = format!($($text)*);

    HOOK.call_once(|| {
        let old_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            old_hook(info);
            println!("Logs:");
            println!("{}\n", LOG.lock().unwrap());
        }));
    });

    if let Some(start) = $crate::DEBUG_TIME_START.get() {
        if text.lines().count() > 1 {
            let chars = text.char_indices().filter_map(|(pos, char)| (char == '\n').then_some(pos));
            let nl_indices: Vec<usize> = chars.collect();
            for index in nl_indices.iter().rev() {
                text.insert_str(index + 1, "  ");
            }

            let duration = Instant::now().duration_since(*start);
            write!(LOG.lock().unwrap(), "\nat {:.4?}:\n  {text}", duration).unwrap();
        } else {
            let duration = Instant::now().duration_since(*start);
            write!(LOG.lock().unwrap(), "\nat {:.4?}: {text}", duration).unwrap();
        }
    } else {
        write!(LOG.lock().unwrap(), "\n{text}").unwrap();
    }

	let len_lines = LOG.lock().unwrap().lines().count().saturating_sub(200);
    let trimmed = LOG.lock().unwrap().split_inclusive('\n').skip(len_lines).collect();
    *LOG.lock().unwrap() = trimmed;
}}
