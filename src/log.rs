use std;
use std::sync::{Arc, Mutex};
use std::boxed::Box;
use std::marker::Send;
use std::io::Write;

pub use options::LogLevel;

lazy_static! {
    static ref LOGGER: Arc<Mutex<Logger>> = Arc::new(Mutex::new(Logger::default()));
}

struct Logger {
    level: LogLevel,
    device: Box<Write + Send>,
}

impl Logger {
    fn log(&mut self, level: LogLevel, message: String) -> std::io::Result<()> {
        if level <= self.level {
            match level {
                LogLevel::Debug => writeln!(&mut self.device, "debug1: {}", message),
                LogLevel::Debug2 => writeln!(&mut self.device, "debug2: {}", message),
                LogLevel::Debug3 => writeln!(&mut self.device, "debug3: {}", message),
                _ => writeln!(&mut self.device, "{}", message),
            }
        } else {
            Ok(())
        }
    }

    fn set_level(&mut self, level: LogLevel) {
        self.level = level;
    }

    fn increase_level(&mut self) -> LogLevel {
        let level = self.level.succ();
        self.level = level;
        level
    }

    fn decrease_level(&mut self) -> LogLevel {
        let level = self.level.pred();
        self.level = level;
        level
    }

    fn set_device<T: Write + Send + 'static>(&mut self, device: T) {
        self.device = Box::new(device);
    }
}

impl Default for Logger {
    fn default() -> Logger {
        Logger {
            level: LogLevel::default(),
            device: Box::new(std::io::stderr()),
        }
    }
}

pub fn log(level: LogLevel, message: String) -> std::io::Result<()> {
    let mut logger = LOGGER.lock().unwrap();
    logger.log(level, message)
}

pub fn level() -> LogLevel {
    let logger = LOGGER.lock().unwrap();
    logger.level
}

pub fn set_level(level: LogLevel) {
    let mut logger = LOGGER.lock().unwrap();
    logger.set_level(level);
}

pub fn increase_level() -> LogLevel {
    let mut logger = LOGGER.lock().unwrap();
    logger.increase_level()
}

pub fn decrease_level() -> LogLevel {
    let mut logger = LOGGER.lock().unwrap();
    logger.decrease_level()
}

pub fn set_device<T: Write + Send + 'static>(device: T) {
    let mut logger = LOGGER.lock().unwrap();
    logger.set_device(device);
}

#[macro_export]
macro_rules! log(
    ($level:expr, $($arg:tt)*) => { {
        if $level <= ::log::level() {
            let message = format!($($arg)*);
            ::log::log($level, message).unwrap();
        }
    } }
);

#[macro_export]
macro_rules! fatal(
    ($($arg:tt)*) => { {
        log!(::log::LogLevel::Fatal, $($arg)*);
    } }
);

#[macro_export]
macro_rules! error(
    ($($arg:tt)*) => { {
        log!(::log::LogLevel::Error, $($arg)*);
    } }
);

#[macro_export]
macro_rules! info(
    ($($arg:tt)*) => { {
        log!(::log::LogLevel::Info, $($arg)*);
    } }
);

#[macro_export]
macro_rules! verbose(
    ($($arg:tt)*) => { {
        log!(::log::LogLevel::Verbose, $($arg)*);
    } }
);

#[macro_export]
macro_rules! debug(
    ($($arg:tt)*) => { {
        log!(::log::LogLevel::Debug, $($arg)*);
    } }
);

#[macro_export]
macro_rules! debug2(
    ($($arg:tt)*) => { {
        log!(::log::LogLevel::Debug2, $($arg)*);
    } }
);

#[macro_export]
macro_rules! debug3(
    ($($arg:tt)*) => { {
        log!(::log::LogLevel::Debug3, $($arg)*);
    } }
);
