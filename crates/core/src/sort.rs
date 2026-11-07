//! The orders `cards.list` can return cards in (see Card order in `docs/contract.md`). Every key comes from the
//! `CardView` alone, so any copy of the cards sorts the same way.

use std::cmp::Ordering;

use crate::model::CardView;
use crate::protocol::CardSort;

/// Sorts `cards` by `by`, reversed when `desc`. Cards without a value for the sort (one never reviewed, under
/// `reviewed`, `interval` or `retention`) come last either way. Ties, and those cards, go oldest first.
pub fn sort(cards: &mut Vec<CardView>, by: CardSort, desc: bool, seed: u32) {
    let mut keyed: Vec<(Option<Key>, CardView)> = cards.drain(..).map(|c| (key(&c, by, seed), c)).collect();
    keyed.sort_by(|(ka, a), (kb, b)| {
        let oldest = (a.card.created_at, &a.card.id).cmp(&(b.card.created_at, &b.card.id));
        match (ka, kb) {
            (Some(x), Some(y)) => {
                let o = x.cmp(y).then(oldest);
                if desc { o.reverse() } else { o }
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => oldest,
        }
    });
    cards.extend(keyed.into_iter().map(|(_, c)| c));
}

enum Key {
    Int(i64),
    Real(f64),
    Text(String),
}

impl Key {
    fn cmp(&self, other: &Key) -> Ordering {
        match (self, other) {
            (Key::Int(a), Key::Int(b)) => a.cmp(b),
            (Key::Real(a), Key::Real(b)) => a.total_cmp(b),
            (Key::Text(a), Key::Text(b)) => natural_cmp(a, b),
            _ => Ordering::Equal,
        }
    }
}

fn key(c: &CardView, by: CardSort, seed: u32) -> Option<Key> {
    let s = &c.schedule;
    Some(match by {
        CardSort::Created => Key::Int(c.card.created_at),
        CardSort::Updated => Key::Int(c.card.updated_at),
        CardSort::Name => Key::Text(title_key(c.sides.first().map_or("", String::as_str))),
        CardSort::Due => Key::Int(s.due),
        CardSort::Reviewed => Key::Int(s.last_review?),
        CardSort::Interval => Key::Int(s.due - s.last_review?),
        CardSort::Retention if s.reps == 0 => return None,
        CardSort::Retention => Key::Real(f64::from(s.reps.saturating_sub(s.lapses)) / f64::from(s.reps)),
        CardSort::Random => Key::Int(i64::from(fnv1a(format!("{seed}:{}", c.card.id).as_bytes()))),
    })
}

/// What `name` sorts a card by: its front's first non-blank line without Markdown's `#*_\`~>` marks or a leading
/// list dash, lowercased.
pub fn title_key(front: &str) -> String {
    let line = front.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let plain: String = line.chars().filter(|c| !"#*_`~>".contains(*c)).collect();
    plain.trim().trim_start_matches('-').trim().to_lowercase()
}

/// Text in order with runs of ASCII digits compared by their value, so "card 9" comes before "card 10".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (ra, rb) = (digits(&a, &mut i), digits(&b, &mut j));
            let o = ra.len().cmp(&rb.len()).then_with(|| ra.cmp(rb));
            if o != Ordering::Equal {
                return o;
            }
            continue;
        }
        match a[i].cmp(&b[j]) {
            Ordering::Equal => (i, j) = (i + 1, j + 1),
            o => return o,
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// The digit run starting at `*at`, without its leading zeros; moves `*at` past it.
fn digits<'a>(s: &'a [char], at: &mut usize) -> &'a [char] {
    let start = *at;
    while *at < s.len() && s[*at].is_ascii_digit() {
        *at += 1;
    }
    let run = &s[start..*at];
    let zeros = run.iter().take_while(|c| **c == '0').count();
    &run[zeros.min(run.len() - 1)..]
}

/// 32-bit FNV-1a: small, and the same in every implementation.
fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |h: u32, b| (h ^ u32::from(*b)).wrapping_mul(0x0100_0193))
}

#[cfg(test)]
mod tests;
