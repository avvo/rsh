use std;

use getopts;
use rpassword;
use serde_json;
use shell_escape;
use url;
use users;

use config::{self, Config};
use log;
use prompt::prompt_with_default;
use rancher;
use options::{self, Options};

use std::io::{Read, Write};

pub enum ProgramStatus {
    Success,
    SuccessWithHelp,
    Failure,
    FailureWithHelp,
}

impl ProgramStatus {
    pub fn exit(self, opts: getopts::Options, program: &str, final_arg: &str) {
        match self {
            ProgramStatus::Success => (),
            ProgramStatus::SuccessWithHelp => {
                print!(
                    "{}",
                    opts.usage(&format!(
                        "Usage: {} [opts] [protocol://][user@]host[:port][[/env]/stack]/service {}",
                        program,
                        final_arg,
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
}

pub fn set_log_level(matches: &getopts::Matches) {
    match matches.opt_count("v") {
        0 => (),
        1 => log::set_level(options::LogLevel::Debug),
        2 => log::set_level(options::LogLevel::Debug2),
        _ => log::set_level(options::LogLevel::Debug3),
    };
}

pub fn set_log_file(matches: &getopts::Matches) -> Result<(), ()> {
    match matches.opt_str("E") {
        Some(ref path) => {
            let result = std::fs::OpenOptions::new().create(true).append(true).open(
                path,
            );
            match result {
                Ok(file) => log::set_device(file),
                Err(e) => {
                    fatal!("Couldn't open logfile {}: {}", path, e);
                    return Err(());
                }
            };

            let file = std::fs::File::create(path).unwrap();
            log::set_device(file);
        }
        None => (),
    };
    Ok(())
}

pub fn is_early_exit(matches: &getopts::Matches, name: &str) -> Option<ProgramStatus> {
    if matches.opt_present("version") {
        println!("{} {}", name, super::VERSION);
        Some(ProgramStatus::Success)
    } else if matches.opt_present("help") {
        Some(ProgramStatus::SuccessWithHelp)
    } else {
        None
    }
}

pub fn config(matches: &getopts::Matches, config_opt: &str) -> Result<Config, ()> {
    std::fs::create_dir_all(config::user_config_dir()).expect("couldn't create config dir");

    let config_paths = match matches.opt_str(config_opt).map(std::path::PathBuf::from) {
        Some(val) => vec![val],
        None => vec![config::user_config_path(), config::system_config_path()],
    };

    let mut cfg = match config::Config::try_from(
        matches.opt_strs("o").iter().map(AsRef::as_ref).collect(),
    ) {
        Ok(v) => v,
        Err(config::Error::OptionError(key, value)) => {
            fatal!("Bad configuration option: \"{}\" for {}.", value, key);
            return Err(());
        }
        Err(config::Error::UnknownOption(key)) => {
            fatal!("Bad configuration option: {}.", key);
            return Err(());
        }
        Err(config::Error::OptionNotAllowed(key)) => {
            fatal!("{} directive not supported as a command-line option.", key);
            return Err(());
        }
        _ => {
            fatal!("Error configuration option.");
            return Err(());
        }
    };
    for path in config_paths {
        debug!("Reading configuration data {}", path.to_string_lossy());
        match config::open_config(&path) {
            Ok(v) => cfg = cfg.append(v),
            Err(config::Error::OptionError(key, value)) => {
                fatal!(
                    "{}: Bad configuration option: \"{}\" for {}.",
                    path.to_string_lossy(),
                    value,
                    key
                );
                return Err(());
            }
            Err(config::Error::UnknownOption(key)) => {
                fatal!(
                    "{}: Bad configuration option: {}.",
                    path.to_string_lossy(),
                    key
                );
                return Err(());
            }
            Err(config::Error::IoError(_)) => {
                fatal!("{}: Error reading config.", path.to_string_lossy());
                return Err(());
            }
            _ => {
                fatal!("{}: Error parsing config.", path.to_string_lossy());
                return Err(());
            }
        }
    }
    Ok(cfg)
}

pub fn options_for_host(matches: &getopts::Matches, config: &Config, host: &str) -> Result<Options, ProgramStatus> {
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
            return Err(ProgramStatus::FailureWithHelp);
        }
    };

    if url.cannot_be_a_base() {
        verbose!("Error parsing host, non-base URL.");
        return Err(ProgramStatus::FailureWithHelp);
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
            return Err(ProgramStatus::FailureWithHelp);
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
            return Err(ProgramStatus::FailureWithHelp);
        }
    });

    if !url.username().is_empty() {
        option_builder.user(url.username().into());
    } else if !matches.opt_defined("l") {
        // skip
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
                return Err(ProgramStatus::Failure);
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

    if !matches.opt_defined("e") {
        // skip
    } else if let Some(escape_str) = matches.opt_str("e") {
        if escape_str != "none" {
            match escape_str.parse::<char>() {
                Ok(v) if v.is_ascii() => option_builder.escape_char(v),
                _ => {
                    eprintln!("Bad escape character '{}'.", escape_str);
                    return Err(ProgramStatus::Failure);
                }
            };
        }
    } else {
        option_builder.escape_char(config.escape_char(&host).unwrap_or('~'));
    }

    if !matches.opt_defined("t") || !matches.opt_defined("T") {
        // skip
    } else if matches.opt_count("t") > 1 {
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
            return Err(ProgramStatus::FailureWithHelp);
        }
        Err(options::BuildError::MissingHostName) => {
            verbose!("Missing host name.");
            return Err(ProgramStatus::FailureWithHelp);
        }
        Err(options::BuildError::MissingService) => {
            verbose!("Missing service.");
            return Err(ProgramStatus::FailureWithHelp);
        }
        Err(options::BuildError::MissingStack) => {
            verbose!("Missing stack.");
            return Err(ProgramStatus::FailureWithHelp);
        }
        Err(options::BuildError::UnknownToken(c)) => {
            fatal!("Unknown token %{}.", c);
            return Err(ProgramStatus::Failure);
        }
    };

    Ok(options)
}

pub fn get_containers(manager: &mut rancher::Manager, options: &options::Options, filter: impl Clone + Fn(&rancher::Container) -> bool) -> Result<Vec<rancher::Container>, ProgramStatus> {
    let (ref mut client, ref mut tries) = manager.get(options.host_with_port().to_owned());

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

    let url = options.url();
    let containers = loop {
        match client.filter_containers(
            &url,
            &options.environment,
            &options.stack,
            &options.service,
            filter.clone(),
        ) {
            Ok(v) => break v,
            Err(rancher::Error::Unauthorized) if *tries == 0 => {
                debug2!("Received Unauthorized, attempting authentication");
                let user = prompt_with_default("Rancher User", users::get_current_username())
                    .expect("couldn't get user");
                let password = rpassword::prompt_password_stdout(&"Rancher Password: ")
                    .expect("couldn't get password");
                match client.ldap_auth(&url, &user, &password) {
                    Ok(_) => (),
                    Err(_) => {
                        fatal!("Authentication failed.");
                        return Err(ProgramStatus::Failure);
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
                return Err(ProgramStatus::Failure);
            }
        };
        // need to give rancher a tiny bit of time to activate the api keys
        std::thread::sleep(std::time::Duration::from_millis(250));
        *tries += 1;
    };

    Ok(containers)
}
