//! An in-memory stand-in for the core, for building the UI before the real client is wired up and for
//! deterministic screenshots (`--fixtures`). It answers the same calls with data shaped like the core's, but it
//! does not schedule: an answered card just moves a fixed time into the future so the queue advances.

use super::types::*;

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;

/// The sample deck's cards, as the core seeds them.
const BASICS: [&str; 5] = [
    "What does a line with only `---` do in a card?\n---\nIt splits the card into sides. The first side is the \
     question; the rest are revealed when you flip it.",
    "How do you answer a card while reviewing?\n---\nPress Space (Enter in the terminal) to reveal the answer, \
     then grade yourself: 1 again, 2 hard, 3 good, 4 easy.",
    "Why is a card you know well shown less often?\n---\nRecall schedules with FSRS. Each grade updates how well \
     you know the card, so easy cards wait longer and hard ones come back sooner.",
    "Can other tools use Recall?\n---\nYes. The `recall` command line does everything the app does (add `--json` \
     for scripts), and agents can connect through the MCP server.",
    "How do you make your own deck?\n---\nCreate a deck, then add cards: write the question, a line with `---`, \
     then the answer. From a terminal: `printf 'hola\\n---\\nhello' | recall add --deck Spanish -`.",
];

const SPANISH: [&str; 6] = [
    "**la biblioteca**\n---\nthe library",
    "**el desayuno**\n---\nbreakfast",
    "**madrugar**\n---\nto get up early",
    "**la sobremesa**\n---\nthe time spent talking at the table after a meal",
    "**aprovechar**\n---\nto make the most of something",
    "**el atardecer**\n---\nsunset",
];

const RUST: [&str; 4] = [
    "What does `?` do in a function returning `Result`?\n---\nReturns early with the error (converted with `From`), \
     otherwise unwraps the `Ok` value.",
    "`Rc<T>` vs `Arc<T>`?\n---\n`Arc` uses atomic reference counts, so it is `Send + Sync`; `Rc` is cheaper but \
     single-threaded.",
    "What is a lifetime elision rule?\n---\nWith one input reference, its lifetime is given to every output reference.",
    "What does `#[derive(Clone, Copy)]` require?\n---\nEvery field must be `Copy`.",
];

pub struct Fixture {
    decks: Vec<Deck>,
    cards: Vec<CardView>,
    next_id: u64,
}

impl Fixture {
    /// A brand-new store: only the sample deck, nothing reviewed.
    pub fn first_run() -> Fixture {
        let mut f = Fixture { decks: Vec::new(), cards: Vec::new(), next_id: 0 };
        let basics = f.add_deck("Recall basics", 1_767_225_600_000);
        for (i, content) in BASICS.iter().enumerate() {
            f.add_card(&basics, content, &["recall"], 1_767_225_600_000 + i as i64, None);
        }
        f
    }

    /// A store someone has used for a while: some cards reviewed and due later, some due now.
    pub fn lived_in() -> Fixture {
        let now = now_ms();
        let mut f = Fixture::first_run();
        let basics = f.decks[0].id.clone();
        for (i, card) in f.cards.iter_mut().enumerate().filter(|(_, c)| c.card.deck_id == basics) {
            reviewed(&mut card.schedule, now - 2 * DAY, now + (i as i64 + 1) * DAY);
        }
        let spanish = f.add_deck("Spanish", now - 30 * DAY);
        for (i, content) in SPANISH.iter().enumerate() {
            let due = match i {
                0..=2 => Some(now - (i as i64 + 1) * HOUR),
                3 => Some(now + 3 * HOUR),
                _ => None,
            };
            f.add_card(&spanish, content, &["vocab"], now - 30 * DAY + i as i64, due);
        }
        let rust = f.add_deck("Rust", now - 10 * DAY);
        for (i, content) in RUST.iter().enumerate() {
            let due = if i < 2 { Some(now - 20 * MIN) } else { None };
            f.add_card(&rust, content, &["rust", "programming"], now - 10 * DAY + i as i64, due);
        }
        f
    }

    /// Every card reviewed and due later: nothing left to review today.
    pub fn caught_up() -> Fixture {
        let now = now_ms();
        let mut f = Fixture::lived_in();
        for (i, card) in f.cards.iter_mut().enumerate() {
            reviewed(&mut card.schedule, now - DAY, now + 3 * HOUR + i as i64 * HOUR);
        }
        f
    }

    fn id(&mut self, prefix: &str) -> Id {
        self.next_id += 1;
        format!("{prefix}{:022}", self.next_id)
    }

    fn add_deck(&mut self, name: &str, at: i64) -> Id {
        let id = self.id("FXD");
        self.decks.push(Deck {
            id: id.clone(),
            name: name.into(),
            parent_id: None,
            created_at: at,
            updated_at: at,
            deleted: false,
        });
        id
    }

    fn add_card(&mut self, deck: &str, content: &str, tags: &[&str], at: i64, reviewed_due: Option<i64>) -> CardView {
        let id = self.id("FXC");
        let mut schedule = Schedule {
            state: State::New,
            due: at,
            stability: 0.0,
            difficulty: 0.0,
            reps: 0,
            lapses: 0,
            last_review: None,
        };
        if let Some(due) = reviewed_due {
            reviewed(&mut schedule, due - DAY, due);
        }
        let view = CardView {
            sides: sides(content),
            card: Card {
                id,
                deck_id: deck.into(),
                content: content.into(),
                tags: tags.iter().map(|t| t.to_string()).collect(),
                created_at: at,
                updated_at: at,
                deleted: false,
            },
            schedule,
        };
        self.cards.push(view.clone());
        view
    }

    pub fn status(&self) -> Status {
        Status { version: "fixtures".into(), home: "(in memory)".into(), ..Status::default() }
    }

    pub fn decks(&self) -> Vec<DeckSummary> {
        let now = now_ms();
        self.decks
            .iter()
            .map(|d| {
                let cards = self.cards.iter().filter(|c| c.card.deck_id == d.id);
                let (mut n, mut due, mut new) = (0, 0, 0);
                for c in cards {
                    n += 1;
                    match c.schedule.state {
                        State::New => new += 1,
                        _ if c.schedule.due <= now => due += 1,
                        _ => {}
                    }
                }
                DeckSummary { deck: d.clone(), cards: n, due, new }
            })
            .collect()
    }

    pub fn create_deck(&mut self, name: &str) -> Result<Deck, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A deck needs a name".into());
        }
        let id = self.add_deck(name, now_ms());
        Ok(self.decks.iter().find(|d| d.id == id).cloned().expect("just added"))
    }

    pub fn cards(&self, deck_id: Option<&str>) -> Vec<CardView> {
        self.cards.iter().filter(|c| deck_id.is_none_or(|d| c.card.deck_id == d)).cloned().collect()
    }

    pub fn card(&self, id: &str) -> Result<CardView, String> {
        self.cards.iter().find(|c| c.card.id == id).cloned().ok_or_else(|| format!("card {id} not found"))
    }

    pub fn create_card(&mut self, new: NewCard) -> Result<CardView, String> {
        if !self.decks.iter().any(|d| d.id == new.deck_id) {
            return Err(format!("deck {} not found", new.deck_id));
        }
        let tags: Vec<&str> = new.tags.iter().map(String::as_str).collect();
        Ok(self.add_card(&new.deck_id, &new.content, &tags, now_ms(), None))
    }

    pub fn update_card(&mut self, update: CardUpdate) -> Result<CardView, String> {
        let card = self.cards.iter_mut().find(|c| c.card.id == update.id).ok_or("card not found")?;
        if let Some(content) = update.content {
            card.sides = sides(&content);
            card.card.content = content;
        }
        if let Some(tags) = update.tags {
            card.card.tags = tags;
        }
        if let Some(deck) = update.deck_id {
            card.card.deck_id = deck;
        }
        card.card.updated_at = now_ms();
        Ok(card.clone())
    }

    pub fn delete_card(&mut self, id: &str) -> Result<(), String> {
        self.cards.retain(|c| c.card.id != id);
        Ok(())
    }

    pub fn next_review(&self, deck_id: Option<&str>) -> NextReview {
        let now = now_ms();
        let mine = || self.cards.iter().filter(|c| deck_id.is_none_or(|d| c.card.deck_id == d));
        let mut due: Vec<&CardView> =
            mine().filter(|c| c.schedule.state != State::New && c.schedule.due <= now).collect();
        due.sort_by_key(|c| c.schedule.due);
        let mut new: Vec<&CardView> = mine().filter(|c| c.schedule.state == State::New).collect();
        new.sort_by_key(|c| c.card.created_at);
        let remaining = Remaining { due: due.len() as u64, new: new.len() as u64 };
        NextReview { card: due.first().or(new.first()).map(|c| (*c).clone()), remaining }
    }

    pub fn answer(&mut self, answer: Answer) -> Result<CardView, String> {
        let now = now_ms();
        let card = self.cards.iter_mut().find(|c| c.card.id == answer.card_id).ok_or("card not found")?;
        let wait = match answer.rating {
            1 => MIN,
            2 => 6 * MIN,
            3 => 10 * MIN,
            _ => 4 * DAY,
        };
        reviewed(&mut card.schedule, now, now + wait);
        Ok(card.clone())
    }
}

fn reviewed(s: &mut Schedule, at: i64, due: i64) {
    s.state = State::Review;
    s.due = due;
    s.reps += 1;
    s.last_review = Some(at);
}
