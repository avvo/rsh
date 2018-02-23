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
extern crate users;
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
    opts.optopt(
        "F",
        "",
        "Specifies an alternative configuration file",
        "CONFIGFILE",
    );
    opts.optflag("G", "", "Print the configuration and exit");
    opts.optopt(
        "l",
        "",
        "Specifies the user to log in as on the remote machine",
        "USER",
    );
    opts.optmulti("o", "", "Set an option by name", "OPTION");
    opts.optopt("p", "", "Port to connect to on the remote host", "PORT");
    opts.optflag("q", "", "Quiet mode");
    opts.optflag("T", "", "Disable pseudo-terminal allocation");
    opts.optflagmulti("t", "", "Force pseudo-terminal allocation");
    opts.optflagmulti("v", "", "Verbose mode, multiples increase the verbosity");

    let mut args: Vec<String> = std::env::args().collect();
    let program = args.remove(0);

    let matches = match opts.parse(args) {
        Err(e) => {
            eprint!("{}\n{}", e, opts.short_usage(&program));
            std::process::exit(1);
        }
        Ok(matches) => matches,
    };

    match run(matches) {
        ProgramStatus::Success => (),
        ProgramStatus::SuccessWithHelp => {
            print!(
                "{}",
                opts.usage(&format!(
                    "Usage: {} [opts] [protocol://][user@]host[:port][[/env]/stack]/service [cmd]",
                    program
                ))
            )
        }
        ProgramStatus::Failure => std::process::exit(1),
        ProgramStatus::FailureWithHelp => {
            eprint!("{}", opts.short_usage(&program));
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

    let config_paths = match matches.opt_str("F").map(std::path::PathBuf::from) {
        Some(val) => vec![val],
        None => vec![config::user_config_path(), config::system_config_path()],
    };

    let config = {
        let mut acc = match config::Config::try_from(
            matches.opt_strs("o").iter().map(AsRef::as_ref).collect(),
        ) {
            Ok(v) => v,
            Err(config::Error::OptionError(key, value)) => {
                fatal!("Bad configuration option: \"{}\" for {}.", value, key);
                return ProgramStatus::Failure;
            }
            Err(config::Error::UnknownOption(key)) => {
                fatal!("Bad configuration option: {}.", key);
                return ProgramStatus::Failure;
            }
            Err(config::Error::OptionNotAllowed(key)) => {
                fatal!("{} directive not supported as a command-line option.", key);
                return ProgramStatus::Failure;
            }
            _ => {
                fatal!("Error configuration option.");
                return ProgramStatus::Failure;
            }
        };
        for path in config_paths {
            debug!("Reading configuration data {}", path.to_string_lossy());
            match config::open_config(&path) {
                Ok(v) => acc = acc.append(v),
                Err(config::Error::OptionError(key, value)) => {
                    fatal!(
                        "{}: Bad configuration option: \"{}\" for {}.",
                        path.to_string_lossy(),
                        value,
                        key
                    );
                    return ProgramStatus::Failure;
                }
                Err(config::Error::UnknownOption(key)) => {
                    fatal!(
                        "{}: Bad configuration option: {}.",
                        path.to_string_lossy(),
                        key
                    );
                    return ProgramStatus::Failure;
                }
                Err(config::Error::IoError(_)) => {
                    fatal!("{}: Error reading config.", path.to_string_lossy());
                    return ProgramStatus::Failure;
                }
                _ => {
                    fatal!("{}: Error parsing config.", path.to_string_lossy());
                    return ProgramStatus::Failure;
                }
            }
        }
        acc
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

    if let Some(ref value) = environment {
        option_builder.token('e', value.to_owned());
    }
    if let Some(ref value) = stack {
        option_builder.token('S', value.to_owned());
    }
    if let Some(ref value) = service {
        option_builder.token('s', value.to_owned());
    }

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

    let url_host = url.host_str().expect(
        "cannot-be-a-base URL bypassed check?",
    );
    option_builder.token('h', url_host.to_string());
    option_builder.host_name(config.host_name(&host).unwrap_or(url_host.to_string()));

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

    if let Some(value) = config.environment(&host).or(environment) {
        option_builder.environment(value.into());
    }

    if let Some(value) = config.stack(&host).or(stack) {
        option_builder.stack(value.into());
    }

    if let Some(value) = config.service(&host).or(service) {
        option_builder.service(value.into());
    }

    if let Some(value) = config.container(&host) {
        option_builder.container(value.into());
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

    for pattern in config.send_env(&host) {
        option_builder.send_env(pattern);
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
        Err(options::BuildError::MissingEnvironment) => {
            verbose!("Missing environment.");
            return ProgramStatus::FailureWithHelp;
        }
        Err(options::BuildError::MissingHostName) => {
            verbose!("Missing host name.");
            return ProgramStatus::FailureWithHelp;
        }
        Err(options::BuildError::MissingService) => {
            verbose!("Missing service.");
            return ProgramStatus::FailureWithHelp;
        }
        Err(options::BuildError::MissingStack) => {
            verbose!("Missing stack.");
            return ProgramStatus::FailureWithHelp;
        }
        Err(options::BuildError::UnknownToken(c)) => {
            fatal!("Unknown token %{}.", c);
            return ProgramStatus::Failure;
        }
    };

    if matches.opt_present("G") {
        print!("{}", options);
        return ProgramStatus::Success;
    }

    run_with_options(options)
}

fn run_with_options(options: options::Options) -> ProgramStatus {
    let mut client = rancher::Client::new();

    let api_key_path = config::api_key_path(&options.host_with_port());
    debug!(
        "Reading Rancher API key from {}",
        api_key_path.to_string_lossy()
    );
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
        Err(_) => {
            debug!(
                "{} No such file or directory",
                api_key_path.to_string_lossy()
            )
        }
    };

    let is_tty = match options.request_tty {
        options::RequestTTY::Force => true,
        options::RequestTTY::Auto => options.remote_command.starts_with("login -p -f "),
        options::RequestTTY::Yes => termion::is_tty(&std::fs::File::create("/dev/stdout").unwrap()),
        options::RequestTTY::No => false,
    };

    let mut tries = 0;
    let url = options.url();
    let containers = loop {
        match client.executeable_containers(
            &url,
            &options.environment,
            &options.stack,
            &options.service,
        ) {
            Ok(v) => break v,
            Err(rancher::Error::Unauthorized) if tries == 0 => {
                debug2!("Received Unauthorized, attempting authentication");
                let user = prompt_with_default("Rancher User", users::get_current_username())
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
            Err(e) => {
                fatal!("{}", e);
                return ProgramStatus::Failure;
            }
        };
        // need to give rancher a tiny bit of time to activate the api keys
        std::thread::sleep(std::time::Duration::from_millis(250));
        tries += 1;
    };

    if containers.len() == 0 {
        fatal!("Couldn't find container.");
        return ProgramStatus::Failure;
    }

    let container = match options.container {
        options::Container::First => &containers[0],
        options::Container::Auto if containers.len() == 1 || !is_tty => &containers[0],
        options::Container::Menu | options::Container::Auto => prompt::user_choice(&containers).expect("failed to get container choice"),
    };

    let execute_url = container.actions.get("execute").expect(
        "expected executeable container",
    );

    let mut command_parts = Vec::new();
    let mut send_env_patterns = options.send_env;
    send_env_patterns.push("TERM".parse().unwrap());
    let send_env = pattern::PatternList::from(send_env_patterns);
    for (key, val) in std::env::vars() {
        if send_env.matches(&key) {
            command_parts.push(format!("{}={}", key, val));
            command_parts.push(format!("export {}", key));
        }
    }

    if is_tty {
        match termion::terminal_size() {
            Ok((cols, rows)) => command_parts.push(format!("stty cols {} rows {}", cols, rows)),
            Err(_) => (),
        };
        command_parts.push(format!(
            "([ -x /usr/bin/script ] && /usr/bin/script -q -c {} /dev/null || exec {})",
            shell_escape::escape(options.remote_command.clone().into()),
            options.remote_command
        ));
    } else {
        command_parts.push(options.remote_command);
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

fn connect(
    websocket_url: url::Url,
    stdin: futures::sync::mpsc::Receiver<websocket::OwnedMessage>,
) -> ProgramStatus {
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
                    websocket::OwnedMessage::Close(e) => Some(websocket::OwnedMessage::Close(e)),
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
