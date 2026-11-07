//! What Mochi's data file holds once read, whichever of its two encodings it came in (Transit JSON or EDN).

/// An EDN-like value. Keywords and strings are told apart, but lookups by key accept either, so plain JSON reads too.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Keyword(String),
    /// Unix ms.
    Date(i64),
    /// Vectors, lists and sets.
    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
    /// A tag this reader doesn't know, around its value.
    Tagged(String, Box<Value>),
}

impl Value {
    /// The value at `key` in a map, where the key is the keyword or string `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries.iter().find(|(k, _)| k.name() == Some(key)).map(|(_, v)| v),
            Value::Tagged(_, v) => v.get(key),
            _ => None,
        }
    }

    /// A keyword's or a string's text: what ids and keys are.
    pub fn name(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Keyword(s) => Some(s),
            _ => None,
        }
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Tagged(_, v) => v.str(),
            _ => None,
        }
    }

    pub fn items(&self) -> &[Value] {
        match self {
            Value::List(items) => items,
            Value::Tagged(_, v) => v.items(),
            _ => &[],
        }
    }

    pub fn entries(&self) -> &[(Value, Value)] {
        match self {
            Value::Map(entries) => entries,
            Value::Tagged(_, v) => v.entries(),
            _ => &[],
        }
    }

    /// Present and not `nil` or `false` (Mochi marks a trashed deck or card with the date it was trashed).
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Float(n) => Some(*n),
            Value::Tagged(_, v) => v.number(),
            _ => None,
        }
    }

    /// A point in time as Unix ms: a date, an ISO 8601 string, or a number (seconds when it's too small to be ms).
    pub fn date(&self) -> Option<i64> {
        match self {
            Value::Date(ms) => Some(*ms),
            Value::Int(_) | Value::Float(_) => {
                let n = self.number()?;
                Some(if n.abs() < 100_000_000_000.0 { n * 1000.0 } else { n } as i64)
            }
            Value::Str(s) => parse_iso(s),
            Value::Tagged(_, v) => v.date(),
            _ => None,
        }
    }

    /// A field's value as text, for a template: booleans read as `"true"` or nothing.
    pub fn text(&self) -> String {
        match self {
            Value::Str(s) | Value::Keyword(s) => s.clone(),
            Value::Bool(true) => "true".to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => n.to_string(),
            Value::Tagged(_, v) => v.text(),
            _ => String::new(),
        }
    }
}

/// `2024-03-01T09:30:00Z`, with optional fraction and a `Z` or `±hh:mm` offset (a bare date is midnight UTC), as
/// Unix ms.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim();
    let num = |part: &str| part.parse::<i64>().ok();
    let (date, time) = s.split_once(['T', ' ']).unwrap_or((s, ""));
    let mut ymd = date.splitn(3, '-');
    let (year, month, day) = (num(ymd.next()?)?, num(ymd.next()?)?, num(ymd.next()?)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (clock, offset) = match time.find(['Z', 'z', '+', '-']) {
        Some(i) => (&time[..i], &time[i..]),
        None => (time, ""),
    };
    let mut ms = 0;
    if !clock.is_empty() {
        let (hms, fraction) = clock.split_once('.').unwrap_or((clock, ""));
        let mut parts = hms.split(':');
        let h = num(parts.next()?)?;
        let m = parts.next().map_or(Some(0), num)?;
        let sec = parts.next().map_or(Some(0), num)?;
        let frac: String = fraction.chars().chain("000".chars()).take(3).collect();
        ms = ((h * 60 + m) * 60 + sec) * 1000 + if fraction.is_empty() { 0 } else { num(&frac)? };
    }
    let offset_ms = match offset.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let (h, m) = offset[1..].split_once(':').unwrap_or((&offset[1..], "0"));
            let minutes = num(h)? * 60 + num(m)?;
            (if sign == '-' { -1 } else { 1 }) * minutes * 60_000
        }
        _ => 0,
    };
    Some(days_from_civil(year, month, day) * 86_400_000 + ms - offset_ms)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
