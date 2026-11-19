// The app's icons, loaded from the shared `assets/icons/*.svg` (the same set the web and iOS apps draw), so all
// three render the same glyphs. Rune rasterises the `<path>` inside each file; the `<svg>` wrapper is skipped.
// `view` is the SVG's viewBox size (20). One icon stays hand-written: IMPORT, the import tray, which only the desktop
// draws.
use rune::Icon;

pub static PLUS: Icon = Icon {
    name: "furiten-plus",
    view: 20.0,
    body: include_str!("../../../../assets/icons/plus.svg"),
};

/// A tray with an arrow coming down into it: bringing decks in from elsewhere.
pub static IMPORT: Icon = Icon {
    name: "furiten-import",
    view: 16.0,
    body: r#"<path d="M8 2.75V9.25M5.5 6.75L8 9.25L10.5 6.75" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/><path d="M3 9.5V11.5C3 12.33 3.67 13 4.5 13H11.5C12.33 13 13 12.33 13 11.5V9.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" fill="none"/>"#,
};

/// A closed book with a title line on its cover, as the web app draws a deck.
pub static DECK: Icon = Icon {
    name: "furiten-deck",
    view: 20.0,
    body: include_str!("../../../../assets/icons/deck.svg"),
};

/// A tray: every due card from every deck, the sidebar's Review.
pub static INBOX: Icon = Icon {
    name: "furiten-inbox",
    view: 20.0,
    body: include_str!("../../../../assets/icons/inbox.svg"),
};

/// Ascending: the sort direction button.
pub static UP: Icon = Icon {
    name: "furiten-up",
    view: 20.0,
    body: include_str!("../../../../assets/icons/up.svg"),
};

/// A clock face: when the review day rolls over.
pub static CLOCK: Icon = Icon {
    name: "furiten-clock",
    view: 20.0,
    body: include_str!("../../../../assets/icons/clock.svg"),
};

pub static DOWN: Icon = Icon {
    name: "furiten-down",
    view: 20.0,
    body: include_str!("../../../../assets/icons/down.svg"),
};

/// Random's direction button: shuffle again.
pub static SHUFFLE: Icon = Icon {
    name: "furiten-shuffle",
    view: 20.0,
    body: include_str!("../../../../assets/icons/shuffle.svg"),
};

/// Forgot (the web's `close` cross).
pub static CROSS: Icon = Icon {
    name: "furiten-cross",
    view: 20.0,
    body: include_str!("../../../../assets/icons/close.svg"),
};

pub static CLOUD: Icon = Icon {
    name: "furiten-cloud",
    view: 20.0,
    body: include_str!("../../../../assets/icons/cloud.svg"),
};

pub static GRID: Icon = Icon {
    name: "furiten-grid",
    view: 20.0,
    body: include_str!("../../../../assets/icons/grid.svg"),
};

pub static LIST: Icon = Icon {
    name: "furiten-list",
    view: 20.0,
    body: include_str!("../../../../assets/icons/list.svg"),
};

pub static COLUMN: Icon = Icon {
    name: "furiten-column",
    view: 20.0,
    body: include_str!("../../../../assets/icons/column.svg"),
};

/// An arrow up and an arrow down side by side: ordering.
pub static SORT: Icon = Icon {
    name: "furiten-sort",
    view: 20.0,
    body: include_str!("../../../../assets/icons/sort.svg"),
};

pub static PENCIL: Icon = Icon {
    name: "furiten-pencil",
    view: 20.0,
    body: include_str!("../../../../assets/icons/edit.svg"),
};

pub static CHECK: Icon = Icon {
    name: "furiten-check",
    view: 20.0,
    body: include_str!("../../../../assets/icons/check.svg"),
};

/// The search field's magnifying glass (the deck page, as the web and iOS apps draw it).
pub static SEARCH: Icon = Icon {
    name: "furiten-search",
    view: 20.0,
    body: include_str!("../../../../assets/icons/search.svg"),
};
