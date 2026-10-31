//! Recall's desktop app: a thin Rune client of the `recall core` daemon.
//!
//! `cargo run` opens the window. `cargo run -- --fixtures [first-run|lived-in|caught-up]` runs on sample data.
//! Screenshots: `./snap.sh`, or `cargo run -- --fixtures --snap out.png --scene review-front --size 1040x680`.

mod app;
mod core;
mod theme;
mod ui;

use rune::{AppSpec, Chrome, Key, KeyEvent, Tx};
use rune_kit::shell;

use crate::app::App;
use crate::core::{Core, Fixtures};

fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if shell::is_sidebar_toggle(ev) && !s.first_run {
        s.sidebar.toggle();
        return true;
    }
    if s.editor.is_some() {
        return ui::editor_key(s, ev, tx);
    }
    if ev.is_command('n') {
        s.new_card();
        return true;
    }
    if ev.is_command(',') && !s.first_run {
        s.open_settings();
        return true;
    }
    let plain = !(ev.mods.cmd || ev.mods.ctrl || ev.mods.alt);
    if let Some(r) = &s.review {
        let has_card = r.card().is_some();
        match &ev.key {
            Key::Text(t) if t == " " && plain && has_card => s.reveal(),
            Key::Enter if plain && has_card => s.reveal(),
            Key::Text(t) if plain && matches!(t.as_str(), "1" | "2" | "3" | "4") => {
                s.grade(t.parse().expect("a digit"))
            }
            Key::Escape => s.end_review(),
            _ => return false,
        }
        return true;
    }
    if s.settings.is_some() {
        if ev.key == Key::Escape {
            s.settings = None;
            return true;
        }
        return false;
    }
    let (due, new) = s.counts();
    match &ev.key {
        Key::Text(t) if t == " " && plain && due + new > 0 => s.start_review(),
        Key::Enter if plain && due + new > 0 => s.start_review(),
        _ => return false,
    }
    true
}

fn poll(s: &mut App, _: f64, _: &mut Tx) -> bool {
    s.poll_changed()
}

fn setup(s: &mut App, wake: rune::Waker) {
    s.watch(wake);
}

type Scene = (&'static str, fn(&mut App));

const SCENES: &[Scene] = &[
    ("first-run", |s| s.with_core(Core::fixtures(Fixtures::FirstRun))),
    ("review-front", |s| {
        s.with_core(Core::fixtures(Fixtures::FirstRun));
        s.start_review();
    }),
    ("review-revealed", |s| {
        s.with_core(Core::fixtures(Fixtures::FirstRun));
        s.start_review();
        s.reveal();
    }),
    ("review-graded", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.start_review();
        s.reveal();
        s.grade(3);
        s.reveal();
    }),
    ("deck", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
    }),
    ("editor", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.new_card();
        if let Some(e) = s.editor.as_mut() {
            e.content.replace_text("**la merienda**\n\n*noun, feminine*\n---\nan afternoon snack");
            e.content.set_caret(e.content.text.len());
            e.tags.replace_text("vocab, food");
        }
    }),
    ("empty", |s| {
        s.with_core(Core::fixtures(Fixtures::CaughtUp));
        s.start_review();
    }),
    ("settings", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        s.open_settings();
    }),
];

fn select(s: &mut App, name: &str) {
    let id = s.decks.iter().find(|d| d.deck.name == name).map(|d| d.deck.id.clone());
    s.select_deck(id);
}

/// `apps/desktop` is its own workspace, so `cargo run` here never builds the `recall` binary the app spawns as its
/// core. Debug builds build it from the root workspace and point `RECALL_BIN` at it, so the app always runs against
/// the core from the same checkout. Release builds find `recall` next to the app or on PATH.
#[cfg(debug_assertions)]
fn use_dev_core() {
    if std::env::var_os("RECALL_BIN").is_some() {
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let built = std::process::Command::new(cargo)
        .args(["build", "-q", "-p", "recall", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .status()
        .is_ok_and(|status| status.success());
    let bin = root.join("target/debug/recall");
    if !built {
        eprintln!("recall-desktop: building the recall core failed; using {} if it exists", bin.display());
    }
    if bin.exists() {
        // SAFETY: runs first thing in `main`, before any other thread exists.
        unsafe { std::env::set_var("RECALL_BIN", bin) };
    }
}

fn main() {
    #[cfg(debug_assertions)]
    use_dev_core();
    let chrome = Chrome { full_size_content: true, traffic_lights: Some((16.0, 25.0)), drag_regions: true };
    let spec = AppSpec {
        min_size: Some((720.0, 480.0)),
        scenes: SCENES,
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        ..AppSpec::new("Recall", (1040.0, 680.0), app::init, ui::view)
    };
    rune::App::new(spec).chrome(chrome).run();
}
