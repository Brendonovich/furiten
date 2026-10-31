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
