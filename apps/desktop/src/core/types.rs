//! The contract's types, straight from `recall-core`.

pub use recall_core::model::{content_from_sides, sides};
pub use recall_core::now_ms;
pub use recall_core::protocol::{
    Answer, CardQuery, CardSort, CardUpdate, HostStatus, Imported, NewCard, NextReview, PairTicket, Remaining, Status,
    TagSummary, WebEnable, WebStatus,
};
pub use recall_core::{Card, CardView, Deck, DeckSummary, Schedule, State};

pub type Id = String;

/// How a deck's cards are ordered: the core's sort, reversed or not, and the seed `Random` shuffles with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Order {
    pub by: CardSort,
    pub desc: bool,
    pub seed: u32,
}
