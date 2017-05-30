use std::env;
use log::LogRecord;
use env_logger::{LogBuilder, LogTarget};

/// If the `RUST_LOG` environment variable is set, enable logging.
pub fn init_logging() {
    if let Ok(rust_log) = env::var("RUST_LOG") {
        // Disable extraneous formatting.
        let format = |record: &LogRecord| format!("{}", record.args());

        let mut builder = LogBuilder::new();
        builder
            .format(format)
            .target(LogTarget::Stdout)
            .parse(&rust_log);

        if let Err(_) = builder.init() {
            // already initialised
        }
    }
}
