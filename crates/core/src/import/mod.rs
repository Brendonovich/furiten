//! Bringing in other apps' decks. A reader turns an export into a [`Library`], and [`Store::import`] adds it as new
//! decks, cards and reviews.
//!
//! [`Store::import`]: crate::Store::import

#[cfg(feature = "import")]
pub mod mochi;

/// Decks to add, in no particular order: a deck's `parent` is another deck's `key`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Library {
    pub decks: Vec<ImportDeck>,
    /// Cards the export had that won't be imported (trashed, or with nothing on them).
    pub skipped: u64,
    /// Files in the export (images, audio) that aren't imported: Recall has no attachments yet.
    pub attachments: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportDeck {
    /// The export's own id for the deck, only used to find its parent.
    pub key: String,
    pub name: String,
    pub parent: Option<String>,
    /// In the order they should be learned.
    pub cards: Vec<ImportCard>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportCard {
    /// Markdown, `---` between sides.
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: Option<i64>,
    pub reviews: Vec<ImportReview>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportReview {
    pub reviewed_at: i64,
    /// As `Review::rating`.
    pub rating: u8,
    pub duration_ms: Option<i64>,
}
