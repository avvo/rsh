extern crate actix;
extern crate actix_web;
extern crate base64;
extern crate futures;
extern crate getopts;
extern crate rpassword;
#[macro_use]
extern crate rsh;
extern crate serde_json;
extern crate shell_escape;
extern crate termion;
extern crate url;
extern crate users;

use std::io::Write;

use actix_web::ws;
use futures::future::Future;
use rsh::{*, cli::ProgramStatus, rancher::{ContainerExec, HostAccess}};
use termion::raw::IntoRawMode;

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

    let status = run(matches);
    status.exit(opts, &program, "[cmd]")
}

fn run(matches: getopts::Matches) -> ProgramStatus {
    if matches.opt_present("q") || matches.free.len() > 1 {
        log::set_level(options::LogLevel::Quiet);
    }

    cli::set_log_level(&matches);

    if let Err(_) = cli::set_log_file(&matches) {
        return ProgramStatus::Failure;
    }

    if let Some(status) = cli::is_early_exit(&matches, NAME) {
        return status;
    }

    let host = match matches.free.get(0) {
        Some(v) => v.clone(),
        None => return ProgramStatus::FailureWithHelp,
    };

    verbose!("{} {}", NAME, VERSION);

    let config = match cli::config(&matches, "F") {
        Ok(v) => v,
        Err(_) => return ProgramStatus::Failure,
    };

    if let Some(value) = config.log_level(&host) {
        log::set_level(value);
    }

    let options = match cli::options_for_host(&matches, &config, &host) {
        Ok(v) => v,
        Err(s) => return s,
    };

    if matches.opt_present("G") {
        print!("{}", options);
        return ProgramStatus::Success;
    }

    run_with_options(options)
}

fn run_with_options(options: options::Options) -> ProgramStatus {
    let mut manager = rancher::Manager::new();
    let containers = match cli::get_containers(&mut manager, &options) {
        Ok(v) => v,
        Err(s) => return s,
    };

    if containers.len() == 0 {
        fatal!("Couldn't find container.");
        return ProgramStatus::Failure;
    }

    let is_tty = match options.request_tty {
        options::RequestTTY::Force => true,
        options::RequestTTY::Auto => options.remote_command.starts_with("login -p -f "),
        options::RequestTTY::Yes => termion::is_tty(&std::fs::File::create("/dev/stdout").unwrap()),
        options::RequestTTY::No => false,
    };

    let container = match options.container {
        options::Container::First => &containers[0],
        options::Container::Auto if containers.len() == 1 || !is_tty => &containers[0],
        options::Container::Menu | options::Container::Auto => prompt::user_choice(&containers).expect("failed to get container choice"),
    };

    let execute_url = container.actions.get("execute").expect(
        "expected executeable container",
    );

    let (client, _) = manager.get(options.host_with_port().to_owned());
    let url = options.url();

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
        status = connect(host_access.authed_url(), options.escape_char);
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

fn connect(websocket_url: url::Url, escape_char: Option<char>) -> ProgramStatus {
    let sys = actix::System::new("rsh");

    let client = ws::Client::new(websocket_url)
        .connect()
        .map_err(|e| {
            debug3!("{:?}\r", e);
            actix::Arbiter::system().do_send(actix::msgs::SystemExit(1));
        })
        .map(move |(reader, writer)| {
            handler::ReadWrite::start(escape_char, reader, writer);
        });

    debug!("Connecting to websocket\r");
    sys.handle().spawn(client);
    match sys.run() {
        0 => {
            debug2!("connection closed successfully");
            ProgramStatus::Success
        }
        i => {
            debug2!("connection closed with error {:?}", i);
            ProgramStatus::Failure
        }
    }
}
