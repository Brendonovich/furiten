//! The window: a sidebar of decks beside a main pane that shows a deck, a review, the card editor or settings.

mod deck;
mod editor;
pub mod format;
mod icons;
mod review;
mod settings;
mod sidebar;

use rune::{Cx, El};
use rune_kit::shell::{SidebarToggle, SplitView, Toolbar};
use rune_kit::{ControlSize, Keybind, KeybindVariant, Kit};

use crate::app::App;
use crate::theme;

pub use editor::key as editor_key;

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let k = theme::kit(cx.system_dark());
    let mut layer = Vec::new();
    let (main, toolbar) = if let Some(e) = &s.editor {
        editor::view(s, e, &k, &mut layer)
    } else if let Some(settings) = &s.settings {
        settings::view(s, settings, &k)
    } else if let Some(r) = &s.review {
        review::view(s, r, &k)
    } else {
        deck::view(s, &k)
    };
    let mut split = SplitView::new(main.key("main"))
        .sidebar(&s.sidebar, |s: &mut App| &mut s.sidebar, sidebar::view(s, &k))
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
    rune_kit::root(split.build(&k), layer)
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
