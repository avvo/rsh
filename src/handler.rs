use std;
use std::io::{Read, Write};

use actix::{self, Actor, StreamHandler};
use actix_web::ws;
use base64;
use nix;

use escape;
use log;

#[derive(Message)]
struct Input(Vec<u8>);

pub struct ReadWrite {
    stdout: std::io::Stdout,
    writer: ws::ClientWriter,
}

impl ReadWrite {
    pub fn start(escape_char: Option<char>, reader: ws::ClientReader, writer: ws::ClientWriter) {
        let addr: actix::Addr<actix::Syn, _> = ReadWrite::create(|ctx| {
            ReadWrite::add_stream(reader, ctx);
            let stdout = std::io::stdout();
            ReadWrite { stdout, writer }
        });
        start_reader(escape_char, addr);
    }
}

impl actix::Actor for ReadWrite {
    type Context = actix::Context<Self>;
}

impl actix::Handler<Input> for ReadWrite {
    type Result = ();

    fn handle(&mut self, Input(bytes): Input, _ctx: &mut actix::Context<Self>) {
        self.writer.text(base64::encode(&bytes));
    }
}

impl actix::StreamHandler<ws::Message, ws::ProtocolError> for ReadWrite {
    fn handle(&mut self, msg: ws::Message, _ctx: &mut actix::Context<Self>) {
        match msg {
            ws::Message::Text(txt) => {
                self.stdout
                    .write(&base64::decode(&txt).expect("invalid base64"))
                    .unwrap();
                self.stdout.flush().unwrap();
            }
            ws::Message::Binary(_) => panic!("unexpected binary message"),
            ws::Message::Ping(txt) => self.writer.pong(&txt),
            ws::Message::Pong(txt) => debug3!("recieved pong {:?}", txt),
            ws::Message::Close(reason) => {
                actix::Arbiter::system().do_send(actix::msgs::SystemExit(0));
                debug3!("closing {:?}", reason)
            }
        }
    }
}

fn start_reader(escape_char: Option<char>, addr: actix::Addr<actix::Syn, ReadWrite>) {
    std::thread::spawn(move || {
        let mut escape_scanner = escape::scanner(escape_char);
        let mut stdin = std::io::stdin();
        let mut buffer = [0; 4096];
        let mut vbuffer = std::vec::Vec::new();
        'main: loop {
            escape_scanner.reset();
            let mut sent = 0;
            let read = stdin.read(&mut buffer[..]).unwrap();
            while sent < read {
                let escape_type = escape_scanner.next_escape(&buffer, read);
                let bytes = match escape_type {
                    escape::Escape::DecreaseVerbosity
                    | escape::Escape::Help
                    | escape::Escape::IncreaseVerbosity
                    | escape::Escape::Itself
                    | escape::Escape::Suspend
                    | escape::Escape::Terminate => &buffer[sent..escape_scanner.pos() - 1],
                    escape::Escape::Invalid => {
                        vbuffer.clear();
                        vbuffer.push(escape_scanner.char() as u8);
                        vbuffer.extend(&buffer[sent..(escape_scanner.pos())]);
                        &vbuffer[..]
                    }
                    escape::Escape::Literal | escape::Escape::None => {
                        &buffer[sent..(escape_scanner.pos())]
                    }
                };
                let message = Input(bytes.to_owned());
                addr.do_send(message);
                sent = escape_scanner.pos();
                match escape_type {
                    escape::Escape::DecreaseVerbosity => {
                        let level = log::decrease_level();
                        println!("{}V [LogLevel {}]\r", escape_scanner.char(), level);
                    }
                    escape::Escape::Help => {
                        println!(
                            "{0}?\r
Supported escape sequences:\r
{0}.   - terminate connection\r
{0}V/v - decrease/increase verbosity (LogLevel)\r
{0}^Z  - suspend rsh\r
{0}?   - this message\r
{0}{0}   - send the escape character by typing it twice\r
(Note that escapes are only recognized immediately after newline.)\r",
                            escape_scanner.char()
                        );
                    }
                    escape::Escape::IncreaseVerbosity => {
                        let level = log::increase_level();
                        println!("{}v [LogLevel {}]\r", escape_scanner.char(), level);
                    }
                    escape::Escape::Suspend => {
                        nix::sys::signal::kill(
                            nix::unistd::getpid(),
                            Some(nix::sys::signal::Signal::SIGTSTP),
                        ).expect("failed to suspend");
                    }
                    escape::Escape::Terminate => {
                        break 'main;
                    }
                    _ => (),
                }
            }
        }
    });
}
