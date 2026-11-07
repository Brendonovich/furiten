//! Recall's desktop app, which also hosts the core the CLI talks to.
//!
//! `cargo run` opens the window. `cargo run -- --background` hosts the core without one (the CLI starts the app this
//! way when nothing is running). `cargo run -- --fixtures [first-run|lived-in|caught-up]` runs on sample data.
//! Screenshots: `./snap.sh`, or `cargo run -- --fixtures --snap out.png --scene review-front --size 1040x680`.

mod app;
mod core;
mod editor;
mod theme;
mod ui;

use std::sync::{Mutex, OnceLock};

use recall_core::server::Hosted;
use rune::{AppHandle, AppSpec, Chrome, Key, KeyEvent, Tx};
use rune_kit::shell;

use crate::app::{App, CardsView, Editor};
use crate::core::{CardSort, Core, Fixtures, Running};

/// The core this app hosts, stopped (pushing what's left) once the app quits.
static HOSTED: Mutex<Option<Hosted>> = Mutex::new(None);
/// Set for `--background` runs, whose window comes and goes.
static BACKGROUND: OnceLock<AppHandle<App>> = OnceLock::new();

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
        let (has_card, inbox) = (r.card().is_some(), r.inbox());
        match &ev.key {
            Key::Text(t) if t == " " && plain && has_card => s.reveal(),
            Key::Enter if plain && has_card => s.reveal(),
            Key::Text(t) if plain && t == "1" => s.answer(false),
            Key::Text(t) if plain && t == "2" => s.answer(true),
            Key::Escape if !inbox => s.end_review(),
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
        s.answer(true);
        s.reveal();
    }),
    ("inbox", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        s.open_inbox();
    }),
    ("inbox-revealed", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        s.open_inbox();
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
    ("sort-menu", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.set_sort(CardSort::Name);
        s.picking_sort = true;
    }),
    ("search", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.search.replace_text("hubiera");
        s.search.focused = true;
        s.apply_search();
    }),
    ("deck-menu", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.deck_menu = true;
    }),
    ("connecting", |s| {
        *s = App::connecting();
        s.slow_connect = true;
    }),
    ("editor", editor_scene),
    ("editor-separate", |s| {
        editor_scene(s);
        s.toggle_separate_sides();
    }),
    ("tag-menu", |s| {
        s.with_core(Core::fixtures(Fixtures::LivedIn));
        select(s, "Spanish");
        s.new_card();
        if let Some(e) = s.editor.as_mut() {
            e.sides[0].edit.replace_text("**la merienda**\n---\nan afternoon snack");
            e.sides[0].edit.focused = false;
            e.split();
            e.tag_draft.focused = true;
            e.tag_draft.replace_text("vocab ");
            e.take_typed_tags();
            e.tag_draft.replace_text("e");
            e.take_typed_tags();
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

/// Shows the window (`app.open` from a client, or the Dock), with the Dock icon a background run hides.
fn show(handle: &AppHandle<App>) {
    handle.set_dock_icon(true);
    handle.open();
}

/// A background run's window closes back to no window, and no Dock icon; the core keeps serving.
fn hide_dock(_: &mut App, _: &mut Tx) {
    if let Some(handle) = BACKGROUND.get() {
        handle.set_dock_icon(false);
    }
}

/// An accessory app gets no default reopen, so a background run brings its window back itself.
fn reopen(_: &mut App, has_visible_windows: bool, _: &mut Tx) -> bool {
    match BACKGROUND.get() {
        Some(handle) if !has_visible_windows => {
            show(handle);
            true
        }
        _ => false,
    }
}

/// Hosts the core on another thread, so the window never waits on it (opening the store can take seconds): once the
/// first frame is up, or straight away without a window. The app quits when a client stops the core.
fn host_core(app: rune::App<App>, background: bool) -> rune::App<App> {
    if !app::wants_core() {
        return app;
    }
    let handle = app.handle();
    let start = move || {
        let handle = handle.clone();
        std::thread::Builder::new()
            .name("recall-host".into())
            .spawn(move || {
                let (opener, quitter) = (handle.clone(), handle.clone());
                let core = Core::host(move || show(&opener), move || quitter.quit()).map(|(core, hosted)| {
                    *HOSTED.lock().unwrap_or_else(|e| e.into_inner()) = hosted;
                    core
                });
                handle.update(move |s: &mut App, tx| {
                    s.connected(core);
                    // State that arrives right after launch lands without motion.
                    tx.instant();
                });
            })
            .expect("spawn the host thread");
    };
    if background {
        start();
        return app;
    }
    let mut started = false;
    app.on_ready(move |_| {
        if !std::mem::replace(&mut started, true) {
            start();
        }
    })
}

fn main() {
    let background = rune::extra_args().iter().any(|a| a == "--background");
    // One app per library: another one already serving it shows its window instead.
    if app::wants_core() {
        match core::running() {
            Running::App => return,
            Running::Windowless if background => {
                eprintln!("recall-desktop: a core is already running for {}", recall_core::default_home().display());
                return;
            }
            Running::Windowless | Running::Nothing => {}
        }
    }
    if !background {
        rune::prewarm_gpu();
    }
    let chrome = Chrome { full_size_content: true, traffic_lights: Some((25.0, 25.0)), drag_regions: true };
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
    let mut app = rune::App::new(spec).chrome(chrome);
    if background {
        app = app.windowless().accessory().standard_menus().on_close(hide_dock).on_reopen(reopen);
        let _ = BACKGROUND.set(app.handle());
    }
    host_core(app, background).run();
    let hosted = HOSTED.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(Err(e)) = hosted.map(Hosted::stop) {
        eprintln!("recall-desktop: the core didn't stop cleanly: {}", e.message);
    }
}

/// The editor scene: a new card with a few sides, Markdown and tags.
fn editor_scene(s: &mut App) {
    s.with_core(Core::fixtures(Fixtures::LivedIn));
    select(s, "Spanish");
    s.new_card();
    if let Some(e) = s.editor.as_mut() {
        let content = "# la merienda\n\n*noun, feminine* · `la`\n---\nan **afternoon snack**, eaten around six\n- pan con chocolate\n  - con aceite\n- fruta\n---\n\
                       > Siempre meriendo pan con chocolate.";
        *e = Editor::new(None, content, vec!["vocab".into(), "food".into()], e.deck.clone());
    }
}
