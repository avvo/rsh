use std;
use std::fmt;
use std::str::FromStr;

#[derive(Debug)]
pub enum Error {
    EmptyPattern,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Token {
    Any,
    AnyRecurring,
    Char(char),
}

impl fmt::Display for Token {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        match *self {
            Token::Any => "?".fmt(fmt),
            Token::AnyRecurring => "*".fmt(fmt),
            Token::Char(c) => c.fmt(fmt),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pattern {
    tokens: Vec<Token>,
}

impl Pattern {
    pub fn matches(&self, string: &str) -> bool {
        self.do_match(&string.chars().collect::<Vec<char>>(), 0, 0)
    }

    fn do_match(&self, chars: &[char], current_char: usize, current_token: usize) -> bool {
        if current_char >= chars.len() {
            return match self.tokens.get(current_token) {
                None => true,
                Some(&Token::AnyRecurring) => self.tokens.get(current_token + 1).is_none(),
                _ => false,
            };
        }
        match self.tokens.get(current_token) {
            Some(&Token::Char(c)) => {
                if chars[current_char] == c {
                    self.do_match(chars, current_char + 1, current_token + 1)
                } else {
                    false
                }
            }
            Some(&Token::Any) => self.do_match(chars, current_char + 1, current_token + 1),
            Some(&Token::AnyRecurring) => {
                if self.do_match(chars, current_char + 1, current_token) {
                    true
                } else {
                    self.do_match(chars, current_char, current_token + 1)
                }
            }
            None => false,
        }
    }
}

impl Default for Pattern {
    fn default() -> Pattern {
        Pattern { tokens: vec![Token::AnyRecurring] }
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> fmt::Result {
        for token in &self.tokens {
            token.fmt(fmt)?;
        }
        Ok(())
    }
}

impl FromStr for Pattern {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut ts = Vec::new();
        for c in s.chars() {
            let token = match c {
                '?' => Token::Any,
                '*' => Token::AnyRecurring,
                _ => Token::Char(c),
            };
            ts.push(token);
            let len = ts.len();
            if len > 1 && ts[len - 2] == Token::AnyRecurring && ts[len - 1] == Token::Any {
                let last = ts[len - 1];
                ts[len - 1] = ts[len - 2];
                ts[len - 2] = last;
            }
        }
        if ts.is_empty() {
            Err(Error::EmptyPattern)
        } else {
            Ok(Pattern { tokens: ts })
        }
    }
}

#[derive(Debug)]
enum PatternListEntry {
    Positive(Pattern),
    Negative(Pattern),
}

#[derive(Debug)]
pub struct PatternList {
    patterns: Vec<PatternListEntry>,
}

impl PatternList {
    pub fn matches(&self, string: &str) -> bool {
        for entry in self.patterns.iter() {
            match entry {
                &PatternListEntry::Positive(ref p) if p.matches(string) => return true,
                &PatternListEntry::Negative(ref p) if p.matches(string) => return false,
                _ => (),
            };
        }
        false
    }
}

impl Default for PatternList {
    fn default() -> PatternList {
        PatternList { patterns: vec![PatternListEntry::Positive(Pattern::default())] }
    }
}

impl From<Vec<Pattern>> for PatternList {
    fn from(source: Vec<Pattern>) -> PatternList {
        let mut patterns = Vec::new();
        for p in source {
            patterns.push(PatternListEntry::Positive(p))
        }
        PatternList { patterns }
    }
}

impl FromStr for PatternList {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut patterns = Vec::new();
        for substr in s.split(|c| c == ',' || c == ' ') {
            let substr = substr.trim();
            if substr.starts_with("!") {
                patterns.push(PatternListEntry::Negative(substr[1..].parse()?))
            } else {
                patterns.push(PatternListEntry::Positive(substr.parse()?))
            }
        }
        if patterns.is_empty() {
            Err(Error::EmptyPattern)
        } else {
            Ok(PatternList { patterns })
        }
    }
}
