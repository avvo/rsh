extern crate actix;
extern crate actix_web;
extern crate futures;
extern crate getopts;
#[macro_use]
extern crate rsh;
extern crate url;

use actix_web::ws;
use futures::future::Future;
use rsh::{*, cli::ProgramStatus, options::Options, rancher::{ContainerLogs, HostAccess}};

fn main() {
    let mut opts = getopts::Options::new();
    opts.optflag("h", "help", "Print this message and exit");
    opts.optflag("V", "version", "Display the version number and exit");

    opts.optflag("f", "", "Do not stop at end of file, wait for additional data");
    // opts.optflag("F", "", "Detect new containers, implies -f");
    opts.optopt("n", "", "Number of lines", "NUMBER");
    opts.optflag("q", "", "Suppress printing headers for multiple containers");

    // inherited from rsh

    opts.optopt(
        "E",
        "",
        "Append debug logs to LOGFILE instead of standard error",
        "LOGFILE",
    );
    opts.optopt(
        "",
        "config", // notmally -F, but that's taken
        "Specifies an alternative configuration file",
        "FILE",
    );
    opts.optflag("G", "", "Print the configuration and exit");
    opts.optmulti("o", "", "Set an option by name", "OPTION");
    opts.optopt("p", "", "Port to connect to on the remote host", "PORT");
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
    status.exit(opts, &program, "...")
}

fn run(matches: getopts::Matches) -> ProgramStatus {
    cli::set_log_level(&matches);

    if let Err(_) = cli::set_log_file(&matches) {
        return ProgramStatus::Failure;
    }

    if let Some(status) = cli::is_early_exit(&matches, "rtail") {
        return status;
    }

    let result: Result<Vec<Options>, ProgramStatus> = matches.free.clone().into_iter().map(|host| {
        let config = match cli::config(&matches, "config") {
            Ok(v) => v,
            Err(_) => return Err(ProgramStatus::Failure),
        };

        if let Some(value) = config.log_level(&host) {
            log::set_level(value);
        }

        cli::options_for_host(&matches, &config, &host)
    }).collect();

    let all_options = match result {
        Ok(v) => v,
        Err(s) => return s,
    };

    if all_options.is_empty() {
        return ProgramStatus::FailureWithHelp;
    };

    verbose!("rtail {}", VERSION);

    if matches.opt_present("G") {
        print!("{}", all_options.iter().map(Options::to_string).collect::<Vec<_>>().join("\n"));
        return ProgramStatus::Success;
    }

    // handle q

    let sys = actix::System::new("rtail");

    let mut failure = false;
    let mut manager = rancher::Manager::new();
    let mut addrs = Vec::new();
    for options in all_options {
        let containers = cli::get_containers(&mut manager, &options, |c| c.actions.get("logs").is_some()).unwrap_or_else(|_| Vec::new());
        if containers.len() == 0 {
            error!("rtail: {}: No such container", options.url());
            failure = true;
        }
        for c in containers.into_iter() {
            let (client, _) = manager.get(options.host_with_port().to_owned());
            let url = c.actions.get("logs").unwrap();
            let host_access: HostAccess = client.post(url, &ContainerLogs::new(true, 10)).expect("logs failed");
            addrs.push(connect(host_access.authed_url()));
        }
    }

    match sys.run() {
        0 if failure => {
            debug2!("connection closed successfully");
            ProgramStatus::Failure
        }
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

fn connect(websocket_url: url::Url) {
    let client = ws::Client::new(websocket_url)
        .connect()
        .map_err(|e| {
            debug3!("{:?}\r", e);
            actix::Arbiter::system().do_send(actix::msgs::SystemExit(1));
        })
        .map(move |(reader, writer)| {
            handler::Read::start(reader, writer);
        });
    actix::Arbiter::handle().spawn(client);
}
