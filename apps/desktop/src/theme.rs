//! Recall's look: Anomaly's theme from `rune-theme-opencode`, plus a few type styles of Recall's own.

use rune::{Color, TextStyle};
use rune_kit::Kit;

/// The type a card's face is set in on the first-run stack.
pub const CARD_FACE: TextStyle = TextStyle::new(20.0, 400.0).lh(30.0);
/// The type a card's face is set in during review.
pub const REVIEW_FACE: TextStyle = TextStyle::new(26.0, 400.0).lh(36.0);
/// Large headings (the first-run deck title, empty states).
pub const DISPLAY: TextStyle = TextStyle::new(26.0, 600.0).lh(32.0).tracking(-0.3);
/// Small capitals in a tile's footer ("+ 1 HIDDEN SIDE", "DUE").
pub const CAPS: TextStyle = TextStyle::new(10.5, 600.0).lh(14.0).tracking(0.6).features("'tnum' 1");
/// Counts that change while you look at them.
pub const COUNT: TextStyle = TextStyle::new(12.0, 500.0).features("'tnum' 1");

/// The kit for one appearance. Get it in the view each frame: the theme resolves translucent tokens for the window's
/// blend mode.
pub fn kit(dark: bool) -> Kit {
    rune_theme_opencode::kit(dark).clone()
}

/// The accent, for the few things that should catch the eye: due counts and the first-run mark.
pub fn accent(k: &Kit) -> Color {
    k.c.text_accent
}
