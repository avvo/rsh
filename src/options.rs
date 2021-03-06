extern crate url;
extern crate users;

use std;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::str::FromStr;

use pattern;

#[derive(Debug)]
pub enum BuildError {
    MissingEnvironment,
    MissingHostName,
    MissingService,
    MissingStack,
    UnknownToken(char),
}

impl std::error::Error for BuildError {
    fn description(&self) -> &str {
        match *self {
            BuildError::MissingEnvironment => "no environment provided",
            BuildError::MissingHostName => "no hostname provided",
            BuildError::MissingService => "no service provided",
            BuildError::MissingStack => "no stack provided",
            BuildError::UnknownToken(_) => "unknown token",
        }
    }

    fn cause(&self) -> Option<&std::error::Error> {
        None
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        self.description().fmt(fmt)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Container {
    First,
    Auto,
    Menu,
}

impl Default for Container {
    fn default() -> Container {
        Container::First
    }
}

impl fmt::Display for Container {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        match self {
            &Container::First => "first".fmt(fmt),
            &Container::Auto => "auto".fmt(fmt),
            &Container::Menu => "menu".fmt(fmt),
        }
    }
}

impl FromStr for Container {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "first" => Ok(Container::First),
            "auto" => Ok(Container::Auto),
            "menu" => Ok(Container::Menu),
            _ => Err(ParseError),
        }
    }
}

#[derive(Debug)]
pub struct ParseError;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum LogLevel {
    Quiet,
    Fatal,
    Error,
    Info,
    Verbose,
    Debug,
    Debug2,
    Debug3,
}

impl LogLevel {
    pub fn succ(self) -> LogLevel {
        match self {
            LogLevel::Quiet => LogLevel::Fatal,
            LogLevel::Fatal => LogLevel::Error,
            LogLevel::Error => LogLevel::Info,
            LogLevel::Info => LogLevel::Verbose,
            LogLevel::Verbose => LogLevel::Debug,
            LogLevel::Debug => LogLevel::Debug2,
            LogLevel::Debug2 => LogLevel::Debug3,
            LogLevel::Debug3 => LogLevel::Debug3,
        }
    }

    pub fn pred(self) -> LogLevel {
        match self {
            LogLevel::Quiet => LogLevel::Quiet,
            LogLevel::Fatal => LogLevel::Quiet,
            LogLevel::Error => LogLevel::Fatal,
            LogLevel::Info => LogLevel::Error,
            LogLevel::Verbose => LogLevel::Info,
            LogLevel::Debug => LogLevel::Verbose,
            LogLevel::Debug2 => LogLevel::Debug,
            LogLevel::Debug3 => LogLevel::Debug2,
        }
    }
}

impl Default for LogLevel {
    fn default() -> LogLevel {
        LogLevel::Info
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        match self {
            &LogLevel::Quiet => "QUIET".fmt(fmt),
            &LogLevel::Fatal => "FATAL".fmt(fmt),
            &LogLevel::Error => "ERROR".fmt(fmt),
            &LogLevel::Info => "INFO".fmt(fmt),
            &LogLevel::Verbose => "VERBOSE".fmt(fmt),
            &LogLevel::Debug => "DEBUG".fmt(fmt),
            &LogLevel::Debug2 => "DEBUG2".fmt(fmt),
            &LogLevel::Debug3 => "DEBUG3".fmt(fmt),
        }
    }
}

impl FromStr for LogLevel {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_ref() {
            "QUIET" => Ok(LogLevel::Quiet),
            "FATAL" => Ok(LogLevel::Fatal),
            "ERROR" => Ok(LogLevel::Error),
            "INFO" => Ok(LogLevel::Info),
            "VERBOSE" => Ok(LogLevel::Verbose),
            "DEBUG" => Ok(LogLevel::Debug),
            "DEBUG1" => Ok(LogLevel::Debug),
            "DEBUG2" => Ok(LogLevel::Debug2),
            "DEBUG3" => Ok(LogLevel::Debug3),
            _ => Err(ParseError),
        }
    }
}

// pub enum CanonicalizeHostname {
//     Always,
//     No,
//     Yes,
// }

#[derive(Debug, Clone, Copy)]
pub enum Protocol {
    Http,
    Https,
}

impl Protocol {
    pub fn default_port(&self) -> u16 {
        match self {
            &Protocol::Http => 80,
            &Protocol::Https => 443,
        }
    }
}

impl Default for Protocol {
    fn default() -> Protocol {
        Protocol::Https
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        match self {
            &Protocol::Http => "http".fmt(fmt),
            &Protocol::Https => "https".fmt(fmt),
        }
    }
}

impl FromStr for Protocol {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "http" => Ok(Protocol::Http),
            "https" => Ok(Protocol::Https),
            _ => Err(ParseError),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RequestTTY {
    Auto,
    Force,
    No,
    Yes,
}

impl Default for RequestTTY {
    fn default() -> RequestTTY {
        RequestTTY::Auto
    }
}

impl fmt::Display for RequestTTY {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        match self {
            &RequestTTY::Auto => "auto".fmt(fmt),
            &RequestTTY::Force => "force".fmt(fmt),
            &RequestTTY::No => "no".fmt(fmt),
            &RequestTTY::Yes => "yes".fmt(fmt),
        }
    }
}

impl FromStr for RequestTTY {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "auto" => Ok(RequestTTY::Auto),
            "force" => Ok(RequestTTY::Force),
            "no" | "false" => Ok(RequestTTY::No),
            "yes" | "true" => Ok(RequestTTY::Yes),
            _ => Err(ParseError),
        }
    }
}

#[derive(Default)]
pub struct OptionsBuilder {
    tokens: HashMap<char, String>,
    container: Container,
    environment: Option<String>,
    escape_char: Option<char>,
    host_name: Option<String>,
    log_level: LogLevel,
    port: Option<u16>,
    protocol: Protocol,
    remote_command: Option<String>,
    request_tty: RequestTTY,
    send_env: Vec<pattern::Pattern>,
    service: Option<String>,
    stack: Option<String>,
    user: Option<String>,
}

impl OptionsBuilder {
    pub fn build(mut self) -> Result<Options, BuildError> {
        let user = match self.user {
            Some(ref v) => v.to_owned(),
            None => users::get_current_username().unwrap_or("root".to_string()),
        };
        self.token('r', user.clone());
        let environment = expand(
            &self.environment.ok_or(BuildError::MissingEnvironment)?,
            &['e', 'S', 's'],
            &self.tokens,
        )?;
        let host_name = expand(
            &self.host_name.ok_or(BuildError::MissingHostName)?,
            &['h'],
            &self.tokens,
        )?;
        let remote_command = expand(
            &self.remote_command.unwrap_or("login -p -f %r".to_string()),
            &['r'],
            &self.tokens,
        )?;
        let stack = expand(
            &self.stack.ok_or(BuildError::MissingStack)?,
            &['e', 'S', 's'],
            &self.tokens,
        )?;
        let service = expand(
            &self.service.ok_or(BuildError::MissingService)?,
            &['e', 'S', 's'],
            &self.tokens,
        )?;
        Ok(Options {
            container: self.container,
            environment,
            escape_char: self.escape_char,
            host_name,
            log_level: self.log_level,
            port: self.port.unwrap_or(self.protocol.default_port()),
            protocol: self.protocol,
            remote_command,
            request_tty: self.request_tty,
            send_env: self.send_env,
            service,
            stack,
            user,
        })
    }

    pub fn token<'a>(&'a mut self, token: char, replacement: String) -> &'a mut OptionsBuilder {
        self.tokens.insert(token, replacement);
        self
    }

    pub fn container<'a>(&'a mut self, container: Container) -> &'a mut OptionsBuilder {
        self.container = container;
        self
    }

    pub fn environment<'a>(&'a mut self, environment: String) -> &'a mut OptionsBuilder {
        self.environment = Some(environment);
        self
    }

    pub fn escape_char<'a>(&'a mut self, escape_char: char) -> &'a mut OptionsBuilder {
        self.escape_char = Some(escape_char);
        self
    }

    pub fn host_name<'a>(&'a mut self, host_name: String) -> &'a mut OptionsBuilder {
        self.host_name = Some(host_name);
        self
    }

    pub fn log_level<'a>(&'a mut self, log_level: LogLevel) -> &'a mut OptionsBuilder {
        self.log_level = log_level;
        self
    }

    pub fn port<'a>(&'a mut self, port: u16) -> &'a mut OptionsBuilder {
        self.port = Some(port);
        self
    }

    pub fn protocol<'a>(&'a mut self, protocol: Protocol) -> &'a mut OptionsBuilder {
        self.protocol = protocol;
        self
    }

    pub fn remote_command<'a>(&'a mut self, remote_command: String) -> &'a mut OptionsBuilder {
        self.remote_command = Some(remote_command);
        self
    }

    pub fn request_tty<'a>(&'a mut self, request_tty: RequestTTY) -> &'a mut OptionsBuilder {
        self.request_tty = request_tty;
        self
    }

    pub fn send_env<'a>(&'a mut self, pattern: pattern::Pattern) -> &'a mut OptionsBuilder {
        self.send_env.push(pattern);
        self
    }

    pub fn service<'a>(&'a mut self, service: String) -> &'a mut OptionsBuilder {
        self.service = Some(service);
        self
    }

    pub fn stack<'a>(&'a mut self, stack: String) -> &'a mut OptionsBuilder {
        self.stack = Some(stack);
        self
    }

    pub fn user<'a>(&'a mut self, user: String) -> &'a mut OptionsBuilder {
        self.user = Some(user);
        self
    }
}

fn expand(
    string: &str,
    allowed: &[char],
    map: &HashMap<char, String>,
) -> Result<String, BuildError> {
    let mut replace_next = false;
    let mut res = Vec::new();
    for c in string.chars() {
        if replace_next && c == '%' {
            replace_next = false;
            res.push(c);
        } else if replace_next && allowed.contains(&c) {
            replace_next = false;
            match map.get(&c) {
                Some(string) => res.extend(string.chars()),
                None => res.extend(&['%', c]),
            };
        } else if replace_next {
            return Err(BuildError::UnknownToken(c));
        } else if c == '%' {
            replace_next = true;
        } else {
            res.push(c);
        }
    }
    Ok(res.iter().collect())
}

pub struct Options {
    // pub canonical_domains: Vec<String>,
    // pub canonicalize_fallback_local: bool, // default true
    // pub canonicalize_hostname: CanonicalizeHostname, // default no
    // pub canonicalize_max_dots: u16, // default 1
    // pub canonicalize_permitted_cnames: Vec<Rule>,
    // pub connection_attempts: u16, // default 1
    // pub connect_timeout: Option<u16>,
    pub container: Container,
    pub environment: String,
    pub escape_char: Option<char>, // -e default "~"
    pub host_name: String,
    // pub ignore_unknown: Vec<Pattern>,
    // pub local_command: Option<String>,
    pub log_level: LogLevel, // -q quiet -v verbose -vv debug -vvv debug2, default info
    // pub number_of_password_prompts: u16, // default 3
    // pub permit_local_command: bool, // default false
    pub port: u16, // -p default protocol.default_port()
    pub protocol: Protocol, // default https
    // pub proxy_command: Option<String>,
    // pub proxy_use_fdpass: bool, // default false
    pub remote_command: String,
    pub request_tty: RequestTTY, // -T no -t yes -tt force, default auto
    pub send_env: Vec<pattern::Pattern>,
    // pub server_alive_count_max: u16, // default 3
    // pub server_alive_interval: u16, // default 0
    pub service: String,
    pub stack: String, // default stack
    // pub tcp_keep_alive: bool, // default true, 7200
    pub user: String, // -l
}

impl Options {
    pub fn url(&self) -> url::Url {
        if self.port == self.protocol.default_port() {
            url::Url::parse(&format!("{}://{}", self.protocol, self.host_name)).unwrap()
        } else {
            url::Url::parse(&format!(
                "{}://{}:{}",
                self.protocol,
                self.host_name,
                self.port
            )).unwrap()
        }
    }

    pub fn host_with_port(&self) -> String {
        if self.port == self.protocol.default_port() {
            format!("{}", self.host_name)
        } else {
            format!("{}:{}", self.host_name, self.port)
        }
    }
}

impl fmt::Display for Options {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        write!(fmt, "protocol {}\n", self.protocol)?;
        write!(fmt, "user {}\n", self.user)?;
        write!(fmt, "hostname {}\n", self.host_name)?;
        write!(fmt, "port {}\n", self.port)?;
        write!(fmt, "environment {}\n", self.environment)?;
        write!(fmt, "stack {}\n", self.stack)?;
        write!(fmt, "service {}\n", self.service)?;
        write!(fmt, "container {}\n", self.container)?;
        match self.escape_char {
            Some(ref v) => write!(fmt, "escapechar {}\n", v)?,
            None => write!(fmt, "escapechar none\n")?,
        }
        write!(fmt, "loglevel {}\n", self.log_level)?;
        write!(fmt, "remotecommand {}\n", self.remote_command)?;
        write!(fmt, "requesttty {}\n", self.request_tty)?;
        for pattern in &self.send_env {
            write!(fmt, "sendenv {}\n", pattern)?;
        }
        Ok(())
    }
}
