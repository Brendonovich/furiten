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
     then say whether you remembered it: 1 forgot, 2 remembered.",
    "Why is a card you know well shown less often?\n---\nRecall schedules with FSRS. Each answer updates how well \
     you know the card, so cards you keep remembering wait longer and ones you forget come back sooner.",
    "Can other tools use Recall?\n---\nYes. The `recall` command line does everything the app does (add `--json` \
     for scripts), and agents can connect through the MCP server.",
    "How do you make your own deck?\n---\nCreate a deck, then add cards: write the question, a line with `---`, \
     then the answer. From a terminal: `printf 'hola\\n---\\nhello' | recall add --deck Spanish -`.",
];

/// When a sample card is due, relative to now, with its review and lapse counts; `None` is a new card.
type Seen = Option<(i64, u32, u32)>;

/// A deck someone has used for a while: short and long fronts, one-sided and multi-sided cards, some due now, some
/// later, some new.
const SPANISH: [(&str, &[&str], Seen); 10] = [
    ("**la biblioteca**\n---\nthe library", &["vocab", "places"], Some((-HOUR, 4, 0))),
    (
        "**el desayuno**\n---\nbreakfast\n---\n*Desayuno a las ocho.* I have breakfast at eight.",
        &["vocab", "food"],
        Some((-2 * HOUR, 6, 1)),
    ),
    ("**madrugar**\n---\nto get up early", &["vocab", "verbs"], Some((-3 * HOUR, 2, 0))),
    (
        "**la sobremesa**\n---\nthe time spent talking at the table after a meal",
        &["vocab", "culture"],
        Some((3 * HOUR, 5, 0)),
    ),
    (
        "When do you use *ser* rather than *estar*?\n---\n*Ser* for what something is: identity, origin, \
         profession, time.\n---\n*Estar* for how or where it is: location, mood, the result of a change.",
        &["grammar"],
        Some((2 * DAY, 9, 2)),
    ),
    (
        "Translate: *Si hubiera sabido que venías, habría preparado algo especial para la cena, pero nadie me \
         avisó hasta el último momento.*\n---\nIf I had known you were coming, I would have made something special \
         for dinner, but nobody told me until the last minute.",
        &["grammar", "subjunctive"],
        Some((5 * DAY, 3, 0)),
    ),
    ("**aprovechar**\n---\nto make the most of something", &["vocab", "verbs"], None),
    ("**el atardecer**\n---\nsunset", &["vocab"], None),
    (
        "*Ojalá* comes from the Arabic *law šá lláh*, “if God wills”, from the centuries when Arabic was spoken \
         across much of Spain. It always takes the subjunctive: *ojalá que llueva* (I hope it rains), *ojalá \
         hubiera venido* (if only she had come). Hundreds of everyday words came the same way: *aceite*, \
         *almohada*, *azúcar*, *hasta*.",
        &["culture"],
        None,
    ),
    ("**tener ganas de**\n---\nto feel like doing something", &["vocab", "expressions"], Some((12 * DAY, 7, 0))),
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
        for (i, (content, tags, seen)) in SPANISH.iter().enumerate() {
            let card = f.add_card(&spanish, content, tags, now - 30 * DAY + i as i64, seen.map(|(due, ..)| now + due));
            if let Some((_, reps, lapses)) = seen {
                let schedule =
                    &mut f.cards.iter_mut().find(|c| c.card.id == card.card.id).expect("just added").schedule;
                schedule.reps = *reps;
                schedule.lapses = *lapses;
                // A card reviewed more often waits longer between reviews, so the intervals differ.
                schedule.last_review = Some(schedule.due - i64::from(*reps) * DAY);
            }
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

    /// As the core filters: by deck, then by `q` anywhere in the Markdown, ignoring case; sorted as the core sorts.
    pub fn cards(&self, deck_id: Option<&str>, q: &str, order: Order) -> Vec<CardView> {
        let q = q.to_lowercase();
        let mut cards = self
            .cards
            .iter()
            .filter(|c| deck_id.is_none_or(|d| c.card.deck_id == d))
            .filter(|c| q.is_empty() || c.card.content.to_lowercase().contains(&q))
            .cloned()
            .collect();
        recall_core::sort::sort(&mut cards, order.by, order.desc, order.seed);
        cards
    }

    pub fn card(&self, id: &str) -> Result<CardView, String> {
        self.cards.iter().find(|c| c.card.id == id).cloned().ok_or_else(|| format!("card {id} not found"))
    }

    /// As the core lists them: those on cards in `deck_id` or its subdecks first, then the rest, each part the most
    /// recently used (by the latest `updated_at` among their cards) first.
    pub fn tags(&self, deck_id: &str) -> Vec<TagSummary> {
        let mut scope = vec![deck_id.to_owned()];
        let mut i = 0;
        while i < scope.len() {
            let children = self.decks.iter().filter(|d| d.parent_id.as_ref() == Some(&scope[i]));
            let children: Vec<String> = children.map(|d| d.id.clone()).filter(|id| !scope.contains(id)).collect();
            scope.extend(children);
            i += 1;
        }
        let in_deck = |tag: &str| {
            self.cards.iter().any(|c| scope.contains(&c.card.deck_id) && c.card.tags.iter().any(|t| t == tag))
        };
        let mut tags: Vec<TagSummary> = Vec::new();
        for c in &self.cards {
            for tag in &c.card.tags {
                match tags.iter_mut().find(|t| &t.tag == tag) {
                    Some(t) => {
                        t.cards += 1;
                        t.last_used = t.last_used.max(c.card.updated_at);
                    }
                    None => tags.push(TagSummary { tag: tag.clone(), cards: 1, last_used: c.card.updated_at }),
                }
            }
        }
        tags.sort_by(|a, b| {
            (in_deck(&b.tag).cmp(&in_deck(&a.tag)))
                .then_with(|| b.last_used.cmp(&a.last_used))
                .then_with(|| a.tag.cmp(&b.tag))
        });
        tags
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
        let wait = if answer.remembered { 10 * MIN } else { MIN };
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
