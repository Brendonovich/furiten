//! The app's state and the actions that change it. Every action is a thin call into `Core` followed by a refetch;
//! nothing here decides what's due or when a card comes back.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rune::TextEdit;
use rune_kit::PaneState;

use crate::core::{Answer, CardUpdate, CardView, Core, DeckSummary, Fixtures, Id, NewCard, NextReview, Status, now_ms};

pub struct App {
    pub core: Core,
    pub status: Status,
    pub decks: Vec<DeckSummary>,
    /// The deck in the main pane; `None` is "All decks".
    pub deck: Option<Id>,
    /// The cards of `deck`, for its card list.
    pub cards: Vec<CardView>,
    pub review: Option<Review>,
    pub editor: Option<Editor>,
    pub settings: Option<Settings>,
    /// The name of a deck being added in the sidebar.
    pub new_deck: Option<TextEdit>,
    pub sidebar: PaneState,
    /// Nothing has been reviewed yet: the app shows only the sample deck and "Start review".
    pub first_run: bool,
    /// The last failed call, shown where it happened until the next one succeeds.
    pub error: Option<String>,
    /// Set from the core's `changed` events; `poll` refetches.
    pub changed: Arc<AtomicBool>,
    /// Why the app couldn't reach the core and fell back to sample data.
    pub offline: Option<String>,
}

pub struct Review {
    pub next: NextReview,
    /// How many of the current card's sides are showing (1 is just the front).
    pub shown: usize,
    pub started: Instant,
    /// Cards answered since the review started.
    pub answered: u32,
    /// What the last answer did, so you see the card get scheduled.
    pub last: Option<Answered>,
}

#[derive(Clone, Copy)]
pub struct Answered {
    pub rating: u8,
    pub due: i64,
}

pub struct Editor {
    /// `None` while writing a new card.
    pub card: Option<Id>,
    pub content: TextEdit,
    pub tags: TextEdit,
    pub deck: Id,
    pub picking_deck: bool,
    /// The first press on "Delete card" asks again in place.
    pub confirm_delete: bool,
    pub error: Option<String>,
}

pub struct Settings {
    pub server: TextEdit,
    pub error: Option<String>,
}

pub const RATINGS: [(u8, &str); 4] = [(1, "Again"), (2, "Hard"), (3, "Good"), (4, "Easy")];

impl Review {
    pub fn card(&self) -> Option<&CardView> {
        self.next.card.as_ref()
    }

    /// Every side is showing, so the card can be graded.
    pub fn revealed(&self) -> bool {
        self.card().is_some_and(|c| self.shown >= c.sides.len())
    }
}

/// `--fixtures [first-run|lived-in|caught-up]` runs on sample data instead of the core.
fn fixtures_arg() -> Option<Fixtures> {
    let args = rune::extra_args();
    let i = args.iter().position(|a| a == "--fixtures")?;
    Some(match args.get(i + 1).map(String::as_str) {
        Some("lived-in") => Fixtures::LivedIn,
        Some("caught-up") => Fixtures::CaughtUp,
        _ => Fixtures::FirstRun,
    })
}

pub fn init() -> App {
    if let Some(which) = fixtures_arg() {
        return App::new(Core::fixtures(which));
    }
    match Core::connect() {
        Ok(core) => App::new(core),
        Err(e) => {
            let mut s = App::new(Core::fixtures(Fixtures::FirstRun));
            s.offline = Some(e);
            s
        }
    }
}

impl App {
    pub fn new(core: Core) -> App {
        let changed = Arc::new(AtomicBool::new(false));
        let mut s = App {
            core,
            status: Status::default(),
            decks: Vec::new(),
            deck: None,
            cards: Vec::new(),
            review: None,
            editor: None,
            settings: None,
            new_deck: None,
            sidebar: PaneState::new(232.0).range(180.0, 360.0),
            first_run: false,
            error: None,
            changed: changed.clone(),
            offline: None,
        };
        s.refresh();
        s.first_run = s.decks.len() == 1 && s.decks[0].cards > 0 && s.decks[0].new == s.decks[0].cards;
        if s.first_run {
            s.deck = Some(s.decks[0].deck.id.clone());
            s.sidebar.set_open(false);
            s.load_cards();
        }
        s
    }

    /// Replaces the core (scenes and tests switch fixture stores) and starts over.
    pub fn with_core(&mut self, core: Core) {
        let sidebar = std::mem::replace(&mut self.sidebar, PaneState::new(0.0));
        *self = App::new(core);
        if !self.first_run {
            self.sidebar = sidebar;
            self.sidebar.set_open(true);
        }
    }

    pub fn watch(&self, wake: rune::Waker) {
        let changed = self.changed.clone();
        self.core.watch(move || {
            changed.store(true, Ordering::Release);
            wake();
        });
    }

    /// Refetches after a `changed` event. True when there was one.
    pub fn poll_changed(&mut self) -> bool {
        if !self.changed.swap(false, Ordering::AcqRel) {
            return false;
        }
        self.refresh();
        true
    }

    fn ok<T>(&mut self, r: crate::core::Result<T>) -> Option<T> {
        match r {
            Ok(v) => {
                self.error = None;
                Some(v)
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }

    pub fn refresh(&mut self) {
        if let Ok(status) = self.core.status() {
            self.status = status;
        }
        let decks = self.core.decks();
        if let Some(decks) = self.ok(decks) {
            self.decks = decks;
        }
        if self.deck.as_ref().is_some_and(|id| !self.decks.iter().any(|d| &d.deck.id == id)) {
            self.deck = None;
        }
        self.load_cards();
        self.refresh_review();
    }

    fn load_cards(&mut self) {
        let cards = self.core.cards(self.deck.as_deref());
        if let Some(cards) = self.ok(cards) {
            self.cards = cards;
        }
    }

    /// Keeps the card on screen if it still exists (picking up edits made elsewhere), and updates the counts.
    fn refresh_review(&mut self) {
        let Some(current) = self.review.as_ref().map(|r| r.card().map(|c| c.card.id.clone())) else { return };
        let next = self.core.next_review(self.deck.as_deref());
        let Some(mut next) = self.ok(next) else { return };
        if let Some(id) = current
            && let Ok(card) = self.core.card(&id)
        {
            next.card = Some(card);
        }
        let review = self.review.as_mut().expect("reviewing");
        if review.card().map(|c| &c.card.id) != next.card.as_ref().map(|c| &c.card.id) {
            review.shown = 1;
            review.started = Instant::now();
        }
        review.next = next;
    }

    pub fn deck_summary(&self) -> Option<&DeckSummary> {
        self.deck.as_ref().and_then(|id| self.decks.iter().find(|d| &d.deck.id == id))
    }

    pub fn deck_name(&self, id: &str) -> &str {
        self.decks.iter().find(|d| d.deck.id == id).map_or("", |d| d.deck.name.as_str())
    }

    /// Due and new cards in the main pane's deck (or all decks).
    pub fn counts(&self) -> (u64, u64) {
        match self.deck_summary() {
            Some(d) => (d.due, d.new),
            None => self.decks.iter().fold((0, 0), |(due, new), d| (due + d.due, new + d.new)),
        }
    }

    /// When the soonest card in the main pane's deck comes due, if it isn't due already.
    pub fn next_due(&self) -> Option<i64> {
        let now = now_ms();
        self.cards.iter().map(|c| c.schedule.due).filter(|&due| due > now).min()
    }

    pub fn select_deck(&mut self, deck: Option<Id>) {
        self.deck = deck;
        self.review = None;
        self.editor = None;
        self.settings = None;
        self.load_cards();
    }

    pub fn start_review(&mut self) {
        self.editor = None;
        self.settings = None;
        let next = self.core.next_review(self.deck.as_deref());
        if let Some(next) = self.ok(next) {
            self.review = Some(Review { next, shown: 1, started: Instant::now(), answered: 0, last: None });
        }
    }

    pub fn reveal(&mut self) {
        if let Some(r) = self.review.as_mut().filter(|r| !r.revealed()) {
            r.shown += 1;
        }
    }

    pub fn grade(&mut self, rating: u8) {
        let Some(review) = self.review.as_ref().filter(|r| r.revealed()) else { return };
        let card_id = review.card().expect("revealed").card.id.clone();
        let duration_ms = Some(review.started.elapsed().as_millis() as i64);
        let answered = self.core.answer(Answer { card_id, rating, duration_ms });
        let Some(card) = self.ok(answered) else { return };
        let next = self.core.next_review(self.deck.as_deref());
        let Some(next) = self.ok(next) else { return };
        let review = self.review.as_mut().expect("reviewing");
        review.next = next;
        review.shown = 1;
        review.started = Instant::now();
        review.answered += 1;
        review.last = Some(Answered { rating, due: card.schedule.due });
        let finished = review.card().is_none();
        if finished && self.first_run {
            // The first review is done: now the rest of the app can show itself.
            self.first_run = false;
            self.sidebar.set_open(true);
        }
        let decks = self.core.decks();
        if let Some(decks) = self.ok(decks) {
            self.decks = decks;
        }
        self.load_cards();
    }

    pub fn end_review(&mut self) {
        self.review = None;
        if self.first_run {
            self.first_run = false;
            self.sidebar.set_open(true);
        }
    }

    pub fn new_card(&mut self) {
        let deck = self.deck.clone().or_else(|| self.decks.first().map(|d| d.deck.id.clone()));
        let Some(deck) = deck else {
            self.error = Some("Add a deck first".into());
            return;
        };
        self.open_editor(None, String::new(), String::new(), deck);
    }

    pub fn edit_card(&mut self, id: &str) {
        let Some(c) = self.cards.iter().find(|c| c.card.id == id).cloned() else { return };
        self.open_editor(Some(c.card.id), c.card.content, c.card.tags.join(", "), c.card.deck_id);
    }

    fn open_editor(&mut self, card: Option<Id>, content: String, tags: String, deck: Id) {
        let mut content = TextEdit::new(content);
        content.set_caret(content.text.len());
        content.focused = true;
        self.settings = None;
        self.editor = Some(Editor {
            card,
            content,
            tags: TextEdit::new(tags),
            deck,
            picking_deck: false,
            confirm_delete: false,
            error: None,
        });
    }

    pub fn cancel_editor(&mut self) {
        self.editor = None;
    }

    pub fn save_editor(&mut self) {
        let Some(e) = self.editor.as_ref() else { return };
        let content = e.content.text.trim().to_string();
        if content.is_empty() {
            self.editor.as_mut().expect("editing").error = Some("Write the front of the card first".into());
            return;
        }
        let tags: Vec<String> =
            e.tags.text.split([',', ' ']).map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect();
        let deck = e.deck.clone();
        let saved = match &e.card {
            Some(id) => self.core.update_card(CardUpdate {
                id: id.clone(),
                content: Some(content),
                tags: Some(tags),
                deck_id: Some(deck),
            }),
            None => self.core.create_card(NewCard { deck_id: deck, content, tags }),
        };
        match saved {
            Ok(_) => {
                self.editor = None;
                self.refresh();
            }
            Err(err) => self.editor.as_mut().expect("editing").error = Some(err),
        }
    }

    pub fn delete_card(&mut self) {
        let Some(e) = self.editor.as_mut() else { return };
        let Some(id) = e.card.clone() else { return };
        if !e.confirm_delete {
            e.confirm_delete = true;
            return;
        }
        match self.core.delete_card(&id) {
            Ok(()) => {
                self.editor = None;
                self.refresh();
            }
            Err(err) => self.editor.as_mut().expect("editing").error = Some(err),
        }
    }

    pub fn start_new_deck(&mut self) {
        let mut edit = TextEdit::new("");
        edit.focused = true;
        self.new_deck = Some(edit);
    }

    pub fn commit_new_deck(&mut self) {
        let Some(edit) = self.new_deck.take() else { return };
        if edit.text.trim().is_empty() {
            return;
        }
        let created = self.core.create_deck(&edit.text);
        if let Some(deck) = self.ok(created) {
            self.refresh();
            self.select_deck(Some(deck.id));
        }
    }

    pub fn open_settings(&mut self) {
        self.editor = None;
        self.review = None;
        let server = self.status.server.clone().unwrap_or_default();
        self.settings = Some(Settings { server: TextEdit::new(server), error: None });
    }

    pub fn sign_in(&mut self) {
        let Some(settings) = self.settings.as_ref() else { return };
        let server = settings.server.text.trim().to_string();
        let result =
            if server.is_empty() { Err("Enter a server address".to_string()) } else { self.core.login(&server) };
        match result {
            Ok(_) => {
                if let Some(s) = self.settings.as_mut() {
                    s.error = None;
                }
                self.refresh();
            }
            Err(e) => {
                if let Some(s) = self.settings.as_mut() {
                    s.error = Some(e);
                }
            }
        }
    }

    pub fn sync_now(&mut self) {
        let result = self.core.sync_now();
        if let Err(e) = result
            && let Some(s) = self.settings.as_mut()
        {
            s.error = Some(e);
        }
        self.refresh();
    }
}
