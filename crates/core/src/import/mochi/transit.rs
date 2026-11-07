//! Transit JSON (https://github.com/cognitect/transit-format), which Mochi's `data.json` is written in: in its
//! compact form maps are `["^ ", k, v, ...]` arrays and repeated keys and keywords are replaced by `^N` cache codes;
//! in its verbose form maps are JSON objects.

use serde_json::Value as Json;

use super::value::{Value, parse_iso};
use crate::{Error, Result};

/// Cache codes are `^` and one or two base-44 digits from `0`, so the cache holds 44² strings, then starts over.
const CACHE_SIZE: usize = 44 * 44;

pub fn parse(text: &str) -> Result<Value> {
    let json: Json = serde_json::from_str(text).map_err(|e| Error::invalid(format!("data.json isn't JSON: {e}")))?;
    Ok(Reader::default().value(&json, false))
}

#[derive(Default)]
struct Reader {
    cache: Vec<String>,
}

/// A string is either a value or, as the first of a pair, the tag of a tagged value.
enum Read {
    Value(Value),
    Tag(String),
}

impl Reader {
    fn value(&mut self, json: &Json, key: bool) -> Value {
        match json {
            Json::Null => Value::Nil,
            Json::Bool(b) => Value::Bool(*b),
            Json::Number(n) => n.as_i64().map_or_else(|| Value::Float(n.as_f64().unwrap_or(0.0)), Value::Int),
            Json::String(s) => match self.string(s, key) {
                Read::Value(v) => v,
                Read::Tag(t) => Value::Str(format!("~#{t}")),
            },
            Json::Array(items) => self.array(items),
            Json::Object(map) => {
                if map.len() == 1
                    && let Some((k, v)) = map.iter().next()
                    && let Some(tag) = k.strip_prefix("~#")
                {
                    return self.tagged(tag, v);
                }
                Value::Map(map.iter().map(|(k, v)| (self.key(k), self.value(v, false))).collect())
            }
        }
    }

    fn array(&mut self, items: &[Json]) -> Value {
        match items {
            [Json::String(marker), rest @ ..] if marker == "^ " => Value::Map(
                rest.chunks(2)
                    .map(|pair| {
                        let k = match &pair[0] {
                            Json::String(s) => self.key(s),
                            other => self.value(other, true),
                        };
                        (k, pair.get(1).map_or(Value::Nil, |v| self.value(v, false)))
                    })
                    .collect(),
            ),
            // The first string must be read (and cached) exactly once, so it's read before knowing what it is.
            [Json::String(first), second] => match self.string(first, false) {
                Read::Tag(tag) => self.tagged(&tag, second),
                Read::Value(v) => Value::List(vec![v, self.value(second, false)]),
            },
            _ => Value::List(items.iter().map(|v| self.value(v, false)).collect()),
        }
    }

    fn key(&mut self, s: &str) -> Value {
        match self.string(s, true) {
            Read::Value(v) => v,
            Read::Tag(t) => Value::Str(t),
        }
    }

    fn tagged(&mut self, tag: &str, inner: &Json) -> Value {
        let value = self.value(inner, false);
        match tag {
            "set" | "list" | "'" => value,
            "cmap" => {
                let items = match value {
                    Value::List(items) => items,
                    _ => Vec::new(),
                };
                let mut items = items.into_iter();
                let mut entries = Vec::new();
                while let (Some(k), Some(v)) = (items.next(), items.next()) {
                    entries.push((k, v));
                }
                Value::Map(entries)
            }
            _ => Value::Tagged(tag.to_string(), Box::new(value)),
        }
    }

    fn string(&mut self, s: &str, key: bool) -> Read {
        if s.len() > 1 && s.starts_with('^') && !s.starts_with("^ ") {
            let raw = self.lookup(s);
            return decode(&raw);
        }
        if s.len() > 3 && (key || s.starts_with("~:") || s.starts_with("~$") || s.starts_with("~#")) {
            if self.cache.len() == CACHE_SIZE {
                self.cache.clear();
            }
            self.cache.push(s.to_string());
        }
        decode(s)
    }

    fn lookup(&self, code: &str) -> String {
        let digit = |b: u8| (b as usize).saturating_sub(48);
        let index = match code.as_bytes() {
            [_, a] => digit(*a),
            [_, a, b, ..] => digit(*a) * 44 + digit(*b),
            _ => usize::MAX,
        };
        self.cache.get(index).cloned().unwrap_or_default()
    }
}

fn decode(s: &str) -> Read {
    let Some(rest) = s.strip_prefix('~') else { return Read::Value(Value::Str(s.to_string())) };
    let Some(kind) = rest.chars().next() else { return Read::Value(Value::Str(s.to_string())) };
    let body = &rest[kind.len_utf8()..];
    Read::Value(match kind {
        // Escaped: a string that really starts with one of these.
        '~' | '^' | '`' => Value::Str(rest.to_string()),
        ':' => Value::Keyword(body.to_string()),
        '#' => return Read::Tag(body.to_string()),
        'm' => body.parse().map_or_else(|_| Value::Str(body.to_string()), Value::Date),
        // ISO 8601 by the spec; Mochi writes Unix ms here too.
        't' => match body.parse() {
            Ok(ms) => Value::Date(ms),
            Err(_) => parse_iso(body).map_or_else(|| Value::Str(body.to_string()), Value::Date),
        },
        'i' => body.parse().map_or_else(|_| Value::Str(body.to_string()), Value::Int),
        'd' | 'f' | 'n' => body.parse().map_or_else(|_| Value::Str(body.to_string()), Value::Float),
        '?' => Value::Bool(body == "t"),
        '_' => Value::Nil,
        // Symbols, UUIDs, URIs, bytes, chars, special numbers: their text is enough here.
        _ => Value::Str(body.to_string()),
    })
}
