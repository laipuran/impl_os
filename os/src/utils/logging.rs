use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};

struct Logger;

static LOGGER: Logger = Logger;

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            crate::println!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

pub fn init() -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER)?;
    log::set_max_level(LevelFilter::Info);
    Ok(())
}
