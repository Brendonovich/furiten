//! Recall's look: a warm paper palette with one ember accent, turned into a rune-kit `Kit`.

use std::cell::RefCell;

use rune::{Color, TextStyle, Theme};
use rune_kit::{Kit, PaletteSeed};

const THEME: &str = include_str!("../assets/theme.json");

/// The type a card's face is set in during review.
pub const CARD_FACE: TextStyle = TextStyle::new(20.0, 400.0).lh(30.0);
/// Large headings (the first-run deck title, empty states).
pub const DISPLAY: TextStyle = TextStyle::new(26.0, 600.0).lh(32.0).tracking(-0.3);
/// Small capitals in a tile's footer ("+ 1 HIDDEN SIDE", "DUE").
pub const CAPS: TextStyle = TextStyle::new(10.5, 600.0).lh(14.0).tracking(0.6).features("'tnum' 1");
/// Counts that change while you look at them.
pub const COUNT: TextStyle = TextStyle::new(12.0, 500.0).features("'tnum' 1");

thread_local! {
    static KITS: RefCell<[Option<Kit>; 2]> = const { RefCell::new([None, None]) };
}

pub fn kit(dark: bool) -> Kit {
    KITS.with(|kits| kits.borrow_mut()[dark as usize].get_or_insert_with(|| build(dark)).clone())
}

fn build(dark: bool) -> Kit {
    let t = Theme::from_json(THEME, dark).unwrap_or_else(|e| panic!("assets/theme.json: {e}"));
    let mut k = Kit::from_palette(PaletteSeed { bg: t.color("bg"), fg: t.color("text"), accent: t.color("accent") });
    k.radius.lg = 10.0;
    k.radius.xl = 14.0;
    k
}

/// The accent, for the few things that should catch the eye: due counts and the first-run mark.
pub fn accent(k: &Kit) -> Color {
    k.c.text_accent
}
