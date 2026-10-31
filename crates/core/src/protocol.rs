//! Params and results of the local core protocol (and the matching HTTP bodies).

use serde::{Deserialize, Deserializer, Serialize};

use crate::model::{CardView, Change, Id};

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
    pub rating: u8,
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

/// `{ server, token? }` signs in to Recall Cloud; `{ hub, name? }` pairs with a self-hosted hub.
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
