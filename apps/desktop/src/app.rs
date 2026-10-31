//! The app's state and the actions that change it. Every action is a thin call into `Core` followed by a refetch;
//! nothing here decides what's due or when a card comes back.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rune::prefs::Prefs;
use rune::{TextEdit, Tx};
use rune_kit::PaneState;

use crate::core::{Answer, CardUpdate, CardView, Core, DeckSummary, Fixtures, Id, NewCard, NextReview, Status, now_ms};

pub struct App {
    pub core: Core,
    pub status: Status,
    pub decks: Vec<DeckSummary>,
    /// The deck in the main pane; `None` is "All decks".
    pub deck: Option<Id>,
    /// The cards of `deck` that match `query`, for its card views.
    pub cards: Vec<CardView>,
    /// How `deck`'s cards are laid out, remembered per deck in `prefs`.
    pub view: CardsView,
    /// The view switcher's menu is open.
    pub picking_view: bool,
    /// The search field above the cards.
    pub search: TextEdit,
    /// The search the card list shows: `search` becomes it after a pause in typing.
    pub query: String,
    /// When `search` is applied, on the `poll` clock.
    search_due: Option<f64>,
    /// The highlighted card, moved with the arrow keys.
    pub selected: Option<Id>,
    /// In the column view, Return has moved from the list to the selected card's detail.
    pub detail_focused: bool,
    /// The window has keyboard focus; the selection turns neutral while it doesn't.
    pub window_focused: bool,
    /// The card area's width, for the grid's column count.
    pub cards_width: f32,
    /// Local UI preferences (each deck's view). Not library data, so not the core's.
    pub prefs: Prefs,
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
    /// While the core connection is being made: when it started, on the `poll` clock (`None` until the first poll).
    pub connecting: Option<Option<f64>>,
    /// The connection has taken long enough that the window says so.
    pub slow_connect: bool,
    /// The last `poll` clock, in seconds.
    clock: f64,
    waker: Option<rune::Waker>,
}

/// How a deck's cards are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CardsView {
    #[default]
    Grid,
    List,
    Column,
}

impl CardsView {
    pub const ALL: [CardsView; 3] = [CardsView::Grid, CardsView::List, CardsView::Column];

    pub fn label(self) -> &'static str {
        match self {
            CardsView::Grid => "Grid view",
            CardsView::List => "List view",
            CardsView::Column => "Column view",
        }
    }

    /// The menu's shortcut, as shown, and the digit that goes with ⌘.
    pub fn shortcut(self) -> (&'static str, char) {
        match self {
            CardsView::Grid => ("⌘1", '1'),
            CardsView::List => ("⌘2", '2'),
            CardsView::Column => ("⌘3", '3'),
        }
    }

    fn id(self) -> &'static str {
        match self {
            CardsView::Grid => "grid",
            CardsView::List => "list",
            CardsView::Column => "column",
        }
    }

    fn from_id(id: &str) -> Option<CardsView> {
        CardsView::ALL.into_iter().find(|v| v.id() == id)
    }
}

/// How long typing in the search field pauses before the list is asked again.
const SEARCH_DEBOUNCE: f64 = 0.15;
/// A connection that takes longer than this says so; a quicker one shows nothing in between.
const SLOW_CONNECT: f64 = 0.4;
/// Where the app keeps its preferences (`rune::prefs`).
const PREFS_ID: &str = "recall-desktop";

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

/// The first state, built before the first frame, so it never waits on the core: unless `--fixtures` is given, the
/// window opens connecting and `main` connects on another thread once the first frame is up (`App::connected`).
pub fn init() -> App {
    match fixtures_arg() {
        Some(which) => App::new(Core::fixtures(which)),
        None => App::connecting(),
    }
}

/// Whether this run connects to the core (rather than running on fixtures).
pub fn wants_core() -> bool {
    fixtures_arg().is_none()
}

impl App {
    pub fn new(core: Core) -> App {
        let mut s = App::empty(core, Prefs::memory());
        s.start();
        s
    }

    /// Waiting for the core: an empty window with the sidebar where it usually is, so nothing moves when it lands.
    pub fn connecting() -> App {
        let mut s = App::empty(Core::Pending, Prefs::open(PREFS_ID));
        s.connecting = Some(None);
        s
    }

    fn empty(core: Core, prefs: Prefs) -> App {
        App {
            core,
            status: Status::default(),
            decks: Vec::new(),
            deck: None,
            cards: Vec::new(),
            view: CardsView::default(),
            picking_view: false,
            search: TextEdit::new(""),
            query: String::new(),
            search_due: None,
            selected: None,
            detail_focused: false,
            window_focused: true,
            cards_width: 0.0,
            prefs,
            review: None,
            editor: None,
            settings: None,
            new_deck: None,
            sidebar: PaneState::new(232.0).range(180.0, 360.0),
            first_run: false,
            error: None,
            changed: Arc::new(AtomicBool::new(false)),
            offline: None,
            connecting: None,
            slow_connect: false,
            clock: 0.0,
            waker: None,
        }
    }

    /// Loads everything from the core and decides whether this is the first run.
    fn start(&mut self) {
        self.refresh();
        self.first_run = self.decks.len() == 1 && self.decks[0].cards > 0 && self.decks[0].new == self.decks[0].cards;
        if self.first_run {
            self.deck = Some(self.decks[0].deck.id.clone());
            self.sidebar.set_open(false);
            self.load_cards();
        }
        self.view = self.saved_view();
    }

    /// The connection made on another thread has landed: the core, or why there isn't one (then sample data).
    pub fn connected(&mut self, core: crate::core::Result<Core>) {
        self.connecting = None;
        self.slow_connect = false;
        match core {
            Ok(core) => self.core = core,
            Err(e) => {
                self.core = Core::fixtures(Fixtures::FirstRun);
                self.offline = Some(e);
            }
        }
        self.start();
        if let Some(wake) = self.waker.clone() {
            self.watch(wake);
        }
    }

    /// Before each frame: lands a paused search and says when a slow connection has been slow long enough to show.
    /// True when the view needs to run again.
    pub fn tick(&mut self, clock: f64, tx: &mut Tx) -> bool {
        self.clock = clock;
        let mut changed = false;
        if let Some(started) = self.connecting {
            let started = started.unwrap_or(clock);
            self.connecting = Some(Some(started));
            if !self.slow_connect {
                let left = started + SLOW_CONNECT - clock;
                if left <= 0.0 {
                    self.slow_connect = true;
                    changed = true;
                } else {
                    tx.wake_after(Duration::from_secs_f64(left));
                }
            }
        }
        if let Some(due) = self.search_due {
            let left = due - clock;
            if left <= 0.0 {
                self.search_due = None;
                if self.apply_search() {
                    // A new set of results replaces the old at once; nothing slides.
                    tx.instant();
                    changed = true;
                }
            } else {
                tx.wake_after(Duration::from_secs_f64(left));
            }
        }
        changed || self.poll_changed()
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

    /// Keeps the waker, for the connection that lands later, and listens for the core's changes.
    pub fn set_waker(&mut self, wake: rune::Waker) {
        self.waker = Some(wake.clone());
        self.watch(wake);
    }

    fn watch(&self, wake: rune::Waker) {
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
        if self.core.is_pending() {
            return;
        }
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
        let cards = self.core.cards(self.deck.as_deref(), &self.query);
        if let Some(cards) = self.ok(cards) {
            self.cards = cards;
        }
        if self.selected.as_ref().is_some_and(|id| !self.cards.iter().any(|c| &c.card.id == id)) {
            self.selected = None;
            self.detail_focused = false;
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
        self.picking_view = false;
        self.search = TextEdit::new("");
        self.query.clear();
        self.search_due = None;
        self.selected = None;
        self.detail_focused = false;
        self.view = self.saved_view();
        self.load_cards();
    }

    fn view_pref(&self) -> String {
        format!("view.{}", self.deck.as_deref().unwrap_or("all"))
    }

    fn saved_view(&self) -> CardsView {
        self.prefs.get::<String>(&self.view_pref()).and_then(|id| CardsView::from_id(&id)).unwrap_or_default()
    }

    /// Lays the deck's cards out another way, and remembers it for this deck.
    pub fn set_view(&mut self, view: CardsView) {
        self.picking_view = false;
        self.detail_focused = false;
        if self.view == view {
            return;
        }
        self.view = view;
        let key = self.view_pref();
        if let Err(e) = self.prefs.set(&key, view.id()) {
            eprintln!("recall-desktop: {e}");
        }
    }

    /// The search field changed: the list is asked again once typing pauses.
    pub fn search_edited(&mut self, tx: &mut Tx) {
        self.search_due = Some(self.clock + SEARCH_DEBOUNCE);
        tx.wake_after(Duration::from_secs_f64(SEARCH_DEBOUNCE));
    }

    /// Asks for the cards matching the search field now. True when the search changed.
    pub fn apply_search(&mut self) -> bool {
        self.search_due = None;
        if self.query == self.search.text {
            return false;
        }
        self.query = self.search.text.clone();
        self.load_cards();
        true
    }

    pub fn focus_search(&mut self) {
        self.search.focused = true;
        self.search.set_selection(0, self.search.text.len());
    }

    /// Escape in the search field: clears it, or leaves it when it's already empty.
    pub fn leave_search(&mut self) {
        if self.search.text.is_empty() {
            self.search.focused = false;
            return;
        }
        self.search.replace_text("");
        self.search_due = None;
        if !self.query.is_empty() {
            self.query.clear();
            self.load_cards();
        }
    }

    /// The selected card, or in the column view the first card when none is: the detail always shows one.
    pub fn current(&self) -> Option<&CardView> {
        let selected = self.selected.as_ref().and_then(|id| self.cards.iter().find(|c| &c.card.id == id));
        match self.view {
            CardsView::Column => selected.or(self.cards.first()),
            _ => selected,
        }
    }

    fn current_index(&self) -> Option<usize> {
        let id = &self.current()?.card.id;
        self.cards.iter().position(|c| &c.card.id == id)
    }

    /// Selects a card by a click: Grid and List open it, Column shows it.
    pub fn press_card(&mut self, id: &str) {
        self.selected = Some(id.to_string());
        self.detail_focused = false;
        if self.view != CardsView::Column {
            self.edit_card(id);
        }
    }

    /// How many tiles the grid fits in a row, the "New card" tile included.
    pub fn grid_columns(&self) -> usize {
        grid_columns(self.cards_width)
    }

    /// Moves the selection by an arrow key; in the grid, by its rows and columns. Returns the card now selected.
    pub fn move_selection(&mut self, dx: i32, dy: i32) -> Option<Id> {
        let n = self.cards.len();
        if n == 0 {
            return None;
        }
        let next = match self.current_index() {
            None => 0,
            Some(i) => match self.view {
                CardsView::Grid => grid_step(i, n, self.grid_columns(), dx, dy),
                _ => {
                    (i as i64 + i64::from(dy) + i64::from(if dy == 0 { dx } else { 0 })).clamp(0, n as i64 - 1) as usize
                }
            },
        };
        let id = self.cards[next].card.id.clone();
        self.selected = Some(id.clone());
        self.detail_focused = false;
        Some(id)
    }

    /// Return on the selection: the editor in Grid and List; in Column, the detail first, then the editor.
    pub fn open_selected(&mut self) -> bool {
        let Some(id) = self.current().map(|c| c.card.id.clone()) else { return false };
        if self.view == CardsView::Column && !self.detail_focused {
            self.selected = Some(id);
            self.detail_focused = true;
            return true;
        }
        self.edit_card(&id);
        true
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
        self.search.focused = false;
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

/// The narrowest a grid tile gets before the grid drops a column.
const TILE_MIN: f32 = 200.0;
pub const GRID_GAP: f32 = 12.0;

/// Columns of at least `TILE_MIN` that fit in `width`, from one to four; three before the width is known.
pub fn grid_columns(width: f32) -> usize {
    if width <= 0.0 {
        return 3;
    }
    (((width + GRID_GAP) / (TILE_MIN + GRID_GAP)).floor() as usize).clamp(1, 4)
}

/// Card `i` of `n` moved by an arrow in a grid of `cols` columns whose first cell is the "New card" tile.
fn grid_step(i: usize, n: usize, cols: usize, dx: i32, dy: i32) -> usize {
    let (pos, last) = (i as i64 + 1, n as i64);
    let cols = cols as i64;
    let to = if dy == 0 {
        pos + i64::from(dx)
    } else {
        let below = pos + i64::from(dy) * cols;
        // Down from a row above the last one's end lands on the last card, as in Finder.
        if below > last && pos / cols < last / cols { last } else { below }
    };
    if to < 1 || to > last { i } else { to as usize - 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_columns_follow_the_width() {
        assert_eq!(grid_columns(0.0), 3);
        assert_eq!(grid_columns(720.0), 3);
        assert_eq!(grid_columns(420.0), 2);
        assert_eq!(grid_columns(150.0), 1);
    }

    #[test]
    fn grid_arrows_move_by_cell() {
        // Three columns: [new, 0, 1], [2, 3, 4], [5, 6].
        assert_eq!(grid_step(0, 7, 3, 1, 0), 1);
        assert_eq!(grid_step(0, 7, 3, -1, 0), 0, "the New card tile isn't a card");
        assert_eq!(grid_step(0, 7, 3, 0, 1), 3);
        assert_eq!(grid_step(3, 7, 3, 0, -1), 0);
        assert_eq!(grid_step(2, 7, 3, 0, -1), 2, "nothing above but the New card tile");
        assert_eq!(grid_step(4, 7, 3, 0, 1), 6, "down past the end lands on the last card");
        assert_eq!(grid_step(6, 7, 3, 0, 1), 6);
    }

    #[test]
    fn views_are_remembered_per_deck() {
        let mut s = App::new(Core::fixtures(Fixtures::LivedIn));
        let spanish = s.decks.iter().find(|d| d.deck.name == "Spanish").map(|d| d.deck.id.clone());
        s.select_deck(spanish.clone());
        s.set_view(CardsView::Column);
        s.select_deck(None);
        assert_eq!(s.view, CardsView::Grid);
        s.select_deck(spanish);
        assert_eq!(s.view, CardsView::Column);
    }

    #[test]
    fn column_return_shows_the_detail_then_edits() {
        let mut s = App::new(Core::fixtures(Fixtures::LivedIn));
        s.set_view(CardsView::Column);
        assert!(s.open_selected());
        assert!(s.detail_focused && s.editor.is_none());
        assert!(s.open_selected());
        assert!(s.editor.is_some());
    }
}
