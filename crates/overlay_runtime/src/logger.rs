use log::{LevelFilter, Record};
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger, SharedLogger};
use std::fs::File;

use custom_windows::toasts::TOAST_QUEUE;

/// Available values: Off, Error, Warn, Info, Debug, Trace
const LOG_LEVEL: LevelFilter = LevelFilter::Debug;

/// Path to the log file.
const LOG_FILE_PATH: &str = "d3d9_proxy_mod.log";

/// A custom logger implementation that forwards log records to the UI toast queue.
struct ToastLogger;

impl log::Log for ToastLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= LevelFilter::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            // This is a non-blocking attempt to write. If the UI thread is holding the lock,
            // we drop the message to prevent the game from stuttering.
            if let Ok(mut queue) = TOAST_QUEUE.try_lock() {
                // Prevent the queue from growing indefinitely.
                if queue.len() >= 32 {
                    queue.pop_front();
                }
                queue.push_back((record.level(), record.args().to_string()));
            }
        }
    }

    fn flush(&self) {}
}

impl SharedLogger for ToastLogger {
    /// Returns the set Level for this Logger
    fn level(&self) -> LevelFilter {
        LOG_LEVEL
    }

    /// Inspect the config of a running Logger
    fn config(&self) -> Option<&Config> {
        None
    }

    /// Returns the logger as a Log trait object
    fn as_log(self: Box<Self>) -> Box<dyn log::Log> {
        Box::new(*self)
    }
}

/// Initializes the logging system.
pub fn init() {
    let log_file = File::create(LOG_FILE_PATH);

    // simplelog now requires Vec<Box<dyn SharedLogger>>
    let mut loggers: Vec<Box<dyn SharedLogger>> = Vec::new();
    loggers.push(Box::new(ToastLogger));

    match log_file {
        Ok(file) => {
            loggers.push(TermLogger::new(LOG_LEVEL, Config::default(), TerminalMode::Mixed, ColorChoice::Auto));
            loggers.push(WriteLogger::new(LOG_LEVEL, Config::default(), file));
        }
        Err(_) => {
            loggers.push(TermLogger::new(LOG_LEVEL, Config::default(), TerminalMode::Mixed, ColorChoice::Auto));
        }
    };

    // The init function takes an iterator of SharedLogger boxes.
    let result = CombinedLogger::init(loggers);

    if result.is_err() {
        log::error!("Failed to initialize logger!");
        eprintln!("[MOD] Failed to initialize logger!");
    }
}
