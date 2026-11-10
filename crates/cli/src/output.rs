//! Human-readable output.

use furiten_core::model::{CardView, DeckSummary};
use std::io::IsTerminal;

use furiten_core::protocol::{HostStatus, Imported, NextReview, PairTicket, Stats, Status, TagSummary, WebStatus};
use furiten_core::{State, now_ms};
use qrcode::render::unicode::Dense1x2;
use qrcode::{EcLevel, QrCode};

pub fn status(s: &Status) -> String {
    let sync = match &s.server {
        None if s.hosting => "hosting: paired phones sync with this desktop (furiten host)".to_string(),
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
    let mut out = format!("furiten core {} at {}\nsync: {sync}", s.version, s.home);
    if let Some(error) = &s.sync_error {
        out.push_str(&format!("\nlast sync failed: {error}"));
    }
    out
}

pub fn web(w: &WebStatus) -> String {
    if !w.enabled {
        return "Web access is off. Turn it on with: furiten web on".to_string();
    }
    let mut out = String::from(if w.running { "Web access is on." } else { "Web access is on but not serving." });
    if let Some(error) = &w.error {
        out.push_str(&format!(" {error}"));
    }
    if !w.urls.is_empty() {
        out.push_str("\nOpen it at:");
        for url in &w.urls {
            out.push_str(&format!("\n  {url}"));
        }
    }
    if let Some(token) = &w.token {
        out.push_str(&format!("\nToken: {token}"));
    }
    if let Some(link) = &w.link {
        out.push_str(&format!("\nSign-in link: {link}"));
    }
    out
}

pub fn host(h: &HostStatus) -> String {
    let mut out = String::from(match (h.enabled, h.running) {
        (false, _) => "Hosting is off. Pair a phone with: furiten host pair",
        (true, true) => "Hosting: phones paired with this desktop sync with it.",
        (true, false) => "Hosting is on but not running.",
    });
    if let Some(error) = &h.error {
        out.push_str(&format!(" {error}"));
    }
    if let Some(id) = &h.endpoint_id {
        out.push_str(&format!(
            "
Endpoint: {id}"
        ));
    }
    if h.devices.is_empty() {
        out.push_str(
            "
No devices paired.",
        );
    }
    let width = h.devices.iter().map(|d| d.device.name.chars().count()).max().unwrap_or(0);
    for d in &h.devices {
        let state = match (d.connected, d.last_sync) {
            (true, _) => "connected".to_string(),
            (false, Some(at)) => format!("synced {}", relative(at)),
            (false, None) => "offline".to_string(),
        };
        out.push_str(&format!(
            "
  {:width$}  {}  {state}",
            d.device.name, d.device.id
        ));
    }
    out
}

/// The ticket, as a QR code for the phone's camera when printing to a terminal.
pub fn pair(t: &PairTicket) -> String {
    let mut out = String::new();
    // A link, so the phone's Camera app opens Furiten to pair; the ticket in uppercase fits QR's alphanumeric
    // mode, a smaller code, and tickets read case-insensitively.
    let link = format!("furiten://pair/{}", t.ticket.to_uppercase());
    if std::io::stdout().is_terminal()
        && let Ok(code) = QrCode::with_error_correction_level(link, EcLevel::L)
    {
        out.push_str(&code.render::<Dense1x2>().quiet_zone(true).build());
        out.push('\n');
    }
    let minutes = ((t.expires_at - now_ms()) as f64 / 60_000.0).round();
    out.push_str(&format!(
        "{}\n\nScan it in Furiten on your phone (Settings → Pair with your computer), valid {minutes} minutes, once. \
         Or on another device: furiten sync login --hub <ticket>",
        t.ticket
    ));
    out
}

pub fn decks(decks: &[DeckSummary]) -> String {
    if decks.is_empty() {
        return "No decks. Add a card with: furiten add --deck <name> \"question\\n---\\nanswer\"".to_string();
    }
    let mut rows = Vec::new();
    let is_root = |d: &DeckSummary| d.deck.parent_id.as_ref().is_none_or(|p| !decks.iter().any(|o| &o.deck.id == p));
    for deck in decks.iter().filter(|d| is_root(d)) {
        tree(decks, deck, 0, &mut rows);
    }
    let width = rows.iter().map(|(name, _)| name.chars().count()).max().unwrap_or(0);
    rows.into_iter().map(|(name, rest)| format!("{name:width$}  {rest}")).collect::<Vec<_>>().join("\n")
}

pub fn tags(tags: &[TagSummary]) -> String {
    if tags.is_empty() {
        return "No tags. Tag a card with: furiten edit <id> --tags <tag>".to_string();
    }
    let width = tags.iter().map(|t| t.tag.chars().count()).max().unwrap_or(0);
    let rows = tags.iter().map(|t| format!("{:width$}  {:>4} cards  used {}", t.tag, t.cards, relative(t.last_used)));
    rows.collect::<Vec<_>>().join("\n")
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

pub fn answered(remembered: bool, c: &CardView) -> String {
    format!("{}: next {}", if remembered { "Remembered" } else { "Forgot" }, due(c))
}

pub fn stats(s: &Stats) -> String {
    let retention = s.retention_30d.map_or("n/a".to_string(), |r| format!("{:.0}%", r * 100.0));
    format!(
        "{} cards, {} due, {} new\n{} reviews today, 30-day retention {retention}",
        s.cards, s.due, s.new, s.reviews_today
    )
}

pub fn imported(i: &Imported) -> String {
    let names: Vec<&str> = i
        .decks
        .iter()
        .filter(|d| !i.decks.iter().any(|p| Some(&p.id) == d.parent_id.as_ref()))
        .map(|d| d.name.as_str())
        .collect();
    let mut out = format!(
        "Imported {}, {} and {} from Mochi: {}",
        plural(i.decks.len() as u64, "deck"),
        plural(i.cards, "card"),
        plural(i.reviews, "review"),
        names.join(", ")
    );
    if i.skipped > 0 {
        out.push_str(&format!("\nSkipped {} (trashed or empty)", plural(i.skipped, "card")));
    }
    if i.attachments > 0 {
        out.push_str(&format!(
            "\nLeft out {}: Furiten can't show images or audio yet",
            plural(i.attachments, "attachment")
        ));
    }
    out
}

fn plural(n: u64, one: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") }
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
