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

use crate::app::{App, CardsView};
use crate::core::{Core, Fixtures};

fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if s.connecting.is_some() {
        return false;
    }
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
    ui::deck_key(s, ev, tx)
}

fn poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    s.tick(clock, tx)
}

fn setup(s: &mut App, wake: rune::Waker) {
    s.set_waker(wake);
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
    ("grid", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
    }),
    ("grid-selected", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.move_selection(0, 1);
    }),
    ("grid-narrow", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
    }),
    ("list", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.set_view(CardsView::List);
    }),
    ("column", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.set_view(CardsView::Column);
        s.move_selection(0, 1);
        s.move_selection(0, 1);
        s.move_selection(0, 1);
        s.move_selection(0, 1);
    }),
    ("view-menu", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.picking_view = true;
    }),
    ("search", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.search.replace_text("hubiera");
        s.search.focused = true;
        s.apply_search();
    }),
    ("all-decks", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.select_deck(None);
    }),
    ("connecting", |s| {
        *s = App::connecting();
        s.slow_connect = true;
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
    if let Some(bin) = std::env::var_os("RECALL_BIN") {
        eprintln!("recall-desktop: RECALL_BIN is set, not building the core: {}", bin.to_string_lossy());
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap_or(root);
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let target = root.join("target");
    eprintln!("recall-desktop: building the recall core ({} build --profile dev -p recall)…", cargo.to_string_lossy());
    let started = std::time::Instant::now();
    // The dev profile into the checkout's own target dir, whatever the environment or a cargo config says, so the
    // binary is the debug one at the path below.
    let built = match std::process::Command::new(&cargo)
        .args(["build", "-q", "--profile", "dev", "-p", "recall", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .env_remove("CARGO_BUILD_TARGET")
        .status()
    {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!("recall-desktop: cargo build exited with {status}");
            false
        }
        Err(e) => {
            eprintln!("recall-desktop: couldn't run {}: {e}", cargo.to_string_lossy());
            false
        }
    };
    let elapsed = started.elapsed().as_secs_f64();
    let bin = target.join("debug/recall");
    if built {
        eprintln!("recall-desktop: built the recall core (dev profile) in {elapsed:.1}s");
    } else {
        eprintln!(
            "recall-desktop: building the recall core failed after {elapsed:.1}s; using {} if it exists",
            bin.display()
        );
    }
    if bin.exists() {
        eprintln!("recall-desktop: using the core at {}", bin.display());
        // SAFETY: runs first thing in `main`, before any other thread exists.
        unsafe { std::env::set_var("RECALL_BIN", bin) };
    } else {
        eprintln!("recall-desktop: no core at {}; looking next to the app and on PATH", bin.display());
    }
}

/// Connects to the core on another thread once the first frame is up, so the window never waits on it (starting
/// the core can take seconds).
fn connect_when_ready(app: rune::App<App>) -> rune::App<App> {
    if !app::wants_core() {
        return app;
    }
    let handle = app.handle();
    let mut started = false;
    app.on_ready(move |_| {
        if std::mem::replace(&mut started, true) {
            return;
        }
        let handle = handle.clone();
        std::thread::Builder::new()
            .name("recall-connect".into())
            .spawn(move || {
                let core = Core::connect();
                handle.update(move |s: &mut App, tx| {
                    s.connected(core);
                    // State that arrives right after launch lands without motion.
                    tx.instant();
                });
            })
            .expect("spawn the connect thread");
    })
}

fn main() {
    #[cfg(debug_assertions)]
    use_dev_core();
    rune::prewarm_gpu();
    let chrome = Chrome { full_size_content: true, traffic_lights: Some((16.0, 25.0)), drag_regions: true };
    let spec = AppSpec {
        min_size: Some((720.0, 480.0)),
        scenes: SCENES,
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        fonts: rune_theme_opencode::FONTS,
        families: rune_theme_opencode::FAMILIES,
        ..AppSpec::new("Recall", (1040.0, 680.0), app::init, ui::view)
    };
    connect_when_ready(rune::App::new(spec).chrome(chrome)).run();
}
