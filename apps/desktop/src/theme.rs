//! Furiten's look: Anomaly's theme from `rune-theme-opencode`, plus a few type styles and surfaces of Furiten's own.
//! The iOS app (Furiten theme) is the reference for these: its cards, sides, rules and tags (apps/ios, `Components.swift`).

use rune::{Color, El, TextStyle, el};
use rune_kit::Kit;

/// The type a card's face is set in on the first-run stack.
pub const CARD_FACE: TextStyle = TextStyle::new(20.0, 400.0).lh(30.0);
/// A card's sides wherever they're shown whole: review, a card's detail and the editor (iOS `.title2`, 4 points of
/// line spacing).
pub const SIDE: TextStyle = TextStyle::new(22.0, 400.0).lh(30.0);
/// Large headings (the first-run deck title, empty states).
pub const DISPLAY: TextStyle = TextStyle::new(26.0, 600.0).lh(32.0).tracking(-0.3);
/// Small capitals in a tile's footer ("+ 1 HIDDEN SIDE", "DUE").
pub const CAPS: TextStyle = TextStyle::new(11.0, 600.0).lh(14.0).tracking(0.6).features("'tnum' 1");
/// Small capitals over a card's detail ("✓ 3 REVIEWS · DUE IN 2 DAYS").
pub const META: TextStyle = TextStyle::new(12.0, 600.0).lh(16.0).tracking(0.6).features("'tnum' 1");
/// Counts that change while you look at them.
pub const COUNT: TextStyle = TextStyle::new(12.0, 500.0).features("'tnum' 1");
/// A tag on a card (iOS `.footnote`).
pub const TAG: TextStyle = TextStyle::new(13.0, 400.0).lh(18.0);
/// The editor's pills: Side, the deck, the tags (iOS `.subheadline` medium).
pub const PILL: TextStyle = TextStyle::new(15.0, 500.0).lh(20.0);
/// The editor's hint beside Add side (iOS `.footnote`).
pub const HINT: TextStyle = TextStyle::new(13.0, 400.0).lh(18.0);

/// A tile in the grid, the list or the column.
pub const TILE_RADIUS: f32 = 16.0;
/// A card's detail and the editor's card.
pub const CARD_RADIUS: f32 = 18.0;
/// The card under review.
pub const REVIEW_RADIUS: f32 = 24.0;
/// A side's inset from the card's edges, and the room above the first side and below the last.
pub const SIDE_X: f32 = 24.0;
pub const SIDE_Y: f32 = 26.0;
/// The room either side of the rule between two sides.
pub const RULE_GAP: f32 = 20.0;

/// The kit for one appearance. Get it in the view each frame: the theme resolves translucent tokens for the window's
/// blend mode.
pub fn kit(dark: bool) -> Kit {
    rune_theme_opencode::kit(dark).clone()
}

/// The accent, for the few things that should catch the eye: due counts and the first-run mark.
pub fn accent(k: &Kit) -> Color {
    k.c.text_accent
}

/// The label colours iOS dims text with: secondary, tertiary and quaternary.
pub fn secondary(k: &Kit) -> Color {
    k.alpha(k.c.text_base, 0.6)
}
pub fn tertiary(k: &Kit) -> Color {
    k.alpha(k.c.text_base, 0.3)
}
pub fn quaternary(k: &Kit) -> Color {
    k.alpha(k.c.text_base, 0.18)
}

/// A pill's fill (iOS `.fill.tertiary`).
pub fn pill_fill(k: &Kit) -> Color {
    let grey = rune::color::hex("#767680").expect("a colour");
    k.alpha(grey, if k.dark { 0.24 } else { 0.12 })
}

/// A card's surface, with the lit edge the iOS app's cards have: a hairline ring, brighter along the top as if lit
/// from above, and a soft shadow; `raised` lifts it further, for the card under review.
pub fn card(e: El, k: &Kit, radius: f32, raised: bool) -> El {
    let e = e.radius(radius).fill(k.c.layer_01);
    let e = if k.dark {
        e.border_gradient(0.75, k.light(0.2), k.light(0.06))
    } else {
        e.border_gradient(0.5, k.dark_a(0.05), k.dark_a(0.09))
    };
    match (raised, k.dark) {
        (true, true) => e.shadow(8.0, 32.0, k.dark_a(0.3)),
        (true, false) => e.shadow(8.0, 32.0, k.dark_a(0.08)),
        (false, true) => e.shadow(1.0, 4.0, k.dark_a(0.3)),
        (false, false) => e.shadow(1.0, 4.0, k.dark_a(0.04)),
    }
}

/// The dashed rule between a card's sides, the same in review, a card's detail and the editor. It runs the card's
/// full width, so the sides around it carry their own inset.
pub fn side_rule(k: &Kit, key: &str) -> El {
    side_line(k, key).my(RULE_GAP)
}

/// The side rule's line alone, for a tile's footer, where it marks the sides the front hides.
pub fn side_line(k: &Kit, key: &str) -> El {
    dashed(k.alpha(k.c.text_base, 0.33), key, 4.0, 4.0)
}

/// A dashed line across its parent in `color`: the top edge of a dashed box, clipped to one point.
pub fn dashed(color: Color, key: &str, dash: f32, gap: f32) -> El {
    el().key(key.to_owned())
        .w_full()
        .h(1.0)
        .clip_rounded(0.0)
        .child(el().absolute(0.0, 0.0).w_full().h(8.0).border(1.0, color).border_dash(dash, gap))
}

/// A card's tag: `#tag` in the accent on a capsule of it.
pub fn tag(k: &Kit, tag: &str) -> El {
    rune::text(format!("#{tag}"))
        .key(format!("tag-{tag}"))
        .font(TAG)
        .color(accent(k))
        .px(8.0)
        .py(3.0)
        .radius(12.0)
        .fill(k.alpha(accent(k), 0.18))
}
