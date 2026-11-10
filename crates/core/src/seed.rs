//! The sample deck every new store starts with. Its IDs and times are fixed so two devices that both seed it
//! converge on one deck after sync instead of two copies.

use crate::model::{Card, Deck, ulid_from};

/// 2026-01-01T00:00:00Z.
const SEEDED_AT: i64 = 1_767_225_600_000;

const CARDS: [&str; 5] = [
    "What does a line with only `---` do in a card?\n---\nIt splits the card into sides. The first side is the \
     question; the rest are revealed when you flip it.",
    "How do you answer a card while reviewing?\n---\nPress Space (Enter in the terminal) to reveal the answer, then say whether \
     you remembered it: 1 forgot, 2 remembered.",
    "Why is a card you know well shown less often?\n---\nFuriten schedules with FSRS. Each answer updates how well \
     you know the card, so cards you keep remembering wait longer and ones you forget come back sooner.",
    "Can other tools use Furiten?\n---\nYes. The `furiten` command line does everything the app does (add `--json` \
     for scripts), and agents can connect through the MCP server.",
    "How do you make your own deck?\n---\nCreate a deck, then add cards: write the question, a line with `---`, \
     then the answer. From a terminal: `printf 'hola\\n---\\nhello' | furiten add --deck Spanish -`.",
];

pub fn deck() -> Deck {
    Deck {
        id: ulid_from(SEEDED_AT, 0),
        name: "Furiten basics".to_string(),
        parent_id: None,
        created_at: SEEDED_AT,
        updated_at: SEEDED_AT,
        deleted: false,
    }
}

pub fn cards() -> Vec<Card> {
    let deck_id = deck().id;
    CARDS
        .iter()
        .enumerate()
        .map(|(i, content)| Card {
            id: ulid_from(SEEDED_AT + i as i64, i as u128 + 1),
            deck_id: deck_id.clone(),
            content: content.to_string(),
            tags: vec!["furiten".to_string()],
            // One ms apart so they come up in order.
            created_at: SEEDED_AT + i as i64,
            updated_at: SEEDED_AT + i as i64,
            deleted: false,
        })
        .collect()
}
