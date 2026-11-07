//! EDN (https://github.com/edn-format/edn), which older Mochi exports' `data.edn` is written in.

use super::value::{Value, parse_iso};
use crate::{Error, Result};

/// Deeper than any Mochi export nests; a guard against a hostile file blowing the stack.
const MAX_DEPTH: usize = 128;

pub fn parse(text: &str) -> Result<Value> {
    let mut p = Parser { s: text, i: 0 };
    let value = p.value(0)?;
    p.skip()?;
    if p.i < p.s.len() {
        return Err(p.error("more after the end of the data"));
    }
    Ok(value)
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn error(&self, what: &str) -> Error {
        Error::invalid(format!("data.edn isn't valid EDN: {what} at byte {}", self.i))
    }

    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += c.len_utf8();
        Some(c)
    }

    /// Whitespace, commas, `;` comments and `#_` discarded forms.
    fn skip(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() || c == ',' => {
                    self.bump();
                }
                Some(';') => while self.bump().is_some_and(|c| c != '\n') {},
                Some('#') if self.s[self.i..].starts_with("#_") => {
                    self.i += 2;
                    self.value(0)?;
                }
                _ => return Ok(()),
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        self.skip()?;
        match self.peek().ok_or_else(|| self.error("unexpected end"))? {
            '{' => {
                self.bump();
                let items = self.items('}', depth)?;
                if items.len() % 2 != 0 {
                    return Err(self.error("a map with a key and no value"));
                }
                let mut items = items.into_iter();
                let mut entries = Vec::new();
                while let (Some(k), Some(v)) = (items.next(), items.next()) {
                    entries.push((k, v));
                }
                Ok(Value::Map(entries))
            }
            '[' => {
                self.bump();
                Ok(Value::List(self.items(']', depth)?))
            }
            '(' => {
                self.bump();
                Ok(Value::List(self.items(')', depth)?))
            }
            '"' => self.string(),
            ':' => {
                self.bump();
                Ok(Value::Keyword(self.token().to_string()))
            }
            '\\' => {
                self.bump();
                let c = self.bump().ok_or_else(|| self.error("a character with nothing after it"))?;
                let more = self.token();
                Ok(Value::Str(if more.is_empty() { c.to_string() } else { format!("{c}{more}") }))
            }
            '#' => {
                self.bump();
                if self.peek() == Some('{') {
                    self.bump();
                    return Ok(Value::List(self.items('}', depth)?));
                }
                let tag = self.token().to_string();
                let inner = self.value(depth + 1)?;
                Ok(match (tag.as_str(), &inner) {
                    ("inst", Value::Str(s)) => parse_iso(s).map_or(inner, Value::Date),
                    ("uuid", _) => inner,
                    _ => Value::Tagged(tag, Box::new(inner)),
                })
            }
            _ => {
                let token = self.token();
                if token.is_empty() {
                    return Err(self.error("unexpected character"));
                }
                Ok(atom(token))
            }
        }
    }

    fn items(&mut self, close: char, depth: usize) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        loop {
            self.skip()?;
            match self.peek() {
                Some(c) if c == close => {
                    self.bump();
                    return Ok(items);
                }
                Some(_) => items.push(self.value(depth + 1)?),
                None => return Err(self.error(&format!("missing {close}"))),
            }
        }
    }

    fn string(&mut self) -> Result<Value> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump().ok_or_else(|| self.error("a string that never ends"))? {
                '"' => return Ok(Value::Str(out)),
                '\\' => match self.bump().ok_or_else(|| self.error("a string that never ends"))? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'u' => {
                        let hex = self.s.get(self.i..self.i + 4).ok_or_else(|| self.error("a short \\u escape"))?;
                        let code = u32::from_str_radix(hex, 16).map_err(|_| self.error("a bad \\u escape"))?;
                        self.i += 4;
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                    }
                    c => out.push(c),
                },
                c => out.push(c),
            }
        }
    }

    /// A symbol, keyword or number: everything up to whitespace or a delimiter.
    fn token(&mut self) -> &str {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c.is_whitespace() || matches!(c, ',' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';') {
                break;
            }
            self.bump();
        }
        &self.s[start..self.i]
    }
}

fn atom(token: &str) -> Value {
    match token {
        "nil" => return Value::Nil,
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    let starts_numeric = token.trim_start_matches(['-', '+']).starts_with(|c: char| c.is_ascii_digit());
    if starts_numeric {
        let digits = token.trim_end_matches(['N', 'M']);
        if let Ok(n) = digits.parse::<i64>() {
            return Value::Int(n);
        }
        if let Ok(n) = digits.parse::<f64>() {
            return Value::Float(n);
        }
    }
    Value::Str(token.to_string())
}
