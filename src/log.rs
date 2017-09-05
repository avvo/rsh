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
            writeln!(&mut self.device, "{}", message)
        } else {
            Ok(())
        }
    }

    fn set_level(&mut self, level: LogLevel) {
        self.level = level;
    }

    // fn increase_level(&mut self) {
    //     self.level = self.level.succ();
    // }
    //
    // fn decrease_level(&mut self) {
    //     self.level = self.level.pred();
    // }

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

// pub fn increase_level() {
//     let mut logger = LOGGER.lock().unwrap();
//     logger.increase_level();
// }
//
// pub fn decrease_level() {
//     let mut logger = LOGGER.lock().unwrap();
//     logger.decrease_level();
// }

pub fn set_device<T: Write + Send + 'static>(device: T) {
    let mut logger = LOGGER.lock().unwrap();
    logger.set_device(device);
}

macro_rules! log(
    ($level:expr, $($arg:tt)*) => { {
        let message = format!($($arg)*);
        ::log::log($level, message).unwrap();
    } }
);

macro_rules! fatal(
    ($($arg:tt)*) => { {
        log!(::log::LogLevel::Fatal, $($arg)*);
    } }
);

// macro_rules! error(
//     ($($arg:tt)*) => { {
//         log!(::log::LogLevel::Error, $($arg)*);
//     } }
// );

macro_rules! info(
    ($($arg:tt)*) => { {
        log!(::log::LogLevel::Info, $($arg)*);
    } }
);

// macro_rules! verbose(
//     ($($arg:tt)*) => { {
//         log!(::log::LogLevel::Vebose, $($arg)*);
//     } }
// );

// macro_rules! debug(
//     ($($arg:tt)*) => { {
//         log!(::log::LogLevel::Debug, $($arg)*);
//     } }
// );

// macro_rules! debug2(
//     ($($arg:tt)*) => { {
//         log!(::log::LogLevel::Debug2, $($arg)*);
//     } }
// );

// macro_rules! debug3(
//     ($($arg:tt)*) => { {
//         log!(::log::LogLevel::Debug3, $($arg)*);
//     } }
// );
