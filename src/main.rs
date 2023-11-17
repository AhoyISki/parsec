#![feature(decl_macro, lazy_cell, generic_const_exprs)]
#![allow(incomplete_features)]
use std::{
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
};

use duat::{prelude::*, run_duat};
use duat_core::{data::RwData, ui, widgets::File};
use libloading::os::unix::{Library, Symbol};
use notify::{Event, EventKind, RecursiveMode, Watcher};

static FILES_CHANGED: AtomicBool = AtomicBool::new(false);
static BREAK: AtomicU32 = AtomicU32::new(0);

fn main() {
    // Assert that the configuration crate actually exists.
    // The watcher is returned as to not be dropped.
    if let Some((_watcher, toml_path, so_path)) = dirs_next::config_dir().and_then(|config_dir| {
        let crate_dir = config_dir.join("duat");

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

        let mut cur_lib = unsafe { Some(Library::new(&so_path).unwrap()) };
        let mut run = cur_lib.as_ref().and_then(find_run_fn);
        let mut prev_files = Vec::new();

        loop {
            let (tx, rx) = mpsc::channel();

            let handle = if let Some(run) = run.take() {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let ret = run(prev_files, tx, rx);
                    atomic_wait::wake_all(&BREAK);
                    ret
                })
            } else {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let ret = run_duat(prev_files, tx, rx);
                    atomic_wait::wake_all(&BREAK);
                    ret
                })
            };

            loop {
                atomic_wait::wait(&BREAK, 0);

                if !FILES_CHANGED.load(Ordering::Relaxed) {
                    break;
                }
                FILES_CHANGED.store(false, Ordering::Relaxed);

                if run_cargo(&toml_path).is_ok() {
                    let cur_lib = unsafe { Library::new(&so_path).ok() };
                    if cur_lib.as_ref().and_then(find_run_fn).is_some() {
                        let _ = tx.send(ui::Event::ReloadConfig);
                        break;
                    }
                }
            }

            prev_files = handle.join().unwrap();

            if prev_files.is_empty() {
                break;
            } else {
                if let Some(cur_lib) = cur_lib.take() {
                    cur_lib.close().unwrap()
                }
                cur_lib = unsafe { Library::new(&so_path).ok() };
                run = cur_lib.as_ref().and_then(find_run_fn);
            }
        }
    } else {
        let (tx, rx) = mpsc::channel();
        run_duat(Vec::new(), tx, rx);
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

type PrevFiles = Vec<(RwData<File>, bool)>;
type RunFn =
    fn(prev: PrevFiles, tx: mpsc::Sender<ui::Event>, rx: mpsc::Receiver<ui::Event>) -> PrevFiles;
