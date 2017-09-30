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
        let mut current = 0;
        let mut consumed = current;
        let mut tokens = self.tokens.iter();
        let mut token = tokens.next();
        let chars = string.chars().collect::<Vec<_>>();
        while current < chars.len() {
            match token {
                Some(&Token::Char(c)) => {
                    if chars[current] == c {
                        current += 1;
                        consumed = current;
                        token = tokens.next();
                    } else if current > consumed {
                        current -= 1;
                    } else {
                        return false;
                    }
                }
                Some(&Token::Any) => {
                    current += 1;
                    consumed = current;
                    token = tokens.next();
                }
                Some(&Token::AnyRecurring) => {
                    current += 1;
                    if current == chars.len() && current > consumed {
                        current -= 1;
                        token = tokens.next();
                        if token.is_none() {
                            return true;
                        }
                    }
                }
                None => return false,
            }
        }
        token.is_none()
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
