//! Human-readable output.

use recall_core::model::{CardView, DeckSummary};
use recall_core::protocol::{NextReview, Stats, Status};
use recall_core::{State, now_ms};

pub fn status(s: &Status) -> String {
    let sync = match &s.server {
        None => "not signed in".to_string(),
        Some(server) => {
            let last = s.last_sync.map_or("never synced".to_string(), |t| format!("synced {}", relative(t)));
            match (server.strip_prefix("hub:"), s.connected) {
                (Some(id), connected) => {
                    let state = if connected == Some(true) { "connected" } else { "not connected" };
                    format!("hub {id} ({state}), {last}")
                }
                (None, _) => format!("{server}, {last}"),
            }
        }
    };
    let mut out = format!("recall core {} at {}\nsync: {sync}", s.version, s.home);
    if let Some(error) = &s.sync_error {
        out.push_str(&format!("\nlast sync failed: {error}"));
    }
    out
}

pub fn decks(decks: &[DeckSummary]) -> String {
    if decks.is_empty() {
        return "No decks. Add a card with: recall add --deck <name> \"question\\n---\\nanswer\"".to_string();
    }
    let mut rows = Vec::new();
    let is_root = |d: &DeckSummary| d.deck.parent_id.as_ref().is_none_or(|p| !decks.iter().any(|o| &o.deck.id == p));
    for deck in decks.iter().filter(|d| is_root(d)) {
        tree(decks, deck, 0, &mut rows);
    }
    let width = rows.iter().map(|(name, _)| name.chars().count()).max().unwrap_or(0);
    rows.into_iter().map(|(name, rest)| format!("{name:width$}  {rest}")).collect::<Vec<_>>().join("\n")
}

fn tree(all: &[DeckSummary], deck: &DeckSummary, depth: usize, rows: &mut Vec<(String, String)>) {
    if depth > 32 {
        return;
    }
    let d = deck;
    rows.push((
        format!("{}{}", "  ".repeat(depth), d.deck.name),
        format!("{:>4} cards {:>4} due {:>4} new  {}", d.cards, d.due, d.new, d.deck.id),
    ));
    for child in all.iter().filter(|c| c.deck.parent_id.as_ref() == Some(&d.deck.id)) {
        tree(all, child, depth + 1, rows);
    }
}

pub fn cards(cards: &[CardView]) -> String {
    if cards.is_empty() {
        return "No cards.".to_string();
    }
    cards
        .iter()
        .map(|c| {
            let tags = c.card.tags.iter().map(|t| format!(" #{t}")).collect::<String>();
            format!("{}  {:<9} {}{tags}", c.card.id, due(c), truncate(first_line(c.front()), 60))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn card(c: &CardView, deck: Option<&str>) -> String {
    let s = &c.schedule;
    let mut out = format!("{}  deck: {}", c.card.id, deck.unwrap_or(&c.card.deck_id));
    if !c.card.tags.is_empty() {
        out.push_str(&format!("  tags: {}", c.card.tags.join(", ")));
    }
    out.push_str(&format!("\n{}, {}, {} reps, {} lapses\n\n", s.state.as_str(), due(c), s.reps, s.lapses));
    out.push_str(&c.sides.join("\n---\n"));
    out
}

pub fn next(n: &NextReview) -> String {
    let left = format!("{} due, {} new", n.remaining.due, n.remaining.new);
    match &n.card {
        None => format!("Nothing to review ({left})."),
        Some(c) => format!("{}  ({left})\n\n{}", c.card.id, c.front()),
    }
}

pub fn answered(rating: u8, c: &CardView) -> String {
    format!("{}: next {}", rating_name(rating), due(c))
}

pub fn stats(s: &Stats) -> String {
    let retention = s.retention_30d.map_or("n/a".to_string(), |r| format!("{:.0}%", r * 100.0));
    format!(
        "{} cards, {} due, {} new\n{} reviews today, 30-day retention {retention}",
        s.cards, s.due, s.new, s.reviews_today
    )
}

pub fn rating_name(rating: u8) -> &'static str {
    match rating {
        1 => "Again",
        2 => "Hard",
        3 => "Good",
        _ => "Easy",
    }
}

/// "new", "due" or when it's due, e.g. "in 3d".
pub fn due(c: &CardView) -> String {
    if c.schedule.state == State::New {
        "new".to_string()
    } else if c.schedule.due <= now_ms() {
        "due".to_string()
    } else {
        relative(c.schedule.due)
    }
}

/// "in 5m", "3d ago", "now".
pub fn relative(at: i64) -> String {
    let delta = at - now_ms();
    let secs = delta.abs() / 1000;
    let round = |unit: i64| (secs + unit / 2) / unit;
    let span = match secs {
        0..30 => return "now".to_string(),
        30..3570 => format!("{}m", round(60)),
        3570..84_600 => format!("{}h", round(3600)),
        84_600..2_548_800 => format!("{}d", round(86_400)),
        2_548_800..31_536_000 => format!("{}mo", round(2_592_000)),
        _ => format!("{:.1}y", secs as f64 / 31_536_000.0),
    };
    if delta > 0 { format!("in {span}") } else { format!("{span} ago") }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{cut}…")
}
