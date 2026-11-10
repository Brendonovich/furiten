use rune::Icon;

pub static PLUS: Icon = Icon {
    name: "furiten-plus",
    view: 16.0,
    body: r#"<path d="M8 3.5V12.5M3.5 8H12.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#,
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
    body: r#"<path d="M4.5 15.5v-11A1.5 1.5 0 0 1 6 3h9.5v11H6a1.5 1.5 0 0 0 0 3h9.5M8 6.5h4.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

/// A tray: every due card from every deck, the sidebar's Review.
pub static INBOX: Icon = Icon {
    name: "furiten-inbox",
    view: 20.0,
    body: r#"<path d="M3.5 11l2.2-6h8.6l2.2 6v3.5a2 2 0 0 1-2 2h-9a2 2 0 0 1-2-2zM3.5 11h4l1 2h3l1-2h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

/// Ascending: the sort direction button.
pub static UP: Icon = Icon {
    name: "furiten-up",
    view: 20.0,
    body: r#"<path d="M10 16V4M5 9l5-5 5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

pub static DOWN: Icon = Icon {
    name: "furiten-down",
    view: 20.0,
    body: r#"<path d="M10 4v12M5 11l5 5 5-5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

/// Random's direction button: shuffle again.
pub static SHUFFLE: Icon = Icon {
    name: "furiten-shuffle",
    view: 20.0,
    body: r#"<path d="M3 6h3l8 8h3M3 14h3l8-8h3M15 4l2 2-2 2M15 12l2 2-2 2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

/// Forgot.
pub static CROSS: Icon = Icon {
    name: "furiten-cross",
    view: 20.0,
    body: r#"<path d="M5 5l10 10M15 5L5 15" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" fill="none"/>"#,
};

pub static CLOUD: Icon = Icon {
    name: "furiten-cloud",
    view: 16.0,
    body: r#"<path d="M5 12.5H11.25C12.77 12.5 14 11.27 14 9.75C14 8.32 12.91 7.14 11.51 7.01C11.14 5.27 9.6 4 7.75 4C5.79 4 4.18 5.44 3.9 7.32C2.81 7.69 2 8.73 2 9.95C2 11.36 3.34 12.5 5 12.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/>"#,
};

pub static GRID: Icon = Icon {
    name: "furiten-grid",
    view: 16.0,
    body: r#"<rect x="2.75" y="2.75" width="4.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/><rect x="8.75" y="2.75" width="4.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/><rect x="2.75" y="8.75" width="4.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/><rect x="8.75" y="8.75" width="4.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/>"#,
};

pub static LIST: Icon = Icon {
    name: "furiten-list",
    view: 16.0,
    body: r#"<rect x="2.75" y="2.75" width="10.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/><rect x="2.75" y="8.75" width="10.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/>"#,
};

pub static COLUMN: Icon = Icon {
    name: "furiten-column",
    view: 16.0,
    body: r#"<rect x="2.75" y="2.75" width="10.5" height="10.5" rx="1.6" stroke="currentColor" stroke-width="1.2" fill="none"/><path d="M6.5 2.75V13.25" stroke="currentColor" stroke-width="1.2"/>"#,
};

/// An arrow up and an arrow down side by side: ordering.
pub static SORT: Icon = Icon {
    name: "furiten-sort",
    view: 16.0,
    body: r#"<path d="M5 12.5V3.5M2.75 5.75L5 3.5L7.25 5.75M11 3.5V12.5M8.75 10.25L11 12.5L13.25 10.25" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

pub static PENCIL: Icon = Icon {
    name: "furiten-pencil",
    view: 16.0,
    body: r#"<path d="M10.2 3.3L12.7 5.8M3.5 12.5L4.1 9.6L10.9 2.8C11.3 2.4 12 2.4 12.4 2.8L13.2 3.6C13.6 4 13.6 4.7 13.2 5.1L6.4 11.9L3.5 12.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/>"#,
};

pub static CHECK: Icon = Icon {
    name: "furiten-check",
    view: 16.0,
    body: r#"<path d="M3.5 8.5L6.5 11.5L12.5 4.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};
