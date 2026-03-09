#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let debug = std::env::args().any(|a| a == "--debug");

    let autostart = std::env::args().find_map(|a| {
        if a == "--autostart=true" {
            Some(true)
        } else if a == "--autostart=false" {
            Some(false)
        } else {
            None
        }
    });

    if debug {
        let log_path = debug_log_path();
        setup_debug_logging(&log_path);
        log::info!("Ms. T starting in debug mode");
        log::info!("Log file: {}", log_path.display());
    }

    mst::run(autostart)
}

fn debug_log_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("."))
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("mst-debug.log")
}

fn setup_debug_logging(log_path: &PathBuf) {
    // Install a panic hook that writes to the log file before aborting
    let panic_path = log_path.clone();
    std::panic::set_hook(Box::new(move |info| {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&panic_path)
            .ok();
        if let Some(f) = f.as_mut() {
            let _ = writeln!(f, "PANIC: {info}");
            if let Some(loc) = info.location() {
                let _ = writeln!(f, "  at {}:{}:{}", loc.file(), loc.line(), loc.column());
            }
        }
    }));

    // Configure env_logger to write to the log file
    let target_path = log_path.clone();
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&target_path)
        .expect("failed to open debug log file");

    std::env::set_var("RUST_LOG", "debug");
    env_logger::Builder::from_default_env()
        .target(env_logger::Target::Pipe(Box::new(file)))
        .init();
}
