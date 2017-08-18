extern crate base64;
extern crate futures;
extern crate getopts;
extern crate reqwest;
extern crate rpassword;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate shell_escape;
extern crate termion;
extern crate tokio_core;
extern crate url;
extern crate url_serde;
extern crate websocket;

use std::collections::HashMap;
use std::error::Error as StdError;

use futures::future::Future;
use futures::sink::Sink;
use futures::stream::Stream;

use std::fmt;
use std::io::Read;
use std::io::Write;
use termion::raw::IntoRawMode;

const NAME: &'static str = env!("CARGO_PKG_NAME");
const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
enum UclError {
    NoHost,
    NonBase,
    NoStack,
    TooManyPathSegments,
    UrlParseError(url::ParseError),
}

impl From<url::ParseError> for UclError {
    fn from(err: url::ParseError) -> UclError {
        UclError::UrlParseError(err)
    }
}

impl StdError for UclError {
    fn description(&self) -> &str {
        match *self {
            UclError::NoHost => "no hostname provided",
            UclError::NonBase => "non base URL",
            UclError::NoStack => "no stack provided",
            UclError::TooManyPathSegments => "too many path segments",
            UclError::UrlParseError(ref err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match *self {
            UclError::UrlParseError(ref err) => Some(err as &StdError),
            _ => None,
        }
    }
}

impl fmt::Display for UclError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        self.description().fmt(fmt)
    }
}

#[derive(Debug)]
enum Error {
    CouldNotDetermineEnvironment,
    Empty,
    HttpError(reqwest::Error),
    Unauthorized,
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Error {
        Error::HttpError(err)
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            Error::CouldNotDetermineEnvironment => "could not determine environment",
            Error::Empty => "empty",
            Error::HttpError(ref err) => err.description(),
            Error::Unauthorized => "unauthorized",
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match *self {
            Error::HttpError(ref err) => Some(err as &StdError),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        self.description().fmt(fmt)
    }
}

// Uniform Container Location
#[derive(Debug)]
struct Ucl {
    url: url::Url,
    environment: Option<String>,
    stack: String,
    service: String,
}

impl Ucl {
    fn from(arg: &str) -> Result<Ucl, UclError> {
        let mut url = if !arg.contains("://") {
            url::Url::parse(&format!("https://{}", arg))?
        } else {
            url::Url::parse(arg)?
        };

        if url.cannot_be_a_base() {
            return Err(UclError::NonBase);
        };

        let (environment, stack, service) = {
            let mut path_segments = url.path_segments()
                .expect("cannot-be-a-base URL bypassed check?")
                .map(String::from);
            let first = path_segments.next();
            let second = path_segments.next();
            let third = path_segments.next();

            if path_segments.next().is_some() {
                return Err(UclError::TooManyPathSegments);
            };

            match (first, second, third) {
                (None, None, None) => return Err(UclError::NoStack),
                (Some(a), None, None) => (None, a.clone(), a),
                (Some(a), Some(b), None) => (None, a, b),
                (a @ Some(_), Some(b), Some(c)) => (a, b, c),
                _ => panic!("didn't expect a path segment to follow None"),
            }
        };

        url.set_path("");
        Ok(Ucl {
            url,
            environment,
            stack,
            service,
        })
    }
}

impl fmt::Display for Ucl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.environment {
            Some(ref environment) => {
                write!(
                    f,
                    "{}{}/{}/{}",
                    self.url,
                    environment,
                    self.stack,
                    self.service
                )
            }
            None => write!(f, "{}{}/{}", self.url, self.stack, self.service),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Collection<T> {
    data: Vec<T>,
    pagination: Option<Pagination>,
}

#[derive(Debug, Deserialize)]
struct Pagination {
    // #[serde(with = "url_serde")]
    // first: Option<url::Url>,
    // #[serde(with = "url_serde")]
    // previous: Option<url::Url>,
    #[serde(with = "url_serde")]
    next: Option<url::Url>,
    // #[serde(with = "url_serde")]
    // last: Option<url::Url>,
    // limit: u64,
    // total: Option<u64>,
    // partial: bool,
}

#[derive(Debug, Deserialize)]
struct Index {
    links: HashMap<String, url_serde::Serde<url::Url>>,
}

#[derive(Debug, Deserialize)]
struct Project {
    name: String,
    links: HashMap<String, url_serde::Serde<url::Url>>,
}

#[derive(Debug, Deserialize)]
struct Stack {
    name: String,
    links: HashMap<String, url_serde::Serde<url::Url>>,
}

#[derive(Debug, Deserialize)]
struct Service {
    name: String,
    links: HashMap<String, url_serde::Serde<url::Url>>,
}

#[derive(Debug, Deserialize)]
struct Container {
    actions: HashMap<String, url_serde::Serde<url::Url>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenRequest {
    code: String,
    auth_provider: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Token {
    account_id: String,
    jwt: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyRequest {
    account_id: String,
    name: String,
    description: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiKey {
    public_value: String,
    secret_value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContainerExec {
    attach_stdin: bool,
    attach_stdout: bool,
    command: Vec<String>,
    tty: bool,
}

impl ContainerExec {
    fn new(command: Vec<String>, tty: bool) -> ContainerExec {
        ContainerExec {
            attach_stdin: true,
            attach_stdout: true,
            command,
            tty,
        }
    }
}

#[derive(Debug, Deserialize)]
struct HostAccess {
    token: String,
    #[serde(with = "url_serde")]
    url: url::Url,
}

impl HostAccess {
    fn authed_url(&self) -> url::Url {
        let mut copy = self.url.clone();
        copy.query_pairs_mut().append_pair("token", &self.token);
        copy
    }
}

struct RancherClient {
    http: reqwest::Client,
    api_key: Option<ApiKey>,
}

impl RancherClient {
    fn new() -> RancherClient {
        RancherClient {
            http: reqwest::Client::new().expect("failed to initalize http client"),
            api_key: None,
        }
    }

    fn ldap_auth(&mut self, url: &url::Url, user: &str, password: &str) -> Result<(), Error> {
        let mut token_url = url.clone();
        token_url.set_path("/v2-beta/token");
        let mut token_request = self.http.post(token_url)?;
        let code = format!("{}:{}", user, password);
        token_request.json(&TokenRequest {
            code,
            auth_provider: String::from("ldapconfig"),
        })?;
        let mut token_response = token_request.send()?;
        if !token_response.status().is_success() {
            return Err(Error::Empty);
        }
        let token: Token = token_response.json()?;

        let mut api_key_url = url.clone();
        api_key_url.set_path("/v2-beta/apikey");
        let mut api_key_request = self.http.post(api_key_url)?;
        let mut cookie = reqwest::header::Cookie::new();
        cookie.set("token", token.jwt);
        api_key_request.header(cookie);
        api_key_request.json(&ApiKeyRequest {
            account_id: token.account_id,
            name: String::from("rsh"),
            description: String::from("Rancher SHell"),
        })?;
        let mut api_key_response = api_key_request.send()?;
        if !api_key_response.status().is_success() {
            return Err(Error::Empty);
        }
        let api_key = api_key_response.json()?;

        self.api_key = Some(api_key);
        Ok(())
    }

    fn executeable_container(&self, ucl: &mut Ucl) -> Result<Container, Error> {
        let index = self.index(&ucl.url)?;
        let projects_link = index.links.get("projects").ok_or(Error::Empty)?;
        let project = match ucl.environment {
            Some(ref e) => {
                self.find_in_collection(
                    projects_link,
                    |p: &Project| &p.name == e,
                )?
            }
            None => {
                let mut projects: Collection<Project> = self.get(&projects_link)?;
                if projects.data.len() == 1 {
                    let environment = projects.data.remove(0);
                    ucl.environment = Some(environment.name.clone());
                    environment
                } else {
                    return Err(Error::CouldNotDetermineEnvironment);
                }
            }
        };
        let stacks_link = project.links.get("stacks").ok_or(Error::Empty)?;
        let stack = self.find_in_collection(
            stacks_link,
            |s: &Stack| s.name == ucl.stack,
        )?;
        let services_link = stack.links.get("services").ok_or(Error::Empty)?;
        let service = self.find_in_collection(
            services_link,
            |s: &Service| s.name == ucl.service,
        )?;
        let instances_link = service.links.get("instances").ok_or(Error::Empty)?;
        self.find_in_collection(instances_link, |c: &Container| {
            c.actions.get("execute").is_some()
        })
    }

    fn index(&self, url: &url::Url) -> Result<Index, Error> {
        let mut copy = url.clone();
        copy.set_path("/v2-beta");
        self.get(&copy)
    }

    fn find_in_collection<F, T>(&self, url: &url::Url, cond: F) -> Result<T, Error>
    where
        F: Fn(&T) -> bool,
        T: serde::de::DeserializeOwned,
    {
        let mut collection: Collection<T> = self.get(url)?;
        match collection.data.iter().position(|x| cond(x)) {
            None => {
                match collection.pagination.and_then(|p| p.next) {
                    Some(ref v) => self.find_in_collection(v, cond),
                    None => Err(Error::Empty),
                }
            }
            Some(i) => Ok(collection.data.remove(i)),
        }
    }

    fn get<T>(&self, url: &url::Url) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut request = self.http.get(url.clone())?;
        match self.api_key {
            Some(ref a) => {
                request.basic_auth(a.public_value.clone(), Some(a.secret_value.clone()));
            }
            _ => (),
        }
        let mut response = request.send()?;
        if response.status() == reqwest::StatusCode::Unauthorized {
            return Err(Error::Unauthorized);
        };

        Ok(response.json()?)
    }

    fn post<T, U>(&self, url: &url::Url, body: &T) -> Result<U, Error>
    where
        T: serde::Serialize,
        U: serde::de::DeserializeOwned,
    {
        let mut request = self.http.post(url.clone())?;
        request.json(body)?;
        match self.api_key {
            Some(ref a) => {
                request.basic_auth(a.public_value.clone(), Some(a.secret_value.clone()));
            }
            _ => (),
        }
        let mut response = request.send()?;
        if response.status() == reqwest::StatusCode::Unauthorized {
            return Err(Error::Unauthorized);
        };

        Ok(response.json()?)
    }
}

struct AndSelect<S1, S2> {
    stream1: futures::stream::Fuse<S1>,
    stream2: futures::stream::Fuse<S2>,
    flag: bool,
}

fn and_select<S1, S2>(stream1: S1, stream2: S2) -> AndSelect<S1, S2>
where
    S1: futures::stream::Stream,
    S2: futures::stream::Stream<Item = S1::Item, Error = S1::Error>,
{
    AndSelect {
        stream1: stream1.fuse(),
        stream2: stream2.fuse(),
        flag: false,
    }
}

impl<S1, S2> futures::stream::Stream for AndSelect<S1, S2>
where
    S1: futures::stream::Stream,
    S2: futures::stream::Stream<
        Item = S1::Item,
        Error = S1::Error,
    >,
{
    type Item = S1::Item;
    type Error = S1::Error;

    fn poll(&mut self) -> futures::Poll<Option<S1::Item>, S1::Error> {
        let (a, b) = if self.flag {
            (
                &mut self.stream2 as &mut futures::stream::Stream<Item = _, Error = _>,
                &mut self.stream1 as &mut futures::stream::Stream<Item = _, Error = _>,
            )
        } else {
            (
                &mut self.stream1 as &mut futures::stream::Stream<Item = _, Error = _>,
                &mut self.stream2 as &mut futures::stream::Stream<Item = _, Error = _>,
            )
        };

        match a.poll()? {
            futures::Async::Ready(Some(item)) => {
                self.flag = !self.flag;
                return Ok(Some(item).into());
            }
            futures::Async::Ready(None) => return Ok(None.into()),
            futures::Async::NotReady => false,
        };

        match b.poll()? {
            futures::Async::Ready(Some(item)) => Ok(Some(item).into()),
            futures::Async::Ready(None) => Ok(None.into()),
            futures::Async::NotReady => Ok(futures::Async::NotReady),
        }
    }
}

fn prompt_with_default(prompt: &str, default: Option<String>) -> std::io::Result<String> {
    let mut stdout = std::io::stdout();
    let mut result = String::new();
    let prompt = match default {
        Some(ref v) => format!("{} ({}): ", prompt, v),
        None => format!("{}: ", prompt),
    };
    write!(stdout, "{}", prompt)?;
    stdout.flush()?;
    std::io::stdin().read_line(&mut result)?;
    if result.chars().last() == Some('\n') {
        result.pop();
    }
    if result.chars().last() == Some('\r') {
        result.pop();
    }
    match (result.as_ref(), default) {
        ("", Some(v)) => Ok(v),
        _ => Ok(result),
    }
}

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

    let mut ucl = match matches.free.get(0).ok_or(UclError::NoHost).and_then(
        |s| Ucl::from(s),
    ) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("{}", opts.usage(&brief));
            std::process::exit(1);
        }
    };

    let mut client = RancherClient::new();

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
            Err(Error::Unauthorized) if tries == 0 => {
                let user = prompt_with_default("Rancher User", std::env::var("USER").ok())
                    .expect("couldn't get user");
                let password = rpassword::prompt_password_stdout(&"Rancher Password: ").expect(
                    "couldn't get password",
                );
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
            Err(Error::Empty) => {
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
        (format!("login -f {}", user), Some(format!("\nConnection to {} closed.", ucl)))
    } else if matches.free.len() == 2 {
        (matches.free.get(1).unwrap().clone(), None)
    } else {
        let vec: Vec<_> = matches.free[1..].iter().map(|s| shell_escape::escape(s.clone().into())).collect();
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
    let is_tty = (termion::is_tty(&std::fs::File::create("/dev/stdout").unwrap()) || matches.opt_present("t")) && !matches.opt_present("T");
    if is_tty {
        match termion::terminal_size() {
            Ok((cols, rows)) => command_parts.push(format!("stty cols {} rows {}", cols, rows)),
            Err(_) => (),
        };
        command_parts.push(format!("([ -x /usr/bin/script ] && /usr/bin/script -q -c {} /dev/null || exec {})", shell_escape::escape(command.clone().into()), command));
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
                and_select(
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
        None => ()
    }
}
