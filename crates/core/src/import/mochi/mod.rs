//! Mochi's `.mochi` export: a zip holding `data.json` (Transit JSON) or, in older exports, `data.edn`, plus the
//! cards' attachments. See https://mochi.cards/docs/import-and-export/mochi-format-reference/.
//!
//! Mochi cards are Markdown with `---` between sides, as Recall's are, so content carries over as is. A card with a
//! template is rendered through it (`<< Field >>` placeholders and `<<#field>>`/`<<^field>>` sections). A review
//! Mochi says was remembered becomes Good, a forgotten one Again, so the schedule picks up where Mochi's left off.

mod edn;
mod transit;
mod value;

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::path::Path;

use super::{ImportCard, ImportDeck, ImportReview, Library};
use crate::{Error, Result};
use value::Value;

/// Ratings for Mochi's remembered/forgot.
const REMEMBERED: u8 = 3;
const FORGOT: u8 = 1;

/// Reads a `.mochi` file, or a bare `data.json` or `data.edn`.
pub fn read(path: &Path) -> Result<Library> {
    let bytes = std::fs::read(path).map_err(|e| Error::new("io", format!("couldn't read {}: {e}", path.display())))?;
    if bytes.starts_with(b"PK") {
        return read_zip(&bytes);
    }
    let text = String::from_utf8(bytes).map_err(|_| Error::invalid("this isn't a Mochi export"))?;
    let edn = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("edn"));
    library(&parse(&text, edn)?, 0)
}

fn read_zip(bytes: &[u8]) -> Result<Library> {
    let bad = |e: zip::result::ZipError| Error::invalid(format!("this .mochi file couldn't be opened: {e}"));
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(bad)?;
    let mut data = None;
    let mut attachments = 0;
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(bad)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().map_err(bad)?;
        let name = name.rsplit('/').next().unwrap_or_default().to_ascii_lowercase();
        match name.as_str() {
            // The JSON one is the newer and wins if both are there.
            "data.json" => data = Some((i, false)),
            "data.edn" if data.is_none() => data = Some((i, true)),
            "data.edn" => {}
            _ => attachments += 1,
        }
    }
    let (index, edn) = data.ok_or_else(|| Error::invalid("this .mochi file has no data.json or data.edn in it"))?;
    let mut text = String::new();
    zip.by_index(index)
        .map_err(bad)?
        .read_to_string(&mut text)
        .map_err(|e| Error::invalid(format!("couldn't read the data in this .mochi file: {e}")))?;
    library(&parse(&text, edn)?, attachments)
}

fn parse(text: &str, edn: bool) -> Result<Value> {
    if edn { edn::parse(text) } else { transit::parse(text) }
}

/// Turns Mochi's data into decks and cards. Trashed decks (with what's under them) and trashed cards are left out.
fn library(data: &Value, attachments: u64) -> Result<Library> {
    if !matches!(data, Value::Map(_)) {
        return Err(Error::invalid("this isn't a Mochi export"));
    }
    let templates: HashMap<&str, &Value> =
        data.get("templates").map_or(&[][..], Value::items).iter().filter_map(|t| Some((id(t)?, t))).collect();
    let mochi_decks = data.get("decks").map_or(&[][..], Value::items);

    let keys: Vec<String> =
        mochi_decks.iter().enumerate().map(|(i, d)| id(d).map_or_else(|| format!("deck-{i}"), String::from)).collect();
    let parents: Vec<Option<&str>> = mochi_decks.iter().map(|d| d.get("parent-id").and_then(Value::name)).collect();
    let trashed = trashed_decks(mochi_decks, &keys, &parents);

    let mut skipped = 0;
    let mut decks: Vec<ImportDeck> = Vec::new();
    let mut deck_of: HashMap<&str, usize> = HashMap::new();
    let mut deck_template: Vec<Option<&str>> = Vec::new();
    for (i, d) in mochi_decks.iter().enumerate() {
        let cards = d.get("cards").map_or(&[][..], Value::items);
        if trashed.contains(&i) {
            skipped += cards.len() as u64;
            continue;
        }
        deck_of.insert(&keys[i], decks.len());
        deck_template.push(d.get("template-id").and_then(Value::name));
        let name = d.get("name").and_then(Value::str).map(str::trim).filter(|n| !n.is_empty());
        decks.push(ImportDeck {
            key: keys[i].clone(),
            name: name.unwrap_or("Untitled deck").to_string(),
            parent: parents[i].map(String::from),
            cards: Vec::new(),
        });
    }

    // Cards inside their deck, then the top-level ones that name theirs.
    let mut placed: Vec<(usize, &Value)> = Vec::new();
    for (i, d) in mochi_decks.iter().enumerate() {
        if let Some(&at) = deck_of.get(keys[i].as_str()) {
            placed.extend(d.get("cards").map_or(&[][..], Value::items).iter().map(|c| (at, c)));
        }
    }
    for card in data.get("cards").map_or(&[][..], Value::items) {
        match card.get("deck-id").and_then(Value::name).and_then(|k| deck_of.get(k)) {
            Some(&at) => placed.push((at, card)),
            None => skipped += 1,
        }
    }
    let mut per_deck: Vec<Vec<(Option<&str>, ImportCard)>> = vec![Vec::new(); decks.len()];
    for (at, card) in placed {
        if card.get("trashed?").is_some_and(Value::truthy) {
            skipped += 1;
            continue;
        }
        let template = card.get("template-id").and_then(Value::name).or(deck_template[at]);
        let content = content(card, template.and_then(|t| templates.get(t).copied()));
        if content.trim().is_empty() {
            skipped += 1;
            continue;
        }
        let pos = card.get("pos").and_then(Value::name);
        let mut tags: Vec<String> =
            card.get("tags").map_or(&[][..], Value::items).iter().filter_map(Value::name).map(String::from).collect();
        let content = take_inline_tags(&content, &mut tags);
        per_deck[at].push((
            pos,
            ImportCard {
                content,
                tags,
                created_at: card.get("created-at").and_then(Value::date),
                reviews: reviews(card),
            },
        ));
    }
    for (deck, mut cards) in decks.iter_mut().zip(per_deck) {
        // Mochi's order within a deck; cards without a position keep the file's order, after the rest.
        cards.sort_by(|(a, _), (b, _)| match (a, b) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        deck.cards = cards.into_iter().map(|(_, c)| c).collect();
    }
    Ok(Library { decks, skipped, attachments })
}

fn id(v: &Value) -> Option<&str> {
    v.get("id").and_then(Value::name)
}

/// Decks that are trashed, or sit under one that is.
fn trashed_decks(decks: &[Value], keys: &[String], parents: &[Option<&str>]) -> HashSet<usize> {
    let index: HashMap<&str, usize> = keys.iter().enumerate().map(|(i, k)| (k.as_str(), i)).collect();
    (0..decks.len())
        .filter(|&i| {
            let mut seen = HashSet::new();
            let mut at = Some(i);
            while let Some(d) = at.filter(|d| seen.insert(*d)) {
                if decks[d].get("trashed?").is_some_and(Value::truthy) {
                    return true;
                }
                at = parents[d].and_then(|p| index.get(p).copied());
            }
            false
        })
        .collect()
}

/// The card's Markdown: its template rendered with its fields when it has one, else its own content.
fn content(card: &Value, template: Option<&Value>) -> String {
    let own = card.get("content").and_then(Value::str).unwrap_or_default();
    let Some(markdown) = template.and_then(|t| t.get("content")).and_then(Value::str).filter(|m| !m.trim().is_empty())
    else {
        return own.trim().to_string();
    };
    let mut fields: HashMap<String, String> = HashMap::new();
    for (k, v) in card.get("fields").map_or(&[][..], Value::entries) {
        let Some(id) = k.name() else { continue };
        let value = v.get("value").map_or_else(|| v.text(), Value::text);
        fields.insert(id.to_lowercase(), value);
    }
    // Placeholders name a field by its name; ids work too.
    for (k, f) in template.and_then(|t| t.get("fields")).map_or(&[][..], Value::entries) {
        let (Some(id), Some(name)) = (k.name(), f.get("name").and_then(Value::str)) else { continue };
        if let Some(value) = fields.get(&id.to_lowercase()).cloned() {
            fields.entry(name.trim().to_lowercase()).or_insert(value);
        }
    }
    render(markdown, &|name| fields.get(&name.trim().to_lowercase()).cloned()).trim().to_string()
}

/// Mochi's Mustache-style templates: `<<field>>`, and sections shown when a field is filled (`<<#field>>…<</field>>`)
/// or empty (`<<^field>>…<</field>>`). Fields this can't fill (Mochi's generated ones, like text to speech) render
/// as nothing.
fn render(template: &str, field: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find("<<") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.find(">>") else {
            out.push_str(&rest[open..]);
            return out;
        };
        let tag = after[..close].trim();
        rest = &after[close + 2..];
        if let Some(name) = tag.strip_prefix('#').or_else(|| tag.strip_prefix('^')) {
            let (inner, after_section) = section(rest, name.trim());
            let filled = field(name).is_some_and(|v| !v.trim().is_empty());
            if filled != tag.starts_with('^') {
                out.push_str(&render(inner, field));
            }
            rest = after_section;
        } else if !tag.starts_with('/') {
            out.push_str(&field(tag).unwrap_or_default());
        }
    }
    out.push_str(rest);
    out
}

/// Splits `s` at the `<</name>>` that closes a section: what's inside, and what follows.
fn section<'a>(s: &'a str, name: &str) -> (&'a str, &'a str) {
    let mut from = 0;
    while let Some(open) = s[from..].find("<<").map(|i| from + i) {
        let Some(close) = s[open + 2..].find(">>").map(|i| open + 2 + i) else { break };
        let tag = s[open + 2..close].trim();
        if tag.strip_prefix('/').is_some_and(|t| t.trim().eq_ignore_ascii_case(name)) {
            return (&s[..open], &s[close + 2..]);
        }
        from = close + 2;
    }
    (s, "")
}

/// Mochi reads `#tags` in a card's text as its tags. Lines at the end made only of them would show as text in Recall,
/// so they become tags instead. A `# Heading` (with its space) isn't a tag.
fn take_inline_tags(content: &str, tags: &mut Vec<String>) -> String {
    let is_tag = |word: &str| {
        word.strip_prefix('#')
            .is_some_and(|t| !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/')))
    };
    let mut lines: Vec<&str> = content.lines().collect();
    while let Some(last) = lines.last() {
        let words: Vec<&str> = last.split_whitespace().collect();
        if !words.is_empty() && !words.iter().all(|w| is_tag(w)) {
            break;
        }
        tags.extend(words.iter().map(|w| w[1..].to_string()));
        lines.pop();
    }
    lines.join("\n").trim_end().to_string()
}

fn reviews(card: &Value) -> Vec<ImportReview> {
    let mut reviews: Vec<ImportReview> = card
        .get("reviews")
        .map_or(&[][..], Value::items)
        .iter()
        .filter_map(|r| {
            let reviewed_at = r.get("date").and_then(Value::date)?;
            let remembered = r.get("remembered?").is_some_and(Value::truthy);
            // Mochi records how long a review took in seconds.
            let duration_ms =
                r.get("duration").and_then(Value::number).filter(|d| *d >= 0.0).map(|d| (d * 1000.0) as i64);
            Some(ImportReview { reviewed_at, rating: if remembered { REMEMBERED } else { FORGOT }, duration_ms })
        })
        .collect();
    reviews.sort_by_key(|r| r.reviewed_at);
    reviews
}

#[cfg(test)]
mod tests;
