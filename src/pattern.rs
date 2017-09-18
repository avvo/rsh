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

#[derive(Debug)]
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

impl Default for PatternList {
    fn default() -> PatternList {
        PatternList { patterns: vec![PatternListEntry::Positive(Pattern::default())] }
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
