extern crate url;

use std;
use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    NoHost,
    NonBase,
    NoStack,
    TooManyPathSegments,
    UrlParseError(url::ParseError),
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Error {
        Error::UrlParseError(err)
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            Error::NoHost => "no hostname provided",
            Error::NonBase => "non base URL",
            Error::NoStack => "no stack provided",
            Error::TooManyPathSegments => "too many path segments",
            Error::UrlParseError(ref err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match *self {
            Error::UrlParseError(ref err) => Some(err as &StdError),
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
pub struct Ucl {
    pub url: url::Url,
    pub environment: Option<String>,
    pub stack: String,
    pub service: String,
}

impl Ucl {
    pub fn from(arg: &str) -> Result<Ucl, Error> {
        let mut url = if !arg.contains("://") {
            url::Url::parse(&format!("https://{}", arg))?
        } else {
            url::Url::parse(arg)?
        };

        if url.cannot_be_a_base() {
            return Err(Error::NonBase);
        };

        let (environment, stack, service) = {
            let mut path_segments = url.path_segments()
                .expect("cannot-be-a-base URL bypassed check?")
                .map(String::from);
            let first = path_segments.next();
            let second = path_segments.next();
            let third = path_segments.next();

            if path_segments.next().is_some() {
                return Err(Error::TooManyPathSegments);
            };

            match (first, second, third) {
                (None, None, None) => return Err(Error::NoStack),
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
