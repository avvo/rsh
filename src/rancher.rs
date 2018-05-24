extern crate reqwest;
extern crate url;
extern crate serde;
extern crate url_serde;

use std;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum Error {
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
pub struct Container {
    name: String,
    pub actions: HashMap<String, url_serde::Serde<url::Url>>,
}

impl fmt::Display for Container {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        self.name.fmt(fmt)
    }
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
pub struct ApiKey {
    pub public_value: String,
    secret_value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerExec {
    attach_stdin: bool,
    attach_stdout: bool,
    command: Vec<String>,
    tty: bool,
}

impl ContainerExec {
    pub fn new(command: Vec<String>, tty: bool) -> ContainerExec {
        ContainerExec {
            attach_stdin: true,
            attach_stdout: true,
            command,
            tty,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct HostAccess {
    token: String,
    #[serde(with = "url_serde")]
    pub url: url::Url,
}

impl HostAccess {
    pub fn authed_url(&self) -> url::Url {
        let mut copy = self.url.clone();
        copy.query_pairs_mut().append_pair("token", &self.token);
        copy
    }
}

pub struct Client {
    http: reqwest::Client,
    pub api_key: Option<ApiKey>,
}

impl Client {
    pub fn new() -> Client {
        Client {
            http: reqwest::Client::new(),
            api_key: None,
        }
    }

    pub fn ldap_auth(&mut self, url: &url::Url, user: &str, password: &str) -> Result<(), Error> {
        let mut token_url = url.clone();
        token_url.set_path("/v2-beta/token");
        debug2!("POST {}", &token_url);
        let mut token_request = self.http.post(token_url);
        let code = format!("{}:{}", user, password);
        token_request.json(&TokenRequest {
            code,
            auth_provider: String::from("ldapconfig"),
        });
        let mut token_response = token_request.send()?;
        debug3!("{:?}", token_response);
        if !token_response.status().is_success() {
            return Err(Error::Empty);
        }
        let token: Token = token_response.json()?;

        let mut api_key_url = url.clone();
        api_key_url.set_path("/v2-beta/apikey");
        debug2!("POST {}", &api_key_url);
        let mut api_key_request = self.http.post(api_key_url);
        let mut cookie = reqwest::header::Cookie::new();
        cookie.set("token", token.jwt);
        api_key_request.header(cookie);
        api_key_request.json(&ApiKeyRequest {
            account_id: token.account_id,
            name: String::from("rsh"),
            description: String::from("Rancher SHell"),
        });
        let mut api_key_response = api_key_request.send()?;
        debug3!("{:?}", api_key_response);
        if !api_key_response.status().is_success() {
            return Err(Error::Empty);
        }
        let api_key = api_key_response.json()?;

        self.api_key = Some(api_key);
        Ok(())
    }

    pub fn executeable_containers(
        &self,
        url: &url::Url,
        environment: &str,
        stack: &str,
        service: &str,
    ) -> Result<Vec<Container>, Error> {
        self.filter_containers(url, environment, stack, service, |c| {
            c.actions.get("execute").is_some()
        })
    }

    pub fn logging_containers(
        &self,
        url: &url::Url,
        environment: &str,
        stack: &str,
        service: &str,
    ) -> Result<Vec<Container>, Error> {
        self.filter_containers(url, environment, stack, service, |c| {
            c.actions.get("logs").is_some()
        })
    }

    fn filter_containers(
        &self,
        url: &url::Url,
        environment: &str,
        stack: &str,
        service: &str,
        filter: impl Fn(&Container) -> bool,
    ) -> Result<Vec<Container>, Error> {
        let index = self.index(&url)?;
        let mut projects_link = index.links.get("projects").ok_or(Error::Empty)?.clone();
        // workaround edge case where Rancher doesn't show any projects
        projects_link.query_pairs_mut().append_pair("all", "true");
        debug!("Searching for environment {}", environment);
        let project = self.find_in_collection(
            &projects_link,
            |p: &Project| &p.name == environment,
        )?;
        debug!("Searching for stack {}", stack);
        let stacks_link = project.links.get("stacks").ok_or(Error::Empty)?;
        let stack = self.find_in_collection(
            stacks_link,
            |s: &Stack| s.name == stack,
        )?;
        debug!("Searching for service {}", service);
        let services_link = stack.links.get("services").ok_or(Error::Empty)?;
        let service = self.find_in_collection(
            services_link,
            |s: &Service| s.name == service,
        )?;
        debug!("Searching for executable container");
        let instances_link = service.links.get("instances").ok_or(Error::Empty)?;
        self.filter_collection(instances_link, filter)
    }

    fn index(&self, url: &url::Url) -> Result<Index, Error> {
        debug!("Connecting to {}", url);
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

    fn filter_collection<F, T>(&self, url: &url::Url, cond: F) -> Result<Vec<T>, Error>
    where
        F: Fn(&T) -> bool,
        T: serde::de::DeserializeOwned,
    {
        let collection: Result<Collection<_>, _> = self.get(url);
        match collection {
            Ok(v) => {
                let mut current: Vec<_> = v.data.into_iter().filter(&cond).collect();
                match v.pagination.and_then(|p| p.next) {
                    Some(u) => current.extend(self.filter_collection(&u, cond)?),
                    None => (),
                };
                Ok(current)
            }
            Err(Error::Empty) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        }
    }

    fn get<T>(&self, url: &url::Url) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned,
    {
        debug2!("GET {}", url);
        let mut request = self.http.get(url.clone());
        match self.api_key {
            Some(ref a) => {
                debug3!("Request Using Rancher API key {}", a.public_value);
                request.basic_auth(a.public_value.clone(), Some(a.secret_value.clone()));
            }
            _ => (),
        }
        let mut response = request.send()?;
        debug3!("{:?}", response);
        if response.status() == reqwest::StatusCode::Unauthorized {
            return Err(Error::Unauthorized);
        };

        Ok(response.json()?)
    }

    pub fn post<T, U>(&self, url: &url::Url, body: &T) -> Result<U, Error>
    where
        T: serde::Serialize,
        U: serde::de::DeserializeOwned,
    {
        debug2!("POST {}", url);
        let mut request = self.http.post(url.clone());
        request.json(body);
        match self.api_key {
            Some(ref a) => {
                debug3!("Request Using Rancher API key {}", a.public_value);
                request.basic_auth(a.public_value.clone(), Some(a.secret_value.clone()));
            }
            _ => (),
        }
        let mut response = request.send()?;
        debug3!("{:?}", response);
        if response.status() == reqwest::StatusCode::Unauthorized {
            return Err(Error::Unauthorized);
        };

        Ok(response.json()?)
    }
}

pub struct Manager {
    clients: HashMap<String, (Client, usize)>,
}

impl Manager {
    pub fn new() -> Manager {
        Manager { clients: HashMap::new() }
    }

    pub fn get(&mut self, host_port: String) -> &mut (Client, usize) {
        self.clients.entry(host_port).or_insert_with(|| (Client::new(), 0))
    }
}
