//! Every key the app answers to, in one table: the handlers ask [`pressed`] whether a key is an action's, and the
//! shortcuts panel (`?`) and the README's Keys table are drawn from [`SECTIONS`]. The keys follow Mochi's where it
//! has one, so they work the same for someone coming from it.

use rune::{Key, KeyEvent};

/// Something a key does. Each is bound once, in one section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NewCard,
    Settings,
    ToggleSidebar,
    Shortcuts,
    Close,

    Advance,
    Reveal,
    Hide,
    Remember,
    Forget,
    Edit,
    Delete,
    Move,
    EndReview,

    AddCard,
    Search,
    StartReview,
    SelectUp,
    SelectDown,
    SelectLeft,
    SelectRight,
    Open,
    ClearSelection,
    GridView,
    ListView,
    ColumnView,

    Save,
    Cancel,
    NewSide,
    Bold,
    Italic,
}

const SHIFT: u8 = 1;
const CMD: u8 = 2;
const CTRL: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum K {
    Char(char),
    Space,
    Enter,
    Escape,
    Up,
    Down,
    Left,
    Right,
}

/// A key and the modifiers held with it. ⌘ is Ctrl off macOS, where a ⌃ chord is the same as its ⌘ one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    key: K,
    mods: u8,
}

const fn key(key: K) -> Chord {
    Chord { key, mods: 0 }
}
const fn ch(c: char) -> Chord {
    key(K::Char(c))
}
const fn shift(c: Chord) -> Chord {
    Chord { mods: c.mods | SHIFT, ..c }
}
const fn cmd(c: Chord) -> Chord {
    Chord { mods: c.mods | CMD, ..c }
}
const fn ctrl(c: Chord) -> Chord {
    Chord { mods: c.mods | CTRL, ..c }
}

const SPACE: Chord = key(K::Space);
const RETURN: Chord = key(K::Enter);
const ESC: Chord = key(K::Escape);
const UP: Chord = key(K::Up);
const DOWN: Chord = key(K::Down);
const LEFT: Chord = key(K::Left);
const RIGHT: Chord = key(K::Right);

pub struct Binding {
    pub action: Action,
    pub label: &'static str,
    pub chords: &'static [Chord],
}

pub struct Section {
    pub title: &'static str,
    pub bindings: &'static [Binding],
}

const fn bind(action: Action, label: &'static str, chords: &'static [Chord]) -> Binding {
    Binding { action, label, chords }
}

/// Rows with the same label next to each other show as one in the panel and the README.
pub const SECTIONS: &[Section] = &[
    Section {
        title: "Anywhere",
        bindings: &[
            bind(Action::NewCard, "New card", &[cmd(ch('n'))]),
            bind(Action::Settings, "Settings and sync", &[cmd(ch(','))]),
            bind(Action::ToggleSidebar, "Show or hide the sidebar", &[cmd(ch('\\')), shift(cmd(ch('b')))]),
            bind(Action::Shortcuts, "Keyboard shortcuts", &[ch('?')]),
            bind(Action::Close, "Close settings or this panel", &[ESC]),
        ],
    },
    Section {
        title: "Review",
        bindings: &[
            bind(Action::Advance, "Show the next side; on the last, Remembered", &[SPACE]),
            bind(Action::Reveal, "Show the next side", &[DOWN, ch('j'), RETURN]),
            bind(Action::Hide, "Hide the last side shown", &[UP, ch('k')]),
            bind(Action::Remember, "Remembered, from any side", &[shift(SPACE), ch('2')]),
            bind(Action::Forget, "Forgot, from any side", &[ch('f'), ch('1')]),
            bind(Action::Edit, "Edit the card in place", &[ch('e')]),
            bind(Action::Delete, "Delete the card (press again to confirm)", &[ch('d')]),
            bind(Action::Move, "Move the card to another deck", &[ctrl(ch('m'))]),
            bind(Action::EndReview, "End a deck’s review", &[ESC]),
        ],
    },
    Section {
        title: "A deck",
        bindings: &[
            bind(Action::AddCard, "New card", &[ch('n')]),
            bind(Action::Search, "Search (Esc clears it, Esc again leaves it)", &[ch('/')]),
            bind(Action::StartReview, "Review the cards shown, due or not, without scheduling them", &[SPACE]),
            bind(Action::SelectUp, "Move the selection", &[UP]),
            bind(Action::SelectDown, "Move the selection", &[DOWN]),
            bind(Action::SelectLeft, "Move the selection", &[LEFT]),
            bind(Action::SelectRight, "Move the selection", &[RIGHT]),
            bind(
                Action::Open,
                "Open the selected card (in Column, its detail first); with none, review the cards shown",
                &[RETURN],
            ),
            bind(Action::ClearSelection, "Clear the selection", &[ESC]),
            bind(Action::GridView, "Grid view", &[cmd(ch('1'))]),
            bind(Action::ListView, "List view", &[cmd(ch('2'))]),
            bind(Action::ColumnView, "Column view", &[cmd(ch('3'))]),
        ],
    },
    Section {
        title: "Editor",
        bindings: &[
            bind(Action::Save, "Save card", &[cmd(RETURN)]),
            bind(Action::Cancel, "Cancel", &[ESC]),
            bind(Action::NewSide, "Start a new side at the caret", &[shift(RETURN), cmd(ch('-'))]),
            bind(Action::Bold, "Bold", &[cmd(ch('b'))]),
            bind(Action::Italic, "Italic", &[cmd(ch('i'))]),
        ],
    },
];

fn binding(action: Action) -> &'static Binding {
    SECTIONS.iter().flat_map(|s| s.bindings).find(|b| b.action == action).expect("every action is bound")
}

/// Whether `ev` is one of `action`'s keys.
pub fn pressed(ev: &KeyEvent, action: Action) -> bool {
    binding(action).chords.iter().any(|c| c.matches(ev))
}

/// The keycaps for `action`'s first key, for a button's tooltip.
pub fn hint(action: Action) -> String {
    let mac = cfg!(target_os = "macos");
    binding(action).chords[0].caps(mac).join(if mac { "" } else { "+" })
}

impl Chord {
    fn matches(self, ev: &KeyEvent) -> bool {
        let m = ev.mods;
        if m.alt {
            return false;
        }
        let mac = cfg!(target_os = "macos");
        let want_ctrl = mac && self.mods & CTRL != 0;
        let want_cmd = self.mods & CMD != 0 || (!mac && self.mods & CTRL != 0);
        if m.cmd != want_cmd || (mac && m.ctrl != want_ctrl) {
            return false;
        }
        let typed = match &ev.key {
            Key::Command(c) => Some(*c),
            Key::Text(t) => {
                let mut cs = t.chars();
                cs.next().filter(|_| cs.next().is_none()).map(|c| c.to_ascii_lowercase())
            }
            _ => None,
        };
        let (hit, shifted) = match self.key {
            K::Space => (typed == Some(' '), true),
            // Shift makes another character of punctuation (`?` is ⇧/), so only a letter's shift counts.
            K::Char(c) => (typed == Some(c), c.is_ascii_alphabetic()),
            K::Enter => (ev.key == Key::Enter, true),
            K::Escape => (ev.key == Key::Escape, true),
            K::Up => (ev.key == Key::Up, true),
            K::Down => (ev.key == Key::Down, true),
            K::Left => (ev.key == Key::Left, true),
            K::Right => (ev.key == Key::Right, true),
        };
        hit && (!shifted || m.shift == (self.mods & SHIFT != 0))
    }

    /// The keycaps, modifiers first: `["⇧", "Space"]`, or `["Shift", "Space"]` off macOS.
    pub fn caps(self, mac: bool) -> Vec<String> {
        let mut caps = Vec::new();
        let mut m = |on: bool, sym: &str, word: &str| {
            if on {
                caps.push(if mac { sym } else { word }.to_owned());
            }
        };
        m(self.mods & CTRL != 0, "⌃", "Ctrl");
        m(self.mods & SHIFT != 0, "⇧", "Shift");
        m(self.mods & CMD != 0, "⌘", "Ctrl");
        caps.dedup();
        caps.push(match self.key {
            K::Char(c) => c.to_ascii_uppercase().to_string(),
            K::Space => "Space".into(),
            K::Enter => if mac { "Return" } else { "Enter" }.into(),
            K::Escape => "Esc".into(),
            K::Up => "↑".into(),
            K::Down => "↓".into(),
            K::Left => "←".into(),
            K::Right => "→".into(),
        });
        caps
    }
}

/// A section's rows, with neighbours that share a label merged: each label and every key that does it.
pub fn rows(section: &Section) -> Vec<(&'static str, Vec<Chord>)> {
    let mut rows: Vec<(&'static str, Vec<Chord>)> = Vec::new();
    for b in section.bindings {
        match rows.last_mut() {
            Some((label, chords)) if *label == b.label => chords.extend_from_slice(b.chords),
            _ => rows.push((b.label, b.chords.to_vec())),
        }
    }
    rows
}

/// The README's Keys table, as macOS shows the keys.
#[cfg(test)]
fn markdown_table() -> String {
    let mut out = String::from("| Where | Keys | Does |\n| --- | --- | --- |\n");
    for section in SECTIONS {
        for (label, chords) in rows(section) {
            let keys: Vec<String> = chords.iter().map(|c| c.caps(true).concat().replace('\\', "\\\\")).collect();
            out.push_str(&format!("| {} | {} | {} |\n", section.title, keys.join(", "), label));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::Mods;

    fn ev(key: Key, mods: Mods) -> KeyEvent {
        KeyEvent { key, mods }
    }
    fn text(t: &str) -> Key {
        Key::Text(t.into())
    }
    const NONE: Mods = Mods { shift: false, cmd: false, alt: false, ctrl: false, logo: false };
    const SHIFTED: Mods = Mods { shift: true, ..NONE };

    #[test]
    fn letters_need_their_shift_and_punctuation_doesnt() {
        assert!(pressed(&ev(text("f"), NONE), Action::Forget));
        assert!(!pressed(&ev(text("F"), SHIFTED), Action::Forget), "⇧F isn't F");
        assert!(pressed(&ev(text("?"), SHIFTED), Action::Shortcuts));
        assert!(pressed(&ev(text(" "), NONE), Action::Advance));
        assert!(!pressed(&ev(text(" "), NONE), Action::Remember));
        assert!(pressed(&ev(text(" "), SHIFTED), Action::Remember));
        assert!(!pressed(&ev(text(" "), SHIFTED), Action::Advance));
    }

    #[test]
    fn command_chords_match_only_with_command() {
        let cmd = Mods { cmd: true, logo: cfg!(target_os = "macos"), ctrl: !cfg!(target_os = "macos"), ..NONE };
        assert!(pressed(&ev(Key::Command('\\'), cmd), Action::ToggleSidebar));
        assert!(pressed(&ev(Key::Command('b'), Mods { shift: true, ..cmd }), Action::ToggleSidebar));
        assert!(!pressed(&ev(Key::Command('b'), cmd), Action::ToggleSidebar), "⌘B is Bold");
        assert!(pressed(&ev(Key::Command('b'), cmd), Action::Bold));
        assert!(pressed(&ev(Key::Enter, cmd), Action::Save));
        assert!(!pressed(&ev(Key::Enter, NONE), Action::Save));
        assert!(!pressed(&ev(text("n"), NONE), Action::NewCard));
        assert!(!pressed(&ev(Key::Command('n'), cmd), Action::AddCard));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn control_is_its_own_modifier_on_macos() {
        let ctrl = Mods { ctrl: true, ..NONE };
        assert!(pressed(&ev(text("m"), ctrl), Action::Move));
        assert!(!pressed(&ev(text("m"), NONE), Action::Move));
        assert!(!pressed(&ev(Key::Command('m'), Mods { cmd: true, logo: true, ..NONE }), Action::Move));
    }

    #[test]
    fn neighbouring_rows_with_one_label_merge() {
        let deck = SECTIONS.iter().find(|s| s.title == "A deck").unwrap();
        let rows = rows(deck);
        let (_, arrows) = rows.iter().find(|(l, _)| *l == "Move the selection").unwrap();
        assert_eq!(arrows.len(), 4);
        assert_eq!(hint(Action::Remember), if cfg!(target_os = "macos") { "⇧Space" } else { "Shift+Space" });
    }

    #[test]
    fn the_readme_lists_every_key() {
        let readme = include_str!("../README.md");
        assert!(readme.contains(&markdown_table()), "update README.md's Keys table to:\n{}", markdown_table());
    }
}
