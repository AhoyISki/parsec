#![feature(decl_macro, lazy_cell, generic_const_exprs)]
#![allow(incomplete_features, dead_code)]
use std::{
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
};

use libloading::os::unix::{Library, Symbol};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use parsec_core::{data::RwData, widgets::File};
pub use utils::run_parsec;
mod remapper;
mod utils;
mod widgets;

static FILES_CHANGED: AtomicBool = AtomicBool::new(false);
static BREAK: AtomicU32 = AtomicU32::new(NO);

fn main() {
    // Assert that the configuration crate actually exists.
    // The watcher is returned as to not be dropped.
    if let Some((_watcher, toml_path, so_path)) = dirs_next::config_dir().and_then(|config_dir| {
        let crate_dir = config_dir.join("parsec");

        let so_path = crate_dir.join("target/release/libconfig.so");
        let src_path = crate_dir.join("src");
        let toml_path = crate_dir.join("Cargo.toml");

        let mut watcher = notify::recommended_watcher(|res| match res {
            Ok(Event {
                kind: EventKind::Modify(_),
                ..
            }) => {
                FILES_CHANGED.store(true, Ordering::Relaxed);
                atomic_wait::wake_one(&BREAK);
            }
            Ok(_) | Err(_) => {}
        })
        .unwrap();

        watcher.watch(&src_path, RecursiveMode::Recursive).ok()?;
        watcher
            .watch(&toml_path, RecursiveMode::NonRecursive)
            .ok()?;

        Some((watcher, toml_path, so_path))
    }) {
        let _ = run_cargo(&toml_path);

        let mut cur_lib = unsafe { Library::new(&so_path).ok() };
        #[allow(unused_assignments)]
        let mut run = cur_lib.as_ref().and_then(find_run_fn);
        let mut prev = Vec::new();

        loop {
            let (tx, rx) = mpsc::channel();

            let handle = if let Some(run) = run.take() {
                std::thread::spawn(move || {
                    let ret = run(prev, rx);
                    atomic_wait::wake_all(&BREAK);
                    ret
                })
            } else {
                std::thread::spawn(move || {
                    let ret = run_parsec(prev, rx);
                    atomic_wait::wake_all(&BREAK);
                    ret
                })
            };

            loop {
                atomic_wait::wait(&BREAK, NO);

                if !FILES_CHANGED.load(Ordering::Relaxed) {
                    break;
                }
                FILES_CHANGED.store(false, Ordering::Relaxed);

                if run_cargo(&toml_path).is_ok() {
                    let cur_lib = unsafe { Library::new(&so_path).ok() };
                    #[allow(unused_assignments)]
                    if cur_lib.as_ref().and_then(find_run_fn).is_some() {
                        let _ = tx.send(());
                        break;
                    }
                }
            }

            prev = handle.join().unwrap();

            if prev.is_empty() {
                break;
            } else {
                cur_lib.take().unwrap().close().unwrap();
                cur_lib = unsafe { Library::new(&so_path).ok() };
                run = cur_lib.as_ref().and_then(find_run_fn);
            }
        }
    } else {
        let (_tx, rx) = mpsc::channel();
        run_parsec(Vec::new(), rx);
    }
}

fn run_cargo(toml_path: &Path) -> Result<std::process::Output, std::io::Error> {
    let mut cargo = Command::new("cargo");
    cargo.args([
        "build",
        "--release",
        "--quiet",
        "--manifest-path",
        toml_path.to_str().unwrap(),
    ]);

    cargo.output()
}

fn find_run_fn(lib: &Library) -> Option<Symbol<RunFn>> {
    unsafe { lib.get::<RunFn>(b"run").ok() }
}

// The main macro to run parsec.
pub macro run($($tree:tt)*) {
    #[no_mangle]
    fn run(prev: PrevFiles, rx: mpsc::Receiver<()>) -> PrevFiles {
        {
            $($tree)*
        };

        run_parsec(prev, rx)
    }
}

// This will eventually be a NOT AND to check if any Uis have been
// chosen at all.
// Later, I'll also have an XOR checker to make sure only one Ui was
// chosen.
#[cfg(not(feature = "term-ui"))]
compile_error! {
    "No ui has been chosen to compile Parsec with."
}

#[cfg(feature = "term-ui")]
pub type Ui = parsec_term::Ui;

pub mod prelude {
    pub use parsec_core::{
        self,
        file::File,
        palette::Form,
        position,
        text::{text, Builder, Text},
    };

    pub use crate::{
        run,
        utils::{control, hook, print, setup},
        widgets::{common::*, status, CommandLine, LineNumbers, StatusLine},
    };
}

const NO: u32 = 0;
type PrevFiles = Vec<(RwData<File<Ui>>, bool)>;
type RunFn = fn(PrevFiles, rx: mpsc::Receiver<()>) -> PrevFiles;
