extern crate base64;
extern crate futures;
extern crate getopts;
extern crate rpassword;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate shell_escape;
extern crate termion;
extern crate tokio_core;
extern crate websocket;

use futures::future::Future;
use futures::sink::Sink;
use futures::stream::Stream;

use std::io::Read;
use std::io::Write;
use termion::raw::IntoRawMode;

mod and_select;
mod prompt;
mod rancher;
mod ucl;

use prompt::prompt_with_default;
use rancher::{ContainerExec, HostAccess};
use ucl::Ucl;

const NAME: &'static str = env!("CARGO_PKG_NAME");
const VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn main() {
    let mut opts = getopts::Options::new();
    opts.parsing_style(getopts::ParsingStyle::StopAtFirstFree);
    opts.optflag("h", "help", "Print this message and exit");
    opts.optflag("V", "version", "Display the version number and exit");

    opts.optflag("T", "", "Disable pseudo-terminal allocation");
    opts.optflag("t", "", "Force pseudo-terminal allocation");

    let mut args: Vec<String> = std::env::args().collect();
    let program = args.remove(0);
    let brief = format!(
        "Usage: {} [options] [scheme://][user@]host[:port]/[environment/]stack[/service] [command]",
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

    let mut ucl = match matches.free.get(0).ok_or(ucl::Error::NoHost).and_then(
        |s| {
            Ucl::from(s)
        },
    ) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("{}", opts.usage(&brief));
            std::process::exit(1);
        }
    };

    let mut client = rancher::Client::new();

    let mut config_path = std::env::home_dir().unwrap_or(std::path::PathBuf::from("/"));
    config_path.push(".rsh");
    std::fs::create_dir_all(&config_path).expect("couldn't create config dir");

    let mut api_key_path = config_path.clone();
    let host = match ucl.url.port() {
        Some(p) => format!("{}:{}", ucl.url.host().expect("expected host"), p),
        None => format!("{}", ucl.url.host().expect("expected host")),
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
    let container = loop {
        match client.executeable_container(&mut ucl) {
            Ok(v) => break v,
            Err(rancher::Error::Unauthorized) if tries == 0 => {
                let user = prompt_with_default("Rancher User", std::env::var("USER").ok())
                    .expect("couldn't get user");
                let password = rpassword::prompt_password_stdout(&"Rancher Password: ")
                    .expect("couldn't get password");
                match client.ldap_auth(&ucl.url, &user, &password) {
                    Ok(_) => (),
                    Err(_) => {
                        eprintln!("Authentication failed.");
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
                eprintln!("Couldn't find container.");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("{}", e);
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


    let (command, close_message) = if matches.free.len() == 1 {
        let user = if ucl.url.username().is_empty() {
            "root"
        } else {
            ucl.url.username()
        };
        (
            format!("login -p -f {}", user),
            Some(format!("\nConnection to {} closed.", ucl)),
        )
    } else if matches.free.len() == 2 {
        (matches.free.get(1).unwrap().clone(), None)
    } else {
        let vec: Vec<_> = matches.free[1..]
            .iter()
            .map(|s| shell_escape::escape(s.clone().into()))
            .collect();
        (vec.join(" "), None)
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
    let is_tty = (termion::is_tty(&std::fs::File::create("/dev/stdout").unwrap()) ||
                      matches.opt_present("t")) && !matches.opt_present("T");
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
            let mut stdin = std::io::stdin();
            let mut buffer = [0; 4096];
            let mut sink = sender.wait();
            loop {
                let read = stdin.read(&mut buffer[..]).unwrap();
                let message = base64::encode(&buffer[0..read]);
                sink.send(websocket::OwnedMessage::Text(message)).unwrap();
            }
        });

        let mut core = tokio_core::reactor::Core::new().unwrap();

        let runner = websocket::ClientBuilder::from_url(&host_access.authed_url())
            .async_connect(None, &core.handle())
            .and_then(|(duplex, _)| {
                let (sink, stream) = duplex.split();
                and_select::new(
                    stream.filter_map(|message| match message {
                        websocket::OwnedMessage::Text(txt) => {
                            let mut stdout = std::io::stdout();
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

    match close_message {
        Some(v) => println!("{}", v),
        None => (),
    }
}
