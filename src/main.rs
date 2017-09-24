extern crate base64;
extern crate futures;
extern crate getopts;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate nom;
extern crate nix;
extern crate rpassword;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate shell_escape;
extern crate termion;
extern crate tokio_core;
extern crate url;
extern crate websocket;

use futures::future::Future;
use futures::sink::Sink;
use futures::stream::Stream;

use std::ascii::AsciiExt;
use std::io::{Read, Write};
use termion::raw::IntoRawMode;

#[macro_use]
mod log;

mod and_select;
mod config;
mod escape;
mod options;
mod pattern;
mod prompt;
mod rancher;

use prompt::prompt_with_default;
use rancher::{ContainerExec, HostAccess};

const NAME: &'static str = env!("CARGO_PKG_NAME");
const VERSION: &'static str = env!("CARGO_PKG_VERSION");

enum ProgramStatus {
    Success,
    SuccessWithHelp,
    Failure,
    FailureWithHelp,
}

fn main() {
    let mut opts = getopts::Options::new();
    opts.parsing_style(getopts::ParsingStyle::StopAtFirstFree);
    opts.optflag("h", "help", "Print this message and exit");
    opts.optflag("V", "version", "Display the version number and exit");

    opts.optopt(
        "E",
        "",
        "Append debug logs to LOGFILE instead of standard error",
        "LOGFILE",
    );
    opts.optopt("e", "", "Sets the escape character (default: `~')", "CHAR");
    opts.optopt("F", "", "Specifies an alternative configuration file", "CONFIGFILE");
    opts.optflag("G", "", "Print the configuration and exit");
    opts.optopt(
        "l",
        "",
        "Specifies the user to log in as on the remote machine",
        "USER",
    );
    opts.optopt("p", "", "Port to connect to on the remote host", "PORT");
    opts.optflag("q", "", "Quiet mode");
    opts.optflag("T", "", "Disable pseudo-terminal allocation");
    opts.optflagmulti("t", "", "Force pseudo-terminal allocation");
    opts.optflagmulti("v", "", "Verbose mode, multiples increase the verbosity");

    let mut args: Vec<String> = std::env::args().collect();
    let program = args.remove(0);
    let brief = format!(
        "Usage: {} [opts] [protocol://][user@]host[:port][[/env]/stack]/service [cmd]",
        program
    );

    let matches = match opts.parse(args) {
        Err(e) => {
            eprint!("{}\n{}", e, opts.usage(&brief));
            std::process::exit(1);
        }
        Ok(matches) => matches,
    };

    match run(matches) {
        ProgramStatus::Success => (),
        ProgramStatus::SuccessWithHelp => print!("{}", opts.usage(&brief)),
        ProgramStatus::Failure => std::process::exit(1),
        ProgramStatus::FailureWithHelp => {
            eprint!("{}", opts.usage(&brief));
            std::process::exit(1);
        }
    };
}

fn run(matches: getopts::Matches) -> ProgramStatus {
    if matches.opt_present("q") || matches.free.len() > 1 {
        log::set_level(options::LogLevel::Quiet);
    }

    match matches.opt_count("v") {
        0 => (),
        1 => log::set_level(options::LogLevel::Debug),
        2 => log::set_level(options::LogLevel::Debug2),
        _ => log::set_level(options::LogLevel::Debug3),
    };

    match matches.opt_str("E") {
        Some(ref path) => {
            let result = std::fs::OpenOptions::new().create(true).append(true).open(
                path,
            );
            match result {
                Ok(file) => log::set_device(file),
                Err(e) => {
                    fatal!("Couldn't open logfile {}: {}", path, e);
                    return ProgramStatus::Failure;
                }
            };

            let file = std::fs::File::create(path).unwrap();
            log::set_device(file);
        }
        None => (),
    };

    if matches.opt_present("version") {
        println!("{} {}", NAME, VERSION);
        return ProgramStatus::Success;
    } else if matches.opt_present("help") {
        return ProgramStatus::SuccessWithHelp;
    }

    let host = match matches.free.get(0) {
        Some(v) => v.clone(),
        None => return ProgramStatus::FailureWithHelp,
    };

    verbose!("{} {}", NAME, VERSION);

    std::fs::create_dir_all(config::user_config_dir()).expect("couldn't create config dir");

    let config = match matches.opt_str("F").map(std::path::PathBuf::from) {
        Some(val) => open_config_or_exit(val),
        None => {
            let user_config = open_config_or_exit(config::user_config_path());
            let system_config = open_config_or_exit(config::system_config_path());
            user_config.append(system_config)
        }
    };

    if let Some(value) = config.log_level(&host) {
        log::set_level(value);
    }

    let url = match if !host.contains("://") {
        let protocol = config.protocol(&host).unwrap_or(
            options::Protocol::default(),
        );
        url::Url::parse(&format!("{}://{}", protocol, host))
    } else {
        url::Url::parse(&host)
    } {
        Ok(v) => v,
        Err(_) => {
            verbose!("Error parsing host.");
            return ProgramStatus::FailureWithHelp;
        }
    };

    if url.cannot_be_a_base() {
        verbose!("Error parsing host, non-base URL.");
        return ProgramStatus::FailureWithHelp;
    };

    let (environment, stack, service) = {
        let mut path_segments = url.path_segments()
            .expect("cannot-be-a-base URL bypassed check?")
            .map(String::from);
        let first = path_segments.next();
        let second = path_segments.next();
        let third = path_segments.next();

        if path_segments.next().is_some() {
            // weren't expecting another path segment
            verbose!("Error parsing host, too many path segments.");
            return ProgramStatus::FailureWithHelp;
        };

        match (first, second, third) {
            (None, None, None) => (None, None, None),
            (Some(ref a), None, None) if a.is_empty() => (None, None, None),
            (a @ Some(_), None, None) => (None, None, a),
            (a @ Some(_), b @ Some(_), None) => (None, a, b),
            (a @ Some(_), b @ Some(_), c @ Some(_)) => (a, b, c),
            _ => panic!("didn't expect a path segment to follow None"),
        }
    };

    let mut option_builder = options::OptionsBuilder::default();

    // log level was set as early as possible, make sure the options stay in
    // sync
    option_builder.log_level(log::level());

    option_builder.protocol(match url.scheme() {
        "http" => options::Protocol::Http,
        "https" => options::Protocol::Https,
        _ => {
            verbose!("Unsupported protocol.");
            return ProgramStatus::FailureWithHelp;
        }
    });

    if !url.username().is_empty() {
        option_builder.user(url.username().into());
    } else if let Some(value) = matches.opt_str("l").or_else(|| config.user(&host)) {
        option_builder.user(value);
    }

    if let Some(value) = config.host_name(&host).or_else(|| {
        url.host_str().map(std::convert::Into::into)
    })
    {
        option_builder.host_name(value);
    }

    if let Some(value) = url.port() {
        option_builder.port(value);
    } else if matches.opt_present("p") {
        let port_string = matches.opt_str("p").unwrap();
        match port_string.parse() {
            Ok(v) => option_builder.port(v),
            Err(_) => {
                eprintln!("Bad port '{}'.", port_string);
                return ProgramStatus::Failure;
            }
        };
    } else if let Some(value) = config.port(&host) {
        option_builder.port(value);
    }

    if let Some(value) = environment.or_else(|| config.environment(&host)) {
        option_builder.environment(value.into());
    }

    if let Some(value) = stack.or_else(|| config.stack(&host)) {
        option_builder.stack(value.into());
    }

    if let Some(value) = service.or_else(|| config.service(&host)) {
        option_builder.service(value.into());
    }

    if let Some(escape_str) = matches.opt_str("e") {
        if escape_str != "none" {
            match escape_str.parse::<char>() {
                Ok(v) if v.is_ascii() => option_builder.escape_char(v),
                _ => {
                    eprintln!("Bad escape character '{}'.", escape_str);
                    return ProgramStatus::Failure;
                }
            };
        }
    } else {
        option_builder.escape_char(config.escape_char(&host).unwrap_or('~'));
    }

    if matches.opt_count("t") > 1 {
        option_builder.request_tty(options::RequestTTY::Force);
    } else if matches.opt_present("t") {
        option_builder.request_tty(options::RequestTTY::Yes);
    } else if matches.opt_present("T") {
        option_builder.request_tty(options::RequestTTY::No);
    } else if let Some(value) = config.request_tty(&host) {
        option_builder.request_tty(value);
    }

    if matches.free.len() > 1 {
        let vec: Vec<_> = matches.free[1..]
            .iter()
            .map(|s| shell_escape::escape(s.clone().into()))
            .collect();
        option_builder.remote_command(vec.join(" "));
    } else if let Some(value) = config.remote_command(&host) {
        option_builder.remote_command(value);
    };

    let options = match option_builder.build() {
        Ok(v) => v,
        Err(options::BuildError::MissingHostName) => {
            verbose!("Missing host name.");
            return ProgramStatus::FailureWithHelp;
        }
        Err(options::BuildError::MissingService) => {
            verbose!("Missing service.");
            return ProgramStatus::FailureWithHelp;
        }
    };

    if matches.opt_present("G") {
        println!("{}", options);
        return ProgramStatus::Success;
    }

    run_with_options(options)
}

fn run_with_options(options: options::Options) -> ProgramStatus {
    let mut client = rancher::Client::new();

    let api_key_path = config::api_key_path(&options.host_with_port());
    debug!("Reading Rancher API key from {}", api_key_path.to_string_lossy());
    match std::fs::File::open(&api_key_path).map(std::io::BufReader::new) {
        Ok(mut reader) => {
            let mut string = String::new();
            reader.read_to_string(&mut string).expect(
                "failed to read api key",
            );
            client.api_key = serde_json::from_str(&string).expect("failed to parse json");
            if let Some(ref key) = client.api_key {
                debug!("Using Rancher API key {}", key.public_value);
            }
        }
        Err(_) => debug!("{} No such file or directory", api_key_path.to_string_lossy()),
    };

    let mut tries = 0;
    let url = options.url();
    let container = loop {
        match client.executeable_container(
            &url,
            &options.environment,
            &options.stack,
            &options.service,
        ) {
            Ok(v) => break v,
            Err(rancher::Error::Unauthorized) if tries == 0 => {
                debug2!("Received Unauthorized, attempting authentication");
                let user = prompt_with_default("Rancher User", std::env::var("USER").ok())
                    .expect("couldn't get user");
                let password = rpassword::prompt_password_stdout(&"Rancher Password: ")
                    .expect("couldn't get password");
                match client.ldap_auth(&url, &user, &password) {
                    Ok(_) => (),
                    Err(_) => {
                        fatal!("Authentication failed.");
                        return ProgramStatus::Failure;
                    }
                };
                debug!("Writing {}", api_key_path.to_string_lossy());
                let json_string =
                    serde_json::to_string(&client.api_key).expect("failed to construct json");
                let mut writer = std::fs::File::create(&api_key_path)
                    .map(std::io::BufWriter::new)
                    .expect("failed to write api key");
                writer.write_all(json_string.as_bytes()).expect(
                    "failed to write api key",
                );

            }
            Err(rancher::Error::Empty) => {
                fatal!("Couldn't find container.");
                return ProgramStatus::Failure;
            }
            Err(e) => {
                fatal!("{}", e);
                return ProgramStatus::Failure;
            }
        };
        // need to give rancher a tiny bit of time to activate the api keys
        std::thread::sleep(std::time::Duration::from_millis(250));
        tries += 1;
    };

    let execute_url = container.actions.get("execute").expect(
        "expected executeable container",
    );

    let is_tty = match options.request_tty {
        options::RequestTTY::Force => true,
        options::RequestTTY::Auto => options.remote_command.is_none(),
        options::RequestTTY::Yes => termion::is_tty(&std::fs::File::create("/dev/stdout").unwrap()),
        options::RequestTTY::No => false,
    };

    let command = match options.remote_command {
        Some(ref v) => v.clone(),
        None => format!("login -p -f {}", options.user),
    };

    let mut command_parts = Vec::new();
    let send_env = vec!["TERM"];
    for var in send_env {
        match std::env::var(var) {
            Ok(v) => {
                command_parts.push(format!("{}={}", var, v));
                command_parts.push(format!("export {}", var));
            }
            Err(_) => (),
        };
    }

    if is_tty {
        match termion::terminal_size() {
            Ok((cols, rows)) => command_parts.push(format!("stty cols {} rows {}", cols, rows)),
            Err(_) => (),
        };
        command_parts.push(format!(
            "([ -x /usr/bin/script ] && /usr/bin/script -q -c {} /dev/null || exec {})",
            shell_escape::escape(command.clone().into()),
            command
        ));
    } else {
        command_parts.push(command);
    }

    let exec = vec![
        String::from("/bin/sh"),
        String::from("-c"),
        command_parts.join("; "),
    ];
    debug!("Making execute request");
    debug3!("Using command {:?} and is_tty: {}", exec, is_tty);
    let host_access: HostAccess = client
        .post(execute_url, &ContainerExec::new(exec, is_tty))
        .expect("execute failed");
    debug2!("Got websocket address {}", host_access.url);

    let status;
    {
        // raw mode will stay in effect until the raw var is dropped
        // we otherwise don't actually need it for anything
        let raw = if is_tty {
            debug3!("Entering raw mode");
            let mut stdout = std::io::stdout().into_raw_mode().unwrap();
            stdout.flush().unwrap();
            Some(stdout)
        } else {
            debug3!("Not a TTY skipping raw mode");
            None
        };
        status = connect(host_access.authed_url(), get_input(options.escape_char));
        // don't really need to do this, but the compiler wants us to use raw
        // for *something*
        match raw {
            Some(mut stdout) => stdout.flush().unwrap(),
            None => (),
        };
    }
    info!("\nConnection to {} closed.", url);
    status
}

fn get_input(escape_char: Option<char>) -> futures::sync::mpsc::Receiver<websocket::OwnedMessage> {
    let (sender, receiver) = futures::sync::mpsc::channel(0);
    std::thread::spawn(move || {
        let mut escape_scanner = escape::scanner(escape_char);
        let mut stdin = std::io::stdin();
        let mut buffer = [0; 4096];
        let mut vbuffer = std::vec::Vec::new();
        let mut sink = sender.wait();
        'main: loop {
            escape_scanner.reset();
            let mut sent = 0;
            let read = stdin.read(&mut buffer[..]).unwrap();
            while sent < read {
                let escape_type = escape_scanner.next_escape(&buffer, read);
                let bytes = match escape_type {
                    escape::Escape::DecreaseVerbosity |
                    escape::Escape::Help |
                    escape::Escape::IncreaseVerbosity |
                    escape::Escape::Itself |
                    escape::Escape::Suspend |
                    escape::Escape::Terminate => &buffer[sent..escape_scanner.pos() - 1],
                    escape::Escape::Invalid => {
                        vbuffer.clear();
                        vbuffer.push(escape_scanner.char() as u8);
                        vbuffer.extend(&buffer[sent..(escape_scanner.pos())]);
                        &vbuffer[..]
                    }
                    escape::Escape::Literal | escape::Escape::None => {
                        &buffer[sent..(escape_scanner.pos())]
                    }
                };
                let message = base64::encode(bytes);
                sink.send(websocket::OwnedMessage::Text(message)).unwrap();
                sent = escape_scanner.pos();
                match escape_type {
                    escape::Escape::DecreaseVerbosity => {
                        let level = log::decrease_level();
                        println!("{}V [LogLevel {}]\r", escape_scanner.char(), level);
                    }
                    escape::Escape::Help => {
                        println!(
                            "{0}?\r
Supported escape sequences:\r
{0}.   - terminate connection\r
{0}V/v - decrease/increase verbosity (LogLevel)\r
{0}^Z  - suspend rsh\r
{0}?   - this message\r
{0}{0}   - send the escape character by typing it twice\r
(Note that escapes are only recognized immediately after newline.)\r",
                            escape_scanner.char()
                        );
                    }
                    escape::Escape::IncreaseVerbosity => {
                        let level = log::increase_level();
                        println!("{}v [LogLevel {}]\r", escape_scanner.char(), level);
                    }
                    escape::Escape::Suspend => {
                        nix::sys::signal::kill(
                            nix::unistd::getpid(),
                            Some(nix::sys::signal::Signal::SIGTSTP),
                        ).expect("failed to suspend");
                    }
                    escape::Escape::Terminate => {
                        break 'main;
                    }
                    _ => (),
                }
            }
        }
    });
    receiver
}

fn connect(websocket_url: url::Url, stdin: futures::sync::mpsc::Receiver<websocket::OwnedMessage>) -> ProgramStatus {
    let mut core = tokio_core::reactor::Core::new().unwrap();
    let mut stdout = std::io::stdout();

    debug!("Connecting to websocket\r");
    let runner = websocket::ClientBuilder::from_url(&websocket_url)
        .async_connect(None, &core.handle())
        .and_then(|(duplex, _)| {
            let (sink, stream) = duplex.split();
            and_select::new(
                stream.filter_map(|message| match message {
                    websocket::OwnedMessage::Text(txt) => {
                        stdout
                            .write(&base64::decode(&txt).expect("invalid base64"))
                            .unwrap();
                        stdout.flush().unwrap();
                        None
                    }
                    websocket::OwnedMessage::Close(e) => Some(
                        websocket::OwnedMessage::Close(e),
                    ),
                    websocket::OwnedMessage::Ping(d) => Some(websocket::OwnedMessage::Pong(d)),
                    _ => None,
                }),
                stdin.map_err(|_| websocket::result::WebSocketError::NoDataAvailable),
            ).forward(sink)
        });

    core.run(runner).unwrap();
    debug3!("connection closed");

    ProgramStatus::Success
}

// TODO figure out returning error rather than exiting
fn open_config_or_exit(config_path: std::path::PathBuf) -> config::Config {
    debug!("Reading configuration data {}", config_path.to_string_lossy());
    match config::open_config(&config_path) {
        Ok(v) => v,
        Err(config::Error::OptionError(key, value)) => {
            fatal!(
                "{}: Bad configuration option: \"{}\" for {}",
                config_path.to_string_lossy(),
                value,
                key
            );
            std::process::exit(1);
        }
        Err(config::Error::UnknownOption(key)) => {
            fatal!(
                "{}: Bad configuration option: {}",
                config_path.to_string_lossy(),
                key
            );
            std::process::exit(1);
        }
        Err(config::Error::IoError(_)) => {
            fatal!("{}: Error reading config.", config_path.to_string_lossy());
            std::process::exit(1);
        }
        _ => {
            fatal!("{}: Error parsing config.", config_path.to_string_lossy());
            std::process::exit(1);
        }
    }
}
