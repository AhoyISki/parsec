#![allow(incomplete_features, clippy::type_complexity)]
#![feature(
    lazy_cell,
    extract_if,
    result_option_inspect,
    trait_upcasting,
    let_chains,
    option_zip,
    control_flow_enum,
    return_position_impl_trait_in_trait,
    decl_macro,
    generic_const_exprs,
    step_trait,
    type_alias_impl_trait
)]

use std::sync::atomic::{AtomicBool, Ordering};

use commands::Commands;
use data::{CurrentFile, CurrentWidget};
use forms::FormPalette;

pub mod commands;
pub mod data;
pub mod forms;
pub mod history;
pub mod input;
pub mod position;
pub mod session;
pub mod text;
pub mod ui;
pub mod widgets;

// Debugging objects.
pub static DEBUG_TIME_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

// Internal control objects.
static COMMANDS: Commands = Commands::new();
static BREAK_LOOP: AtomicBool = AtomicBool::new(false);
static SHOULD_QUIT: AtomicBool = AtomicBool::new(false);

// Public control objects.
pub static PALETTE: FormPalette = FormPalette::new();
pub static CURRENT_FILE: CurrentFile = CurrentFile::new();
pub static CURRENT_WIDGET: CurrentWidget = CurrentWidget::new();

/// Quits Parsec.
pub fn quit() {
    BREAK_LOOP.store(true, Ordering::Release);
    SHOULD_QUIT.store(true, Ordering::Release);
}

/// Internal macro used to log information.
pub macro log_info {
    ($($text:tt)*) => {
        use std::{fs, io::Write, time::Instant};
        let mut log = fs::OpenOptions::new().append(true).open("log").unwrap();
        let mut text = format!($($text)*);
        if text.lines().count() > 1 {
            let chars = text.char_indices().filter_map(|(pos, char)| (char == '\n').then_some(pos));
            let nl_indices: Vec<usize> = chars.collect();
            for index in nl_indices.iter().rev() {
                text.insert_str(index + 1, "  ");
            }

            let duration = Instant::now().duration_since(*$crate::DEBUG_TIME_START.get().unwrap());
            write!(log, "\nat {:.4?}:\n  {text}", duration).unwrap();
        } else {
            let duration = Instant::now().duration_since(*$crate::DEBUG_TIME_START.get().unwrap());
            write!(log, "\nat {:.4?}: {text}", duration).unwrap();
        }
    }
}
