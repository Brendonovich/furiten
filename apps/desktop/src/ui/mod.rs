//! The window: a sidebar of decks beside a main pane that shows a deck, a review or settings, with the card editor in
//! a dialog over it.

mod cards;
mod deck;
mod editor;
pub mod format;
mod icons;
mod markup;
mod review;
mod settings;
mod shortcuts;
mod sidebar;

use rune::{Anim, Cx, El, Spring, col, el, text};
use rune_kit::shell::{SidebarToggle, SplitView, TOOLBAR_HEIGHT, Toolbar};
use rune_kit::{ControlSize, Keybind, KeybindVariant, Kit};

use crate::app::App;
use crate::theme;

pub use deck::key as deck_key;
pub use editor::key as editor_key;
pub use review::key as review_key;

/// The narrowest the main pane gets beside the open sidebar; narrower, the sidebar pushes it aside instead.
const MAIN_MIN_WIDTH: f32 = 480.0;

/// The sidebar opening and closing: the phone's drawer spring (`drawerSpring` in apps/ios, `Shell.swift`). The app
/// steps it a frame at a time (`App::sidebar_shown`).
pub const SIDEBAR: Spring = Spring::new(0.22, 0.0);

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let k = theme::kit(cx.system_dark());
    let mut layer = Vec::new();
    let (main, toolbar) = if s.connecting.is_some() {
        (connecting(s, &k), Toolbar::new())
    } else if let Some(settings) = &s.settings {
        settings::view(s, settings, &k)
    } else if let Some(r) = &s.review {
        review::view(s, r, &k, &mut layer)
    } else {
        deck::view(s, &k, &mut layer)
    };
    // A window too narrow to share slides the main pane aside for the sidebar rather than squeezing it, as the phone's
    // drawer does: what's left of the page dims, and a click on it or picking a row closes the sidebar.
    let sidebar_w = s.sidebar.width();
    let push = cx.layout_with("shell", move |r| r[2] - sidebar_w < MAIN_MIN_WIDTH).unwrap_or(false);
    // The sidebar opens and closes a frame at a time (`App::sidebar_shown`), and the window lays out again at each
    // step, so the page reflows as it resizes (the grid changing its columns as it goes) rather than springing to
    // where it ends up. Under Reduce Motion it lands where it's going.
    let shown = if cx.is_reduced_motion() { None } else { Some(s.sidebar_shown) };
    let fraction = shown.unwrap_or(if s.sidebar.is_open() { 1.0 } else { 0.0 });
    let mut main = col()
        .key("main")
        .on_window_focus(|s: &mut App, focused: bool, _: &mut rune::Tx| s.window_focused = focused)
        .child(main.key("page").grow(1.0).min_h(0.0))
        .child(sidebar_target(s));
    if push && fraction > 0.0 {
        main = main.child(
            el().key("scrim")
                .pin(Some(0.0), Some(-TOOLBAR_HEIGHT), Some(0.0), Some(0.0))
                .fill(k.dark_a(0.18 * fraction))
                .on_press(|s: &mut App| s.sidebar.set_open(false)),
        );
    }
    let sidebar = sidebar::view(s, &k, push, &mut layer);
    if let Some(e) = s.editor.as_ref().filter(|e| !e.inline) {
        editor::dialog(s, e, &k, &mut layer);
    }
    if s.shortcuts {
        shortcuts::dialog(&k, &mut layer);
    }
    let mut split = SplitView::new(main)
        .sidebar(&s.sidebar, |s: &mut App| &mut s.sidebar, sidebar)
        .push_main(push)
        .toolbar(toolbar)
        .titlebar_inset(cx.titlebar_inset())
        .frame_inset(8.0)
        .toolbar_in_panel()
        .sidebar_padding(12.0, 8.0)
        .sidebar_toggle(SidebarToggle::new().size(ControlSize::Large).past_lights(16.0).edge(18.0).gap(4.0).in_header())
        .reduced_motion(cx.is_reduced_motion());
    if s.first_run {
        // One path before the first review: no sidebar, nothing to choose.
        split = split.hide_sidebar_toggle();
    }
    if let Some(shown) = shown {
        split = split.sidebar_shown(shown);
    }
    // The app has no animation but the sidebar opening and closing (`SIDEBAR`, stepped by the app) and the review
    // card's reveal (`review::REVEAL`): every other change, entrance, exit, colour and hover lands on the next frame.
    rune_kit::root(split.build(&k), layer)
        .instant_changes()
        .motion(Spring::NONE)
        .hover_motion_within(Anim::INSTANT, Anim::INSTANT)
}

/// Nothing to see: an element as wide as the sidebar is going to be, open or closed, so that a toggle from anywhere
/// (its button, a key, a picked row) changes a layout, which asks for the frame that starts the sidebar moving.
fn sidebar_target(s: &App) -> El {
    el().key("sidebar-target")
        .pin(Some(0.0), Some(0.0), None, None)
        .size(if s.sidebar.is_open() { 1.0 } else { 0.0 }, 0.0)
        .on_layout_tx(|s: &mut App, _: [f32; 4], tx: &mut rune::Tx| {
            if s.sidebar_shown != if s.sidebar.is_open() { 1.0 } else { 0.0 } {
                tx.wake_after(std::time::Duration::ZERO);
            }
        })
}

/// While the core connection is being made: nothing for a quick one, a quiet line once it's taking a while.
fn connecting(s: &App, k: &Kit) -> El {
    let mut page = col().key("connecting").w_full().h_full().center();
    if s.slow_connect {
        page = page.child(text("Opening your decks…").key("opening").font(k.ty.ui).color(k.c.text_faint));
    }
    page
}

/// A main pane's toolbar with a title.
fn toolbar(title: impl Into<String>) -> Toolbar {
    Toolbar::new().title(title)
}

/// A line of muted text explaining what failed, shown where it happened.
fn error_line(k: &Kit, message: &str) -> El {
    rune::text(message.to_owned()).key("error").font(k.ty.ui).color(k.c.danger.fg).wrap()
}

/// A button label followed by the key that presses it.
fn keyed(label: &str, key: &str, k: &Kit, contrast: bool) -> impl FnOnce(rune::TextStyle, rune::Color) -> El + 'static {
    let (label, key, k) = (label.to_owned(), key.to_owned(), k.clone());
    move |font, ink| {
        let cap = if contrast {
            rune::text(key).font(k.ty.small_medium).color(ink.alpha(0.6))
        } else {
            Keybind::new(&[key]).variant(KeybindVariant::Neutral).build(&k)
        };
        rune::row().gap(8.0).center_y().child(rune::text(label).font(font).color(ink)).child(cap)
    }
}
