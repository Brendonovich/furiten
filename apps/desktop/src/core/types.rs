//! The contract's types, straight from `recall-core`.

pub use recall_core::model::sides;
pub use recall_core::now_ms;
pub use recall_core::protocol::{Answer, CardQuery, CardUpdate, NewCard, NextReview, Remaining, Status};
pub use recall_core::{Card, CardView, Deck, DeckSummary, Schedule, State};

pub type Id = String;
