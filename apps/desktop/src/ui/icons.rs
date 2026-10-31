use rune::Icon;

pub static PLUS: Icon = Icon {
    name: "recall-plus",
    view: 16.0,
    body: r#"<path d="M8 3.5V12.5M3.5 8H12.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#,
};

pub static STACK: Icon = Icon {
    name: "recall-stack",
    view: 16.0,
    body: r#"<rect x="3" y="5.5" width="10" height="7.5" rx="1.6" stroke="currentColor" stroke-width="1.2" fill="none"/><path d="M4.5 3.5H11.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#,
};

pub static DECK: Icon = Icon {
    name: "recall-deck",
    view: 16.0,
    body: r#"<rect x="3.5" y="3" width="9" height="10" rx="1.6" stroke="currentColor" stroke-width="1.2" fill="none"/><path d="M6 6.5H10M6 9H8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#,
};

pub static CLOUD: Icon = Icon {
    name: "recall-cloud",
    view: 16.0,
    body: r#"<path d="M5 12.5H11.25C12.77 12.5 14 11.27 14 9.75C14 8.32 12.91 7.14 11.51 7.01C11.14 5.27 9.6 4 7.75 4C5.79 4 4.18 5.44 3.9 7.32C2.81 7.69 2 8.73 2 9.95C2 11.36 3.34 12.5 5 12.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/>"#,
};

pub static GRID: Icon = Icon {
    name: "recall-grid",
    view: 16.0,
    body: r#"<rect x="2.75" y="2.75" width="4.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/><rect x="8.75" y="2.75" width="4.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/><rect x="2.75" y="8.75" width="4.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/><rect x="8.75" y="8.75" width="4.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/>"#,
};

pub static LIST: Icon = Icon {
    name: "recall-list",
    view: 16.0,
    body: r#"<rect x="2.75" y="2.75" width="10.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/><rect x="2.75" y="8.75" width="10.5" height="4.5" rx="1.2" stroke="currentColor" stroke-width="1.2" fill="none"/>"#,
};

pub static COLUMN: Icon = Icon {
    name: "recall-column",
    view: 16.0,
    body: r#"<rect x="2.75" y="2.75" width="10.5" height="10.5" rx="1.6" stroke="currentColor" stroke-width="1.2" fill="none"/><path d="M6.5 2.75V13.25" stroke="currentColor" stroke-width="1.2"/>"#,
};

pub static PENCIL: Icon = Icon {
    name: "recall-pencil",
    view: 16.0,
    body: r#"<path d="M10.2 3.3L12.7 5.8M3.5 12.5L4.1 9.6L10.9 2.8C11.3 2.4 12 2.4 12.4 2.8L13.2 3.6C13.6 4 13.6 4.7 13.2 5.1L6.4 11.9L3.5 12.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/>"#,
};

pub static CHECK: Icon = Icon {
    name: "recall-check",
    view: 16.0,
    body: r#"<path d="M3.5 8.5L6.5 11.5L12.5 4.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};
