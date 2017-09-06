extern crate base64;
extern crate futures;
extern crate getopts;
#[macro_use]
extern crate lazy_static;
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
mod escape;
mod options;
mod prompt;
mod rancher;

use prompt::prompt_with_default;
use rancher::{ContainerExec, HostAccess};

const NAME: &'static str = env!("CARGO_PKG_NAME");
const VERSION: &'static str = env!("CARGO_PKG_VERSION");

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
    opts.optflagmulti(
        "v",
        "",
        "Verbose mode. Multiple -v options increase the verbosity",
    );

    let mut args: Vec<String> = std::env::args().collect();
    let program = args.remove(0);
    let brief = format!(
        "Usage: {} [options] [scheme://][user@]host[:port][[/environment]/stack]/service [command]",
        program
    );

    let matches = match opts.parse(args) {
        Err(e) => {
            eprintln!("{}\n\n{}", e, opts.usage(&brief));
            std::process::exit(1);
        }
        Ok(matches) => matches,
    };

    if matches.opt_present("version") {
        println!("{} {}", NAME, VERSION);
        std::process::exit(0);
    } else if matches.opt_present("help") {
        println!("{}", opts.usage(&brief));
        std::process::exit(0);
    }

    let host = match matches.free.get(0) {
        Some(v) => v,
        None => {
            eprintln!("{}", opts.usage(&brief));
            std::process::exit(1);
        }
    };

    let url = match if !host.contains("://") {
        url::Url::parse(&format!("{}://{}", options::Protocol::default(), host))
    } else {
        url::Url::parse(host)
    } {
        Ok(v) => v,
        Err(_) => {
            eprintln!("{}", opts.usage(&brief));
            std::process::exit(1);
        }
    };

    if url.cannot_be_a_base() {
        eprintln!("{}", opts.usage(&brief));
        std::process::exit(1);
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
            eprintln!("{}", opts.usage(&brief));
            std::process::exit(1);
        };

        match (first, second, third) {
            (None, None, None) => (None, None, None),
            (a @ Some(_), None, None) => (None, None, a),
            (a @ Some(_), b @ Some(_), None) => (None, a, b),
            (a @ Some(_), b @ Some(_), c @ Some(_)) => (a, b, c),
            _ => panic!("didn't expect a path segment to follow None"),
        }
    };

    match matches.opt_str("E") {
        Some(ref path) => {
            let result = std::fs::OpenOptions::new().create(true).append(true).open(
                path,
            );
            match result {
                Ok(file) => log::set_device(file),
                Err(e) => {
                    eprintln!("Couldn't open logfile {}: {}", path, e);
                    std::process::exit(1);
                }
            };

            let file = std::fs::File::create(path).unwrap();
            log::set_device(file);
        }
        None => (),
    };

    if matches.opt_present("q") || matches.free.len() > 1 {
        log::set_level(options::LogLevel::Quiet);
    }

    match matches.opt_count("v") {
        0 => (),
        1 => log::set_level(options::LogLevel::Debug),
        2 => log::set_level(options::LogLevel::Debug2),
        _ => log::set_level(options::LogLevel::Debug3),
    };

    let mut config_path = std::env::home_dir().unwrap_or(std::path::PathBuf::from("/"));
    config_path.push(".rsh");
    std::fs::create_dir_all(&config_path).expect("couldn't create config dir");

    let mut option_builder = options::OptionsBuilder::default();

    // log level was set as early as possible, make sure the options stay in
    // sync
    option_builder.log_level(log::level());

    option_builder.protocol(match url.scheme() {
        "http" => options::Protocol::Http,
        "https" => options::Protocol::Https,
        _ => {
            eprintln!("{}", opts.usage(&brief));
            std::process::exit(1);
        }
    });

    if !url.username().is_empty() {
        option_builder.user(url.username().into());
    } else if matches.opt_present("l") {
        option_builder.user(matches.opt_str("l").unwrap());
    }

    if url.host_str().is_some() {
        option_builder.host_name(url.host_str().unwrap().into());
    }

    if url.port().is_some() {
        option_builder.port(url.port().unwrap());
    } else if matches.opt_present("p") {
        let port_string = matches.opt_str("p").unwrap();
        match port_string.parse() {
            Ok(v) => option_builder.port(v),
            Err(_) => {
                eprintln!("Bad port '{}'.", port_string);
                std::process::exit(1);
            }
        };
    }

    if environment.is_some() {
        option_builder.environment(environment.unwrap().into());
    }

    if stack.is_some() {
        option_builder.stack(stack.unwrap().into());
    }

    if service.is_some() {
        option_builder.service(service.unwrap().into());
    }

    if matches.opt_present("e") {
        let escape_str = matches.opt_str("e").unwrap();
        if escape_str != "none" {
            match escape_str.parse::<char>() {
                Ok(v) if v.is_ascii() => option_builder.escape_char(v),
                _ => {
                    eprintln!("Bad escape character '{}'.", escape_str);
                    std::process::exit(1);
                }
            };
        }
    } else {
        option_builder.escape_char('~');
    }

    if matches.opt_count("t") > 1 {
        option_builder.request_tty(options::RequestTTY::Force);
    } else if matches.opt_present("t") {
        option_builder.request_tty(options::RequestTTY::Yes);
    } else if matches.opt_present("T") {
        option_builder.request_tty(options::RequestTTY::No);
    }

    if matches.free.len() > 1 {
        let vec: Vec<_> = matches.free[1..]
            .iter()
            .map(|s| shell_escape::escape(s.clone().into()))
            .collect();
        option_builder.remote_command(vec.join(" "));
    };

    let options = match option_builder.build() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("{}", opts.usage(&brief));
            std::process::exit(1);
        }
    };

    if matches.opt_present("G") {
        println!("{}", options);
        std::process::exit(0);
    }

    let mut client = rancher::Client::new();

    let mut api_key_path = config_path.clone();
    let host = if options.port == options.protocol.default_port() {
        format!("{}", options.host_name)
    } else {
        format!("{}:{}", options.host_name, options.port)
    };
    api_key_path.push(host);
    match std::fs::File::open(&api_key_path).map(std::io::BufReader::new) {
        Ok(mut reader) => {
            let mut string = String::new();
            reader.read_to_string(&mut string).expect(
                "failed to read api key",
            );
            client.api_key = serde_json::from_str(&string).expect("failed to parse json");
        }
        Err(_) => (),
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
                let user = prompt_with_default("Rancher User", std::env::var("USER").ok())
                    .expect("couldn't get user");
                let password = rpassword::prompt_password_stdout(&"Rancher Password: ")
                    .expect("couldn't get password");
                match client.ldap_auth(&url, &user, &password) {
                    Ok(_) => (),
                    Err(_) => {
                        fatal!("Authentication failed.");
                        std::process::exit(1);
                    }
                };
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
                std::process::exit(1);
            }
            Err(e) => {
                fatal!("{}", e);
                std::process::exit(1);
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
    let host_access: HostAccess = client
        .post(execute_url, &ContainerExec::new(exec, is_tty))
        .expect("execute failed");

    {
        // raw mode will stay in effect until the raw var is dropped
        // we otherwise don't actually need it for anything
        let raw = if is_tty {
            let mut stdout = std::io::stdout().into_raw_mode().unwrap();
            stdout.flush().unwrap();
            Some(stdout)
        } else {
            None
        };

        let (sender, receiver) = futures::sync::mpsc::channel(0);
        std::thread::spawn(move || {
            let mut escape_scanner = escape::scanner(options.escape_char);
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

        let mut core = tokio_core::reactor::Core::new().unwrap();
        let mut stdout = std::io::stdout();

        let runner = websocket::ClientBuilder::from_url(&host_access.authed_url())
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
                    receiver.map_err(|_| websocket::result::WebSocketError::NoDataAvailable),
                ).forward(sink)
            });
        core.run(runner).unwrap();

        // don't really need to do this, but the compiler wants us to use raw
        // for *something*
        match raw {
            Some(mut stdout) => stdout.flush().unwrap(),
            None => (),
        };
    }

    info!("\nConnection to {} closed.", url);
}
