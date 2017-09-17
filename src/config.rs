extern crate nom;

use std;
use std::str::FromStr;

use options;
pub use options::{LogLevel, Protocol, RequestTTY};
use pattern;

#[derive(Debug)]
pub enum Error {
    CharError(std::char::ParseCharError),
    IntError(std::num::ParseIntError),
    OptionError(options::ParseError),
    ParseError(nom::ErrorKind),
    PatternError(pattern::Error),
    StringError(std::string::ParseError),
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

from!(std::char::ParseCharError => Error, Error::CharError);
from!(std::num::ParseIntError => Error, Error::IntError);
from!(options::ParseError => Error, Error::OptionError);
from!(nom::ErrorKind => Error, Error::ParseError);
from!(pattern::Error => Error, Error::PatternError);
from!(std::string::ParseError => Error, Error::StringError);

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
        match pairs(s) {
            nom::IResult::Done(_, pairs) => build_config(pairs),
            nom::IResult::Error(e) => Err(Error::from(e)),
            nom::IResult::Incomplete(_) => Err(Error::UnexpectedEnd),
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
                current = Section::new(value.parse()?);
            }
            "environment" => current.environment = Some(value.parse()?),
            "escapechar" => current.escape_char = Some(value.parse()?),
            "hostname" => current.host_name = Some(value.parse()?),
            "loglevel" => current.log_level = Some(value.parse()?),
            "port" => current.port = Some(value.parse()?),
            "protocol" => current.protocol = Some(value.parse()?),
            "remotecommand" => current.remote_command = Some(value.parse()?),
            "requesttty" => current.request_tty = Some(value.parse()?),
            "service" => current.service = Some(value.parse()?),
            "stack" => current.stack = Some(value.parse()?),
            "user" => current.user = Some(value.parse()?),
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
