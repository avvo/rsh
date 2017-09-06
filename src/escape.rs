#[derive(Debug)]
pub enum Escape {
    DecreaseVerbosity,
    Help,
    IncreaseVerbosity,
    Invalid,
    Itself,
    Literal,
    Suspend,
    Terminate,
    None,
}

pub trait Scanner {
    fn next_escape(&mut self, buffer: &[u8; 4096], max: usize) -> Escape;
    fn reset(&mut self);
    fn pos(&self) -> usize;
    fn char(&self) -> char;
}

pub struct NullScanner;

impl NullScanner {
    pub fn new() -> NullScanner {
        NullScanner
    }
}

impl Scanner for NullScanner {
    fn next_escape(&mut self, _buffer: &[u8; 4096], _max: usize) -> Escape {
        Escape::None
    }

    fn reset(&mut self) {
        ()
    }

    fn pos(&self) -> usize {
        4095
    }

    fn char(&self) -> char {
        panic!("can't return char for NullScanner");
    }
}

#[derive(Debug)]
enum State {
    AwaitingNewline,
    AwaitingEscape,
    AwaitingChar,
}

#[derive(Debug)]
enum AnsiState {
    AwaitingEscape,
    AwaitingOpen,
    AwaitingChar,
}

pub struct CharScanner {
    pub c: char,
    pub pos: usize,
    state: State,
    ansi_state: AnsiState,
}

impl CharScanner {
    pub fn new(c: char) -> CharScanner {
        CharScanner {
            c: c,
            pos: 0,
            state: State::AwaitingEscape,
            ansi_state: AnsiState::AwaitingEscape,
        }
    }
}

impl Scanner for CharScanner {
    fn next_escape(&mut self, buffer: &[u8; 4096], max: usize) -> Escape {
        while self.pos < max {
            match self.ansi_state {
                AnsiState::AwaitingEscape => {
                    if buffer[self.pos] == 27 {
                        self.ansi_state = AnsiState::AwaitingOpen;
                        self.pos += 1;
                        continue;
                    }
                }
                AnsiState::AwaitingOpen => {
                    if buffer[self.pos] == b'[' {
                        self.ansi_state = AnsiState::AwaitingChar;
                        self.pos += 1;
                        continue;
                    }
                    self.ansi_state = AnsiState::AwaitingEscape;
                }
                AnsiState::AwaitingChar => {
                    let c = buffer[self.pos];
                    if (c >= b'0' && c <= b'9') || c == b';' {
                        self.pos += 1;
                        continue;
                    }
                    self.ansi_state = AnsiState::AwaitingEscape;
                    if c >= b'@' && c <= b'~' {
                        self.pos += 1;
                        continue;
                    }
                }
            }
            match self.state {
                State::AwaitingNewline => {
                    if buffer[self.pos] == b'\r' {
                        self.state = State::AwaitingEscape;
                    }
                    self.pos += 1;
                }
                State::AwaitingEscape => {
                    if buffer[self.pos] == self.c as u8 {
                        self.state = State::AwaitingChar;
                        self.pos += 1;
                        return Escape::Itself;
                    } else {
                        self.pos += 1;
                        self.state = State::AwaitingNewline;
                    }
                }
                State::AwaitingChar => {
                    self.state = State::AwaitingEscape;
                    let escape = match buffer[self.pos] {
                        b'V' => Escape::DecreaseVerbosity,
                        b'?' => Escape::Help,
                        b'v' => Escape::IncreaseVerbosity,
                        b'~' => {
                            self.state = State::AwaitingNewline;
                            Escape::Literal
                        }
                        26 => Escape::Suspend,
                        b'.' => Escape::Terminate,
                        _ => Escape::Invalid,
                    };
                    self.pos += 1;
                    return escape;
                }
            }
        }
        Escape::None
    }

    fn reset(&mut self) {
        self.pos = 0;
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn char(&self) -> char {
        self.c
    }
}

pub fn scanner(escape_char: Option<char>) -> Box<Scanner> {
    match escape_char {
        Some(c) => Box::new(CharScanner::new(c)),
        None => Box::new(NullScanner::new()),
    }
}
