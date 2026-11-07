//! The app's state and the actions that change it. Every action is a thin call into `Core` followed by a refetch;
//! nothing here decides what's due or when a card comes back.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rune::prefs::Prefs;
use rune::{FileDialogOptions, TextEdit, Tx};
use rune_kit::PaneState;

pub use crate::editor::Editor;

use crate::core::{
    Answer, CardSort, CardUpdate, CardView, Core, DeckSummary, Fixtures, HostStatus, Id, Imported, NewCard, NextReview,
    Order, PairTicket, Status, WebStatus, now_ms,
};

pub struct App {
    pub core: Core,
    pub status: Status,
    pub decks: Vec<DeckSummary>,
    /// The deck in the main pane. There's no view of every deck at once: this is `None` only when there are no decks.
    pub deck: Option<Id>,
    /// The cards of `deck` that match `query`, for its card views.
    pub cards: Vec<CardView>,
    /// How `deck`'s cards are laid out, remembered per deck in `prefs`.
    pub view: CardsView,
    /// The view switcher's menu is open.
    pub picking_view: bool,
    /// How `deck`'s cards are ordered, remembered per deck in `prefs` (all but `Random`'s seed).
    pub order: Order,
    /// The sort menu is open.
    pub picking_sort: bool,
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
    /// The sidebar's "+" menu (New deck, Import from Mochi) is open.
    pub deck_menu: bool,
    /// What the last import brought in, shown above the deck it opened until you move on.
    pub notice: Option<String>,
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
/// The preference behind [`App::separate_sides`].
const SEPARATE_SIDES: &str = "experiment.separate-sides";

/// Where the app keeps its preferences (`rune::prefs`).
const PREFS_ID: &str = "recall-desktop";

pub struct Review {
    /// The deck (with its subdecks) being reviewed, or `None` for the inbox: every due card from every deck.
    pub deck: Option<Id>,
    pub next: NextReview,
    /// How many of the current card's sides are showing (1 is just the front).
    pub shown: usize,
    pub started: Instant,
    /// Cards answered since the review started.
    pub answered: u32,
}

pub struct Settings {
    pub server: TextEdit,
    pub error: Option<String>,
    /// Web access: whether this app serves the library to browsers on the network, and where.
    pub web: WebStatus,
    pub web_error: Option<String>,
    /// The token is showing instead of dots.
    pub show_token: bool,
    /// Hosting: whether this app is the library's hub for paired phones, and which they are.
    pub host: HostStatus,
    pub host_error: Option<String>,
    /// The pairing ticket on screen, and how many devices were paired when it was made.
    pub ticket: Option<(PairTicket, usize)>,
    /// When `host` was last fetched (the app clock), to keep connection states fresh while Settings is open.
    pub host_at: f64,
}

/// How often Settings refetches the paired devices, whose connections come and go without a `changed` event.
const HOST_POLL: f64 = 3.0;

/// The answers, with their keys: whether you remembered the card.
pub const ANSWERS: [(bool, &str, &str); 2] = [(false, "Forgot", "1"), (true, "Remembered", "2")];

impl Review {
    pub fn card(&self) -> Option<&CardView> {
        self.next.card.as_ref()
    }

    /// Every side is showing.
    pub fn revealed(&self) -> bool {
        self.card().is_some_and(|c| self.shown >= c.sides.len())
    }

    pub fn inbox(&self) -> bool {
        self.deck.is_none()
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
            order: Order::default(),
            picking_sort: false,
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
            deck_menu: false,
            notice: None,
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
            self.order = self.saved_order();
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
        // Escape closes the tag menu until the field is next focused.
        if let Some(e) = self.editor.as_mut().filter(|e| !e.tag_draft.focused) {
            e.tag_menu_closed = false;
        }
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
        if let Some(at) = self.settings.as_ref().filter(|s| s.host.enabled).map(|s| s.host_at) {
            let left = at + HOST_POLL - clock;
            if left <= 0.0 {
                let before = self.settings.as_ref().map(|s| s.host.clone());
                self.refresh_host();
                changed |= self.settings.as_ref().map(|s| &s.host) != before.as_ref();
                tx.wake_after(Duration::from_secs_f64(HOST_POLL));
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
        self.refresh_host();
        let decks = self.core.decks();
        if let Some(decks) = self.ok(decks) {
            self.decks = decks;
        }
        // When the deck on screen is gone (or none was chosen yet), the first one takes its place.
        let shown = self.deck.as_ref().is_some_and(|id| self.decks.iter().any(|d| &d.deck.id == id));
        let first = self.deck_tree().first().map(|(d, _)| d.deck.id.clone());
        if !shown && first != self.deck {
            self.select_deck(first);
        } else {
            self.load_cards();
        }
        self.refresh_review();
    }

    fn load_cards(&mut self) {
        let cards = self.core.cards(self.deck.as_deref(), &self.query, self.order);
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
        let Some((deck, current)) = self.review.as_ref().map(|r| (r.deck.clone(), r.card().map(|c| c.card.id.clone())))
        else {
            return;
        };
        let next = self.core.next_review(deck.as_deref());
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

    /// A deck's name with its parents': "Languages / Spanish".
    pub fn deck_path(&self, id: &str) -> String {
        let mut names = Vec::new();
        let mut at = Some(id);
        while let Some(d) = at.and_then(|id| self.decks.iter().find(|d| d.deck.id == id)) {
            if names.len() > self.decks.len() {
                break;
            }
            names.push(d.deck.name.as_str());
            at = d.deck.parent_id.as_deref();
        }
        names.reverse();
        names.join(" / ")
    }

    /// Cards waiting in the inbox: every deck's due and new, counting each top deck once (its counts take in its
    /// subdecks').
    pub fn inbox_count(&self) -> u64 {
        let top =
            |d: &&DeckSummary| d.deck.parent_id.as_ref().is_none_or(|p| !self.decks.iter().any(|o| &o.deck.id == p));
        self.decks.iter().filter(top).map(|d| d.due + d.new).sum()
    }

    /// The inbox is in the main pane.
    pub fn in_inbox(&self) -> bool {
        self.settings.is_none() && self.review.as_ref().is_some_and(Review::inbox)
    }

    /// The decks as the sidebar lists them: each followed by its subdecks, with how deep it is.
    pub fn deck_tree(&self) -> Vec<(&DeckSummary, u8)> {
        deck_tree(&self.decks)
    }

    /// Due and new cards in the main pane's deck.
    pub fn counts(&self) -> (u64, u64) {
        self.deck_summary().map_or((0, 0), |d| (d.due, d.new))
    }

    /// When the soonest card in the main pane's deck comes due, if it isn't due already.
    pub fn next_due(&self) -> Option<i64> {
        let now = now_ms();
        self.cards.iter().map(|c| c.schedule.due).filter(|&due| due > now).min()
    }

    pub fn select_deck(&mut self, deck: Option<Id>) {
        self.deck = deck;
        self.notice = None;
        self.review = None;
        self.editor = None;
        self.settings = None;
        self.picking_view = false;
        self.picking_sort = false;
        self.search = TextEdit::new("");
        self.query.clear();
        self.search_due = None;
        self.selected = None;
        self.detail_focused = false;
        self.view = self.saved_view();
        self.order = self.saved_order();
        self.load_cards();
    }

    /// Experiment: each side of the card editor in a box of its own, rather than one box with rules between.
    pub fn separate_sides(&self) -> bool {
        self.prefs.get::<bool>(SEPARATE_SIDES).unwrap_or(false)
    }

    pub fn toggle_separate_sides(&mut self) {
        if let Err(e) = self.prefs.set(SEPARATE_SIDES, &!self.separate_sides()) {
            eprintln!("recall-desktop: {e}");
        }
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

    fn order_pref(&self) -> String {
        format!("sort.{}", self.deck.as_deref().unwrap_or("all"))
    }

    /// The deck's saved order, as `name` or `name-desc`; a saved `random` gets a fresh shuffle.
    fn saved_order(&self) -> Order {
        let saved = self.prefs.get::<String>(&self.order_pref()).unwrap_or_default();
        let (id, desc) = saved.strip_suffix("-desc").map_or((saved.as_str(), false), |id| (id, true));
        let by = CardSort::parse(id).unwrap_or_default();
        Order { by, desc, seed: if by == CardSort::Random { shuffle_seed() } else { 0 } }
    }

    /// Orders the deck's cards by `by`, starting the way that reads best for it (newest first for "Last updated"
    /// and "Last reviewed"), and remembers it for this deck. Choosing Random again shuffles again.
    pub fn set_sort(&mut self, by: CardSort) {
        let desc = if by == self.order.by { self.order.desc } else { sort_starts_desc(by) };
        let seed = if by == CardSort::Random { shuffle_seed() } else { 0 };
        self.set_order(Order { by, desc, seed });
    }

    /// Puts the deck's cards in `order` and remembers it for this deck.
    pub fn set_order(&mut self, order: Order) {
        self.picking_sort = false;
        if self.order == order {
            return;
        }
        self.order = order;
        let id = order.by.as_str();
        let value = if order.desc { format!("{id}-desc") } else { id.to_string() };
        let key = self.order_pref();
        if let Err(e) = self.prefs.set(&key, &value) {
            eprintln!("recall-desktop: {e}");
        }
        self.load_cards();
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

    /// Reviews the deck in the main pane, with its subdecks.
    pub fn start_review(&mut self) {
        self.review_deck(self.deck.clone());
    }

    /// The inbox: every due card from every deck, from the top of the sidebar.
    pub fn open_inbox(&mut self) {
        self.review_deck(None);
    }

    fn review_deck(&mut self, deck: Option<Id>) {
        self.editor = None;
        self.settings = None;
        let next = self.core.next_review(deck.as_deref());
        if let Some(next) = self.ok(next) {
            self.review = Some(Review { deck, next, shown: 1, started: Instant::now(), answered: 0 });
        }
    }

    pub fn reveal(&mut self) {
        if let Some(r) = self.review.as_mut().filter(|r| r.card().is_some() && !r.revealed()) {
            r.shown += 1;
        }
    }

    /// Answers the card on screen, at any side, and moves on to the next.
    pub fn answer(&mut self, remembered: bool) {
        let Some(review) = self.review.as_ref() else { return };
        let Some(card) = review.card() else { return };
        let card_id = card.card.id.clone();
        let deck = review.deck.clone();
        let duration_ms = Some(review.started.elapsed().as_millis() as i64);
        let answered = self.core.answer(Answer { card_id, remembered, duration_ms });
        if self.ok(answered).is_none() {
            return;
        }
        let next = self.core.next_review(deck.as_deref());
        let Some(next) = self.ok(next) else { return };
        let review = self.review.as_mut().expect("reviewing");
        review.next = next;
        review.shown = 1;
        review.started = Instant::now();
        review.answered += 1;
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
        let Some(deck) = self.deck.clone() else {
            self.error = Some("Add a deck first".into());
            return;
        };
        self.open_editor(Editor::new(None, "", Vec::new(), deck));
    }

    pub fn edit_card(&mut self, id: &str) {
        let Some(c) = self.cards.iter().find(|c| c.card.id == id).cloned() else { return };
        self.search.focused = false;
        self.open_editor(Editor::new(Some(c.card.id), &c.card.content, c.card.tags, c.card.deck_id));
    }

    fn open_editor(&mut self, editor: Editor) {
        self.settings = None;
        self.editor = Some(editor);
        self.load_editor_tags();
    }

    /// What the tag menu suggests from: the editor's deck's tags first, then the library's. Again whenever the card
    /// moves to another deck. Without them the menu just has nothing to offer; typing tags still works.
    pub fn load_editor_tags(&mut self) {
        let Some(deck) = self.editor.as_ref().map(|e| e.deck.clone()) else { return };
        let tags = match self.core.tags(&deck) {
            Ok(tags) => tags.into_iter().map(|t| t.tag).collect(),
            Err(e) => {
                eprintln!("recall-desktop: no tag suggestions: {e}");
                Vec::new()
            }
        };
        if let Some(e) = self.editor.as_mut() {
            e.known_tags = tags;
            e.tag_active = None;
        }
    }

    pub fn cancel_editor(&mut self) {
        self.editor = None;
    }

    pub fn save_editor(&mut self) {
        let Some(e) = self.editor.as_ref() else { return };
        let content = e.content();
        if content.is_empty() {
            self.editor.as_mut().expect("editing").error = Some("Write something on the card first".into());
            return;
        }
        let tags = e.saved_tags();
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
        self.deck_menu = false;
        self.sidebar.set_open(true);
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

    /// Asks for a Mochi export, then imports it.
    pub fn choose_mochi_export(&mut self, tx: &mut Tx) {
        self.deck_menu = false;
        let options = FileDialogOptions { title: "Import from Mochi".into(), ..FileDialogOptions::default() }
            .filter("Mochi export", &["mochi"]);
        tx.open_file(options, |s: &mut App, chosen, _| match chosen {
            Ok(Some(path)) => s.import_mochi(&path),
            Ok(None) => {}
            Err(e) => s.error = Some(e.to_string()),
        });
    }

    /// Imports a Mochi export and opens the first deck it added.
    pub fn import_mochi(&mut self, path: &Path) {
        let imported = self.core.import_mochi(path);
        let Some(imported) = self.ok(imported) else { return };
        self.refresh();
        let top = imported
            .decks
            .iter()
            .find(|d| d.parent_id.as_ref().is_none_or(|p| !imported.decks.iter().any(|o| &o.id == p)));
        if let Some(deck) = top {
            self.select_deck(Some(deck.id.clone()));
        }
        self.notice = Some(import_summary(&imported));
    }

    pub fn open_settings(&mut self) {
        self.editor = None;
        self.review = None;
        let server = self.status.server.clone().unwrap_or_default();
        let (web, web_error) = match self.core.web_status() {
            Ok(web) => (web, None),
            Err(e) => (WebStatus::default(), Some(e)),
        };
        let (host, host_error) = match self.core.host_status() {
            Ok(host) => (host, None),
            Err(e) => (HostStatus::default(), Some(e)),
        };
        self.settings = Some(Settings {
            server: TextEdit::new(server),
            error: None,
            web,
            web_error,
            show_token: false,
            host,
            host_error,
            ticket: None,
            host_at: self.clock,
        });
    }

    /// Refetches the paired devices for Settings. A ticket on screen goes once a new device has paired with it.
    fn refresh_host(&mut self) {
        let Some(settings) = self.settings.as_ref() else { return };
        if !settings.host.enabled && settings.ticket.is_none() {
            return;
        }
        let host = self.core.host_status();
        let clock = self.clock;
        let Some(s) = self.settings.as_mut() else { return };
        s.host_at = clock;
        let Ok(host) = host else { return };
        if let Some((_, paired)) = &s.ticket
            && host.devices.len() > *paired
        {
            s.ticket = None;
            if let Some(device) = host.devices.iter().max_by_key(|d| d.device.added_at) {
                self.notice = Some(format!("Paired {}", device.device.name));
            }
        }
        s.host = host;
    }

    /// Turns hosting phones on or off from Settings.
    pub fn toggle_host(&mut self) {
        let Some(on) = self.settings.as_ref().map(|s| !s.host.enabled) else { return };
        let result = self.core.set_host(on);
        let host = self.core.host_status();
        if let Some(s) = self.settings.as_mut() {
            s.host_error = result.err();
            if !on {
                s.ticket = None;
            }
            if let Ok(host) = host {
                s.host = host;
            }
        }
    }

    /// Shows a new pairing ticket for a phone to scan.
    pub fn pair_phone(&mut self) {
        let ticket = self.core.host_pair();
        let host = self.core.host_status();
        let Some(s) = self.settings.as_mut() else { return };
        if let Ok(host) = host {
            s.host = host;
        }
        match ticket {
            Ok(ticket) => {
                s.ticket = Some((ticket, s.host.devices.len()));
                s.host_error = None;
            }
            Err(e) => s.host_error = Some(e),
        }
    }

    pub fn hide_ticket(&mut self) {
        if let Some(s) = self.settings.as_mut() {
            s.ticket = None;
        }
    }

    /// Unpairs a phone. It keeps its decks, and stops syncing with this computer.
    pub fn remove_device(&mut self, id: &str) {
        let result = self.core.host_remove(id);
        let host = self.core.host_status();
        if let Some(s) = self.settings.as_mut() {
            s.host_error = result.err();
            if let Ok(host) = host {
                s.host = host;
            }
        }
    }

    /// Turns web access on or off from Settings.
    pub fn toggle_web(&mut self) {
        let Some(on) = self.settings.as_ref().map(|s| !s.web.enabled) else { return };
        let result = self.core.set_web(on);
        // A failed start (the port is taken) still turned it on: show where it stands and why.
        let web = self.core.web_status();
        if let Some(s) = self.settings.as_mut() {
            s.web_error = result.err();
            if let Ok(web) = web {
                s.web = web;
            }
        }
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

/// "Imported 2 decks and 140 cards from Mochi.", and what was left behind.
pub fn import_summary(i: &Imported) -> String {
    let n = |count: u64, one: &str| if count == 1 { format!("1 {one}") } else { format!("{count} {one}s") };
    let mut out = format!("Imported {} and {} from Mochi.", n(i.decks.len() as u64, "deck"), n(i.cards, "card"));
    if i.skipped > 0 {
        out.push_str(&format!(" {} in the trash or empty stayed behind.", n(i.skipped, "card")));
    }
    if i.attachments > 0 {
        out.push_str(&format!(" Images and audio aren’t imported yet ({}).", n(i.attachments, "file")));
    }
    out
}

/// Each deck followed by its subdecks, alphabetical at every level (`decks` already is), with its depth. A deck whose
/// parent isn't in the list is at the top.
fn deck_tree(decks: &[DeckSummary]) -> Vec<(&DeckSummary, u8)> {
    fn visit<'a>(decks: &'a [DeckSummary], parent: &str, depth: u8, out: &mut Vec<(&'a DeckSummary, u8)>) {
        for d in decks.iter().filter(|d| d.deck.parent_id.as_deref() == Some(parent)) {
            if out.iter().any(|(o, _)| o.deck.id == d.deck.id) {
                continue;
            }
            out.push((d, depth));
            visit(decks, &d.deck.id, depth.saturating_add(1), out);
        }
    }
    let mut out = Vec::with_capacity(decks.len());
    let top = |d: &&DeckSummary| d.deck.parent_id.as_ref().is_none_or(|p| !decks.iter().any(|o| &o.deck.id == p));
    for d in decks.iter().filter(top) {
        out.push((d, 0));
        visit(decks, &d.deck.id, 1, &mut out);
    }
    out
}

/// Mochi's name for each sort, and what its two directions read as (`None` for Random, which has none).
pub fn sort_label(by: CardSort) -> &'static str {
    match by {
        CardSort::Created => "Date created",
        CardSort::Updated => "Last updated",
        CardSort::Name => "Alphanumeric",
        CardSort::Due => "Due date",
        CardSort::Reviewed => "Last reviewed",
        CardSort::Interval => "Last interval",
        CardSort::Retention => "Retention rate",
        CardSort::Random => "Random",
    }
}

/// The sort button's name for `by`: one word, so the button barely changes width.
pub fn sort_short_label(by: CardSort) -> &'static str {
    match by {
        CardSort::Created => "Created",
        CardSort::Updated => "Updated",
        CardSort::Name => "Name",
        CardSort::Due => "Due",
        CardSort::Reviewed => "Reviewed",
        CardSort::Interval => "Interval",
        CardSort::Retention => "Retention",
        CardSort::Random => "Random",
    }
}

/// What ascending and descending read as for `by`.
pub fn sort_directions(by: CardSort) -> Option<(&'static str, &'static str)> {
    Some(match by {
        CardSort::Created | CardSort::Updated | CardSort::Reviewed => ("Oldest first", "Newest first"),
        CardSort::Name => ("A to Z", "Z to A"),
        CardSort::Due => ("Soonest first", "Latest first"),
        CardSort::Interval => ("Shortest first", "Longest first"),
        CardSort::Retention => ("Lowest first", "Highest first"),
        CardSort::Random => return None,
    })
}

fn sort_starts_desc(by: CardSort) -> bool {
    matches!(by, CardSort::Updated | CardSort::Reviewed)
}

/// A new seed for Random, different each time it's asked for.
fn shuffle_seed() -> u32 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    (nanos as u32) ^ ((nanos >> 32) as u32)
}

/// The narrowest a grid tile gets before the grid drops a column.
const TILE_MIN: f32 = 280.0;
pub const GRID_GAP: f32 = 12.0;

/// Columns of at least `TILE_MIN` that fit in `width`, from one to eight; three before the width is known. Wide
/// windows get more columns rather than wider tiles.
pub fn grid_columns(width: f32) -> usize {
    if width <= 0.0 {
        return 3;
    }
    (((width + GRID_GAP) / (TILE_MIN + GRID_GAP)).floor() as usize).clamp(1, 8)
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
        assert_eq!(grid_columns(1000.0), 3);
        assert_eq!(grid_columns(720.0), 2);
        assert_eq!(grid_columns(420.0), 1);
        assert_eq!(grid_columns(1300.0), 4);
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
    fn the_tag_menu_suggests_the_editors_deck_first() {
        let mut s = App::new(Core::fixtures(Fixtures::LivedIn));
        let id = |s: &App, name: &str| s.decks.iter().find(|d| d.deck.name == name).map(|d| d.deck.id.clone());
        s.select_deck(id(&s, "Rust"));
        s.new_card();
        let suggested = |s: &App| s.editor.as_ref().unwrap().known_tags.clone();
        assert_eq!(suggested(&s)[..2], ["programming", "rust"], "Rust's tags first, though Spanish's are newer");
        assert!(suggested(&s).contains(&"vocab".to_string()), "then the rest of the library");

        s.editor.as_mut().unwrap().deck = id(&s, "Spanish").unwrap();
        s.load_editor_tags();
        let spanish = suggested(&s);
        let rust_at = spanish.iter().position(|t| t == "rust").unwrap();
        assert!(spanish[..rust_at].contains(&"vocab".to_string()), "moving the card moves its deck's tags up");
    }

    #[test]
    fn views_are_remembered_per_deck() {
        let mut s = App::new(Core::fixtures(Fixtures::LivedIn));
        let spanish = s.decks.iter().find(|d| d.deck.name == "Spanish").map(|d| d.deck.id.clone());
        let other = s.decks.iter().find(|d| d.deck.name != "Spanish").map(|d| d.deck.id.clone());
        s.select_deck(spanish.clone());
        s.set_view(CardsView::Column);
        s.select_deck(other);
        assert_eq!(s.view, CardsView::Grid);
        s.select_deck(spanish);
        assert_eq!(s.view, CardsView::Column);
    }

    #[test]
    fn sorts_are_remembered_per_deck_and_start_the_natural_way() {
        let mut s = App::new(Core::fixtures(Fixtures::LivedIn));
        let spanish = s.decks.iter().find(|d| d.deck.name == "Spanish").map(|d| d.deck.id.clone());
        let other = s.decks.iter().find(|d| d.deck.name != "Spanish").map(|d| d.deck.id.clone());
        s.select_deck(spanish.clone());
        s.set_sort(CardSort::Name);
        let fronts: Vec<String> = s.cards.iter().map(|c| recall_core::sort::title_key(&c.sides[0])).collect();
        assert_eq!(fronts.first().map(String::as_str), Some("aprovechar"));
        assert!(fronts.windows(2).all(|w| w[0] <= w[1]), "{fronts:?}");
        s.set_order(Order { desc: true, ..s.order });
        s.set_sort(CardSort::Updated);
        assert!(s.order.desc, "Last updated starts newest first");
        s.set_sort(CardSort::Name);
        s.select_deck(other);
        assert_eq!(s.order, Order::default());
        s.select_deck(spanish);
        assert_eq!((s.order.by, s.order.desc), (CardSort::Name, false));
    }

    #[test]
    fn never_reviewed_cards_stay_last() {
        let mut s = App::new(Core::fixtures(Fixtures::LivedIn));
        let spanish = s.decks.iter().find(|d| d.deck.name == "Spanish").map(|d| d.deck.id.clone());
        s.select_deck(spanish);
        for desc in [false, true] {
            s.set_order(Order { by: CardSort::Retention, desc, seed: 0 });
            let seen: Vec<bool> = s.cards.iter().map(|c| c.schedule.reps > 0).collect();
            assert!(seen.windows(2).all(|w| w[0] || !w[1]), "{seen:?}");
        }
    }

    #[test]
    fn a_deck_is_always_open() {
        let s = App::new(Core::fixtures(Fixtures::LivedIn));
        let first = s.deck_tree().first().map(|(d, _)| d.deck.id.clone());
        assert!(s.deck.is_some() && s.deck == first);
        assert_eq!(s.counts(), s.deck_summary().map(|d| (d.due, d.new)).unwrap());
    }

    #[test]
    fn subdecks_follow_their_deck() {
        let deck = |id: &str, name: &str, parent: Option<&str>| DeckSummary {
            deck: crate::core::Deck {
                id: id.into(),
                name: name.into(),
                parent_id: parent.map(String::from),
                created_at: 0,
                updated_at: 0,
                deleted: false,
            },
            cards: 0,
            due: 0,
            new: 0,
        };
        // As the core sorts them: by name.
        let decks = [
            deck("v", "Verbs", Some("s")),
            deck("a", "Arabic", None),
            deck("i", "Irregular", Some("v")),
            deck("s", "Spanish", None),
            deck("o", "Orphan", Some("gone")),
        ];
        let tree: Vec<(&str, u8)> = deck_tree(&decks).iter().map(|(d, depth)| (d.deck.name.as_str(), *depth)).collect();
        assert_eq!(tree, [("Arabic", 0), ("Spanish", 0), ("Verbs", 1), ("Irregular", 2), ("Orphan", 0)]);
    }

    #[test]
    fn an_import_says_what_it_left_behind() {
        let imported = Imported { decks: vec![], cards: 1, reviews: 0, skipped: 2, attachments: 3 };
        assert_eq!(
            import_summary(&imported),
            "Imported 0 decks and 1 card from Mochi. 2 cards in the trash or empty stayed behind. \
             Images and audio aren’t imported yet (3 files)."
        );
    }

    #[test]
    fn importing_on_sample_data_says_why_not() {
        let mut s = App::new(Core::fixtures(Fixtures::LivedIn));
        s.import_mochi(Path::new("/tmp/deck.mochi"));
        assert_eq!(s.error.as_deref(), Some("Importing isn’t available on sample data"));
    }

    #[test]
    fn the_inbox_reviews_every_deck_and_answers_at_any_side() {
        let mut s = App::new(Core::fixtures(Fixtures::LivedIn));
        let waiting = s.inbox_count();
        s.open_inbox();
        assert!(s.in_inbox());
        let r = s.review.as_ref().unwrap();
        assert_eq!(r.next.remaining.due + r.next.remaining.new, waiting);
        let first = r.card().unwrap().card.id.clone();
        assert!(!r.revealed());
        s.answer(true);
        let r = s.review.as_ref().unwrap();
        assert_eq!(r.answered, 1, "answered from the front");
        assert_ne!(r.card().map(|c| c.card.id.clone()), Some(first));
        assert_eq!(s.inbox_count(), waiting - 1);
        let deck = s.decks[0].deck.id.clone();
        s.select_deck(Some(deck));
        assert!(!s.in_inbox());
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
