//! The contract's data types, plus IDs, tags and card sides.

use serde::{Deserialize, Serialize};

pub type Id = String;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deck {
    pub id: Id,
    pub name: String,
    pub parent_id: Option<Id>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub id: Id,
    pub deck_id: Id,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub id: Id,
    pub card_id: Id,
    /// The FSRS grade: 1 forgot, 3 remembered. Reviews from before answers were yes or no may be 2 or 4.
    pub rating: u8,
    pub reviewed_at: i64,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    New,
    Learning,
    Review,
    Relearning,
}

impl State {
    pub fn as_str(self) -> &'static str {
        self.srs().as_str()
    }

    pub fn parse(value: &str) -> Option<State> {
        match value {
            "new" => Some(State::New),
            "learning" => Some(State::Learning),
            "review" => Some(State::Review),
            "relearning" => Some(State::Relearning),
            _ => None,
        }
    }

    fn srs(self) -> furiten_srs::State {
        match self {
            State::New => furiten_srs::State::New,
            State::Learning => furiten_srs::State::Learning,
            State::Review => furiten_srs::State::Review,
            State::Relearning => furiten_srs::State::Relearning,
        }
    }
}

impl From<furiten_srs::State> for State {
    fn from(state: furiten_srs::State) -> State {
        match state {
            furiten_srs::State::New => State::New,
            furiten_srs::State::Learning => State::Learning,
            furiten_srs::State::Review => State::Review,
            furiten_srs::State::Relearning => State::Relearning,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub state: State,
    pub due: i64,
    pub stability: f64,
    pub difficulty: f64,
    pub reps: u32,
    pub lapses: u32,
    pub last_review: Option<i64>,
}

impl From<furiten_srs::Schedule> for Schedule {
    fn from(s: furiten_srs::Schedule) -> Schedule {
        Schedule {
            state: s.state.into(),
            due: s.due,
            stability: s.stability,
            difficulty: s.difficulty,
            reps: s.reps,
            lapses: s.lapses,
            last_review: s.last_review,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeckSummary {
    #[serde(flatten)]
    pub deck: Deck,
    pub cards: u64,
    pub due: u64,
    pub new: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CardView {
    #[serde(flatten)]
    pub card: Card,
    pub sides: Vec<String>,
    pub schedule: Schedule,
}

impl CardView {
    pub fn new(card: Card, schedule: Schedule) -> CardView {
        CardView { sides: sides(&card.content), card, schedule }
    }

    pub fn front(&self) -> &str {
        self.sides.first().map_or("", String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum Change {
    Deck(Deck),
    Card(Card),
    Review(Review),
}

impl Change {
    /// The entity's kind and id, which identify it across devices.
    pub fn key(&self) -> (&'static str, &str) {
        match self {
            Change::Deck(d) => ("deck", &d.id),
            Change::Card(c) => ("card", &c.id),
            Change::Review(r) => ("review", &r.id),
        }
    }
}

/// Splits Markdown into sides on lines that are exactly `---`. Blank lines around each side are dropped.
pub fn sides(content: &str) -> Vec<String> {
    let mut sides = vec![String::new()];
    for line in content.lines() {
        if line.trim_end_matches('\r') == "---" {
            sides.push(String::new());
            continue;
        }
        let side = sides.last_mut().expect("at least one side");
        side.push_str(line);
        side.push('\n');
    }
    sides.iter().map(|s| trim_blank_lines(s)).collect()
}

/// The content with these sides: each trimmed of blank lines and joined by `---` lines, the inverse of [`sides`].
/// Empty sides are dropped.
pub fn content_from_sides<S: AsRef<str>>(sides: &[S]) -> String {
    let kept: Vec<String> = sides.iter().map(|s| trim_blank_lines(s.as_ref())).filter(|s| !s.is_empty()).collect();
    kept.join("\n---\n")
}

fn trim_blank_lines(side: &str) -> String {
    let lines: Vec<&str> = side.lines().collect();
    let start = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|l| !l.trim().is_empty()).map_or(start, |i| i + 1);
    lines[start..end].iter().map(|l| l.trim_end_matches('\r')).collect::<Vec<_>>().join("\n")
}

/// Lowercase, trimmed, `#` stripped, deduplicated and sorted; empty tags are dropped.
pub fn normalize_tags<S: AsRef<str>>(tags: impl IntoIterator<Item = S>) -> Vec<String> {
    let mut out: Vec<String> = tags.into_iter().map(|t| normalize_tag(t.as_ref())).filter(|t| !t.is_empty()).collect();
    out.sort();
    out.dedup();
    out
}

pub fn normalize_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('#').trim().to_lowercase()
}

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A new ULID: 48 bits of Unix ms then 80 random bits, as 26 Crockford base32 characters.
pub fn ulid() -> Id {
    let mut bytes = [0u8; 10];
    if getrandom::getrandom(&mut bytes).is_err() {
        // No OS randomness: fall back to the hasher's per-process random keys.
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_i64(crate::now_ms());
        bytes[..8].copy_from_slice(&h.finish().to_le_bytes());
    }
    let random = bytes.iter().fold(0u128, |acc, b| (acc << 8) | *b as u128);
    // Monotonic within the process: in the same ms, the next id is the last one plus one, so ids sort in creation
    // order.
    static LAST: std::sync::Mutex<(i64, u128)> = std::sync::Mutex::new((0, 0));
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let now = crate::now_ms();
    *last = if now <= last.0 { (last.0, last.1 + 1) } else { (now, random >> 1) };
    ulid_from(last.0, last.1)
}

/// A ULID from explicit parts, for deterministic IDs (the sample deck) and tests.
pub fn ulid_from(ms: i64, random: u128) -> Id {
    let value = ((ms as u128 & 0xFFFF_FFFF_FFFF) << 80) | (random & ((1u128 << 80) - 1));
    (0..26).map(|i| CROCKFORD[((value >> (125 - i * 5)) & 31) as usize] as char).collect()
}

#[cfg(test)]
mod tests;
