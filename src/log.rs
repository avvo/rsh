use std;

pub use options::LogLevel;

lazy_static! {
    static ref LEVEL: std::sync::Mutex<LogLevel> = std::sync::Mutex::new(LogLevel::default());
}

pub fn level() -> LogLevel {
    *LEVEL.lock().unwrap()
}

pub fn set_level(new_level: LogLevel) {
    let mut level = LEVEL.lock().unwrap();
    *level = new_level;
}

pub fn increase_level() {
    let mut level = LEVEL.lock().unwrap();
    *level = level.succ();
}

pub fn decrease_level() {
    let mut level = LEVEL.lock().unwrap();
    *level = level.pred();
}

macro_rules! fatal(
    ($($arg:tt)*) => { {
        if log::LogLevel::Fatal >= log::level() {
            writeln!(&mut ::std::io::stderr(), $($arg)*).unwrap();
        }
    } }
);

macro_rules! error(
    ($($arg:tt)*) => { {
        if log::LogLevel::Error >= log::level() {
            writeln!(&mut ::std::io::stderr(), $($arg)*).unwrap();
        }
    } }
);

macro_rules! info(
    ($($arg:tt)*) => { {
        if log::LogLevel::Info >= log::level() {
            writeln!(&mut ::std::io::stderr(), $($arg)*).unwrap();
        } 
    } }
);

macro_rules! verbose(
    ($($arg:tt)*) => { {
        if log::LogLevel::Verbose >= log::level() {
            writeln!(&mut ::std::io::stderr(), $($arg)*).unwrap();
        }
    } }
);

macro_rules! debug(
    ($($arg:tt)*) => { {
        if log::LogLevel::Debug >= log::level() {
            let message = format!($($arg)*);
            writeln!(&mut ::std::io::stderr(), "debug1: {}", message).unwrap();
        }
    } }
);

macro_rules! debug2(
    ($($arg:tt)*) => { {
        if log::LogLevel::Debug2 >= log::level() {
            let message = format!($($arg)*);
            writeln!(&mut ::std::io::stderr(), "debug2: {}", message).unwrap();
        }
    } }
);

macro_rules! debug3(
    ($($arg:tt)*) => { {
        if log::LogLevel::Debug3 >= log::level() {
            let message = format!($($arg)*);
            writeln!(&mut ::std::io::stderr(), "debug3: {}", message).unwrap();
        }
    } }
);
