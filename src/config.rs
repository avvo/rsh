extern crate nom;

use std::str::FromStr;

pub use options::{LogLevel, Protocol, RequestTTY};
use pattern;

#[derive(Debug)]
pub enum Error {
    OptionError(String, String),
    ParseError(nom::ErrorKind),
    UnexpectedEnd,
    UnknownOption(String),
}

macro_rules! from {
    ( $a:ty => $b:ty, $c:expr ) => {
        impl From<$a> for $b {
            fn from(err: $a) -> $b {
                $c(err)
            }
        }
    };
}

from!(nom::ErrorKind => Error, Error::ParseError);

#[derive(Debug)]
pub struct Config {
    sections: Vec<Section>,
}

macro_rules! search {
    ( $option:ident -> $type:ty ) => {
        pub fn $option(&self, host: &str) -> Option<$type> {
            for section in self.sections.iter() {
                if section.pattern.matches(host) {
                    if let Some(ref value) = section.$option {
                        return Some(value.to_owned());
                    }
                }
            }
            None
        }
    }
}

impl Config {
    search!(environment -> String);
    search!(escape_char -> char);
    search!(host_name -> String);
    search!(log_level -> LogLevel);
    search!(port -> u16);
    search!(protocol -> Protocol);
    search!(remote_command -> String);
    search!(request_tty -> RequestTTY);
    search!(service -> String);
    search!(stack -> String);
    search!(user -> String);
}

impl Default for Config {
    fn default() -> Config {
        Config { sections: Vec::new() }
    }
}

impl FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match config_file(s) {
            nom::IResult::Done(_, pairs) => build_config(pairs),
            nom::IResult::Error(e) => Err(Error::from(e)),
            nom::IResult::Incomplete(_) => Err(Error::UnexpectedEnd),
        }
    }
}

macro_rules! assign {
    ( $name:expr, $lhs:expr => $rhs:expr ) => {
        {
            match $rhs.parse() {
                Ok(v) => $lhs = Some(v),
                Err(_) => {
                    return Err(Error::OptionError($name.into(), $rhs.into()));
                }
            };
        }
    }
}

fn build_config(pairs: Vec<(&str, &str)>) -> Result<Config, Error> {
    let mut sections = Vec::new();
    let mut current = Section::default();
    for (key, value) in pairs {
        match key.to_lowercase().as_ref() {
            "host" => {
                sections.push(current);
                match value.parse() {
                    Ok(v) => current = Section::new(v),
                    Err(_) => return Err(Error::OptionError(key.into(), value.into())),
                };
            }
            "environment" => assign!(key, current.environment => value),
            "escapechar" => assign!(key, current.escape_char => value),
            "hostname" => assign!(key, current.host_name => value),
            "loglevel" => assign!(key, current.log_level => value),
            "port" => assign!(key, current.port => value),
            "protocol" => assign!(key, current.protocol => value),
            "remotecommand" => assign!(key, current.remote_command => value),
            "requesttty" => assign!(key, current.request_tty => value),
            "service" => assign!(key, current.service => value),
            "stack" => assign!(key, current.stack => value),
            "user" => assign!(key, current.user => value),
            _ => return Err(Error::UnknownOption(key.into())),
        }
    }
    sections.push(current);
    Ok(Config { sections })
}

#[derive(Debug, Default)]
struct Section {
    pattern: pattern::PatternList,
    environment: Option<String>,
    escape_char: Option<char>,
    host_name: Option<String>,
    log_level: Option<LogLevel>,
    port: Option<u16>,
    protocol: Option<Protocol>,
    remote_command: Option<String>,
    request_tty: Option<RequestTTY>,
    service: Option<String>,
    stack: Option<String>,
    user: Option<String>,
}

impl Section {
    fn new(pattern: pattern::PatternList) -> Section {
        let mut section = Section::default();
        section.pattern = pattern;
        section
    }
}

named!(config_file(&str) -> Vec<(&str, &str)>, do_parse!(
    res: call!(pairs) >>
    eof!() >>
    (res)
));

named!(pairs(&str) -> Vec<(&str, &str)>, many0!(do_parse!(
    blank >>
    key: call!(nom::alphanumeric) >>
    alt!(tag_s!(" ") | tag_s!("=")) >>
    val: opt!(value) >>
    blank >>
    (key, val.unwrap_or(""))
)));

named!(blank(&str) -> Vec<&str>, many0!(alt!(comment | call!(nom::multispace))));

named!(comment(&str) -> &str, do_parse!(
    tag_s!("#") >>
    comment: call!(nom::not_line_ending) >>
    (comment)
));

named!(value(&str) -> &str, alt!(
    delimited!(tag_s!("\""), take_until_s!("\""), tag_s!("\"")) |
    call!(nom::not_line_ending)
));
