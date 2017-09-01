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
    pub actions: HashMap<String, url_serde::Serde<url::Url>>,
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
    public_value: String,
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
    url: url::Url,
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
            http: reqwest::Client::new().expect("failed to initalize http client"),
            api_key: None,
        }
    }

    pub fn ldap_auth(&mut self, url: &url::Url, user: &str, password: &str) -> Result<(), Error> {
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

    pub fn executeable_container(
        &self,
        url: &url::Url,
        environment: &Option<String>,
        stack: &str,
        service: &str,
    ) -> Result<Container, Error> {
        let index = self.index(&url)?;
        let projects_link = index.links.get("projects").ok_or(Error::Empty)?;
        let project = match environment {
            &Some(ref e) => {
                self.find_in_collection(
                    projects_link,
                    |p: &Project| &p.name == e,
                )?
            }
            &None => {
                let mut projects: Collection<Project> = self.get(&projects_link)?;
                if projects.data.len() == 1 {
                    projects.data.remove(0)
                } else {
                    return Err(Error::CouldNotDetermineEnvironment);
                }
            }
        };
        let stacks_link = project.links.get("stacks").ok_or(Error::Empty)?;
        let stack = self.find_in_collection(
            stacks_link,
            |s: &Stack| s.name == stack,
        )?;
        let services_link = stack.links.get("services").ok_or(Error::Empty)?;
        let service = self.find_in_collection(
            services_link,
            |s: &Service| s.name == service,
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

    pub fn post<T, U>(&self, url: &url::Url, body: &T) -> Result<U, Error>
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
