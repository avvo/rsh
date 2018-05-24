extern crate getopts;
#[macro_use]
extern crate rsh;

use rsh::{*, cli::ProgramStatus, options::Options};

fn main() {
    let mut opts = getopts::Options::new();
    opts.optflag("h", "help", "Print this message and exit");
    opts.optflag("V", "version", "Display the version number and exit");

    opts.optflag("f", "", "Do not stop at end of file, wait for additional data");
    opts.optflag("F", "", "Detect new containers, implies -f");
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

    let options = match result {
        Ok(v) => v,
        Err(s) => return s,
    };

    if options.is_empty() {
        return ProgramStatus::FailureWithHelp;
    };

    verbose!("rtail {}", VERSION);

    if matches.opt_present("G") {
        print!("{}", options.iter().map(Options::to_string).collect::<Vec<_>>().join("\n"));
        return ProgramStatus::Success;
    }

    // handle q

    // find each service in rancher

    // setup connections

    // runloop

    ProgramStatus::Success
}
