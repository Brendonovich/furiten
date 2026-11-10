//! Params and results of the local core protocol (and the matching HTTP bodies).

use serde::{Deserialize, Deserializer, Serialize};

use crate::model::{CardView, Change, Deck, Id};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub home: String,
    pub signed_in: bool,
    pub last_sync: Option<i64>,
    pub server: Option<String>,
    /// The last sync's error, cleared by the next successful sync.
    #[serde(default)]
    pub sync_error: Option<String>,
    /// With a self-hosted hub, whether the core holds a live connection to it; `None` otherwise.
    #[serde(default)]
    pub connected: Option<bool>,
    /// Whether this library is its own hub for paired phones (see `host.status`).
    #[serde(default)]
    pub hosting: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NewDeck {
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<Id>,
}

/// `parent_id: None` leaves the parent alone, `Some(None)` moves the deck to the top level.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeckUpdate {
    pub id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<Id>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IdParams {
    pub id: Id,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CardQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<bool>,
    /// Defaults to 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    /// Defaults to `created`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<CardSort>,
    /// Reverses the sort; cards without a value for it stay last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<bool>,
    /// Picks the `random` order; the same seed gives the same order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
}

/// How `cards.list` orders cards (Mochi's deck-page sorts). It never changes the review queue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardSort {
    #[default]
    Created,
    Updated,
    Name,
    Due,
    Reviewed,
    Interval,
    Retention,
    Random,
}

impl CardSort {
    pub const ALL: [CardSort; 8] = [
        CardSort::Created,
        CardSort::Updated,
        CardSort::Name,
        CardSort::Due,
        CardSort::Reviewed,
        CardSort::Interval,
        CardSort::Retention,
        CardSort::Random,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CardSort::Created => "created",
            CardSort::Updated => "updated",
            CardSort::Name => "name",
            CardSort::Due => "due",
            CardSort::Reviewed => "reviewed",
            CardSort::Interval => "interval",
            CardSort::Retention => "retention",
            CardSort::Random => "random",
        }
    }

    pub fn parse(value: &str) -> Option<CardSort> {
        CardSort::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NewCard {
    pub deck_id: Id,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NewCards {
    pub cards: Vec<NewCard>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cards {
    pub cards: Vec<CardView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CardUpdate {
    pub id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck_id: Option<Id>,
}

/// Params of `review.next` and `stats`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeckFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck_id: Option<Id>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NextReview {
    pub card: Option<CardView>,
    pub remaining: Remaining,
}

/// Cards left in the queue, including the one returned with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remaining {
    pub due: u64,
    pub new: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub card_id: Id,
    pub remembered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub cards: u64,
    pub due: u64,
    pub new: u64,
    pub reviews_today: u64,
    /// `null` when there were no reviews of cards in review state in the last 30 days.
    pub retention_30d: Option<f64>,
}

/// A tag on live cards: how many, and when one of them last changed (`updated_at`), which is when the tag was last
/// used.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSummary {
    pub tag: String,
    pub cards: u64,
    pub last_used: i64,
}

/// `{ server, token? }` signs in to Furiten Cloud; `{ hub, name? }` pairs with a self-hosted hub.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Login {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// A hub's pairing ticket (`rchub1...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub: Option<String>,
    /// This device's name in the hub's device list; defaults to the host name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggedIn {
    pub user_id: String,
    /// The account's token, needed to sign in on another device.
    pub token: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubLoggedIn {
    pub library_id: String,
}

/// Params of `import.mochi`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportFile {
    /// An absolute path on the core's machine: a `.mochi` export, or a bare `data.json` or `data.edn`.
    pub path: String,
    /// Where the imported decks go; the top level when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Id>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Imported {
    /// The new decks, every parent before its children.
    pub decks: Vec<Deck>,
    pub cards: u64,
    pub reviews: u64,
    /// Cards in the export that weren't imported: trashed, or with nothing on them.
    pub skipped: u64,
    /// Files in the export (images, audio) that weren't imported.
    pub attachments: u64,
}

/// Web access: whether the desktop serves its library over HTTP, where, and the one token that opens it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebStatus {
    pub enabled: bool,
    /// Whether it's listening now: only the desktop app serves it, so a core the CLI hosts never does.
    pub running: bool,
    pub bind: String,
    pub port: u16,
    /// `None` until web access is first turned on.
    pub token: Option<String>,
    /// Addresses a browser can open it at, the best first: Tailscale's name and IP, then the LAN's.
    pub urls: Vec<String>,
    /// The first of `urls` with `#token=`, which signs a browser in when opened (or scanned).
    pub link: Option<String>,
    /// Why it isn't running though enabled (the port is taken, say).
    pub error: Option<String>,
}

/// Params of `web.enable`: what to change, or nothing to keep the current (or default) address.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebEnable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,
}

/// Hosting: whether this desktop is its library's hub, so phones pair with it and sync over Iroh.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostStatus {
    pub enabled: bool,
    /// Whether it's accepting devices now: only the desktop app hosts, so a core the CLI hosts never does.
    pub running: bool,
    /// The endpoint id devices dial; `None` before this device has a key.
    pub endpoint_id: Option<String>,
    pub devices: Vec<DeviceInfo>,
    /// Why it isn't running though enabled.
    pub error: Option<String>,
}

/// A device paired with a hub, or with a hosting desktop.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// Its endpoint id (hex).
    pub id: String,
    pub name: String,
    pub added_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    #[serde(flatten)]
    pub device: Device,
    pub connected: bool,
    /// When it last synced since the hub (or the desktop app) started.
    pub last_sync: Option<i64>,
}

/// A one-time pairing ticket (`rchub1...`) and when it stops working.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairTicket {
    pub ticket: String,
    pub expires_at: i64,
}

/// Params of `host.remove`: a device's id, id prefix or name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceQuery {
    pub device: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncResult {
    pub pushed: u64,
    pub pulled: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ack {
    pub ok: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub cursor: Option<i64>,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncResponse {
    pub cursor: i64,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub user_id: String,
    pub token: String,
}

fn double_option<'de, D: Deserializer<'de>>(de: D) -> Result<Option<Option<Id>>, D::Error> {
    Option::<Id>::deserialize(de).map(Some)
}
