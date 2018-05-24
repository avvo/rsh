#[macro_use]
extern crate actix;
extern crate actix_web;
extern crate base64;
extern crate getopts;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate nom;
extern crate nix;
extern crate rpassword;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate shell_escape;
extern crate url;
extern crate users;

#[macro_use]
pub mod log;

pub mod cli;
pub mod config;
pub mod escape;
pub mod handler;
pub mod options;
pub mod pattern;
pub mod prompt;
pub mod rancher;

pub const NAME: &'static str = env!("CARGO_PKG_NAME");
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
