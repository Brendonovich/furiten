//! Reviewing: the front, a click or Space reveals the next side, and Space on the last is Remembered; F or ✕ is
//! Forgot and ⇧Space or ✓ Remembered at any side, and on to the next card. E edits the card where it is, D deletes
//! it and ⌃M moves it. A deck's review, the inbox (every due card from every deck, from the top of the sidebar), or a
//! cram of the cards a deck's list showed, which answers nothing to the core.

use rune::{
    Anim, Curve, Cx, Easing, El, KeyEvent, ScrollSpec, Spring, SpringPair, Transition, Tx, col, el, icon, row, text,
};
use rune_kit::shell::Toolbar;
use rune_kit::{Button, ButtonVariant, Combobox, ComboboxOption, ControlSize, Kit, Markdown, MarkdownStyle};

use super::{editor, error_line, format, icons, toolbar};
use crate::app::{ANSWERS, App, FLY_OFF, Leaving, Review};
use crate::core::{CardView, now_ms};
use crate::keys::{self, Action};
use crate::theme;

/// The review's keys while nothing (or a button) has focus.
pub fn key(s: &mut App, ev: &KeyEvent, _: &mut Tx) -> bool {
    let Some(r) = s.review.as_ref() else { return false };
    let pressed = |action| keys::pressed(ev, action);
    // The deck picker's search field has the keys.
    if r.moving.open {
        return false;
    }
    if r.confirm_delete {
        if pressed(Action::Delete) {
            s.delete_review_card();
        } else if pressed(Action::EndReview) {
            s.keep_review_card();
        } else {
            return false;
        }
        return true;
    }
    let inbox = r.inbox();
    if r.card().is_none() && r.leaving.is_none() {
        // Caught up: Space finishes a deck's review, as Esc does.
        if !inbox && (pressed(Action::Advance) || pressed(Action::EndReview)) {
            s.end_review();
            return true;
        }
        return false;
    }
    // A card still coming in can't be answered: Space would reveal it, and F would grade one that isn't there yet.
    let ready = settled(s.clock, r);
    match [
        Action::Advance,
        Action::Reveal,
        Action::Hide,
        Action::Remember,
        Action::Forget,
        Action::Edit,
        Action::Delete,
        Action::Move,
        Action::EndReview,
    ]
    .into_iter()
    .find(|a| pressed(*a))
    {
        Some(Action::Advance) if ready || !r.revealed() => s.advance(),
        Some(Action::Reveal) => s.reveal(),
        Some(Action::Hide) => s.hide(),
        Some(Action::Remember) if ready => s.answer(true),
        Some(Action::Forget) if ready => s.answer(false),
        Some(Action::Edit) => s.edit_review_card(),
        Some(Action::Delete) => s.delete_review_card(),
        Some(Action::Move) => s.start_move(),
        Some(Action::EndReview) if !inbox => s.end_review(),
        _ => return false,
    }
    true
}

/// How wide a card is, unless cards are full width; a narrower window narrows it. The answer buttons match it. It
/// never follows the text, so showing the next side doesn't change it.
const CARD_W: f32 = 760.0;

/// How the card grows to show a side, and the side comes in: iOS's `ReviewCurve.reveal` (the web's `.reveal-in`, a
/// 0.28 s `cubic-bezier(0.2, 0.8, 0.2, 1)`), as the spring that follows that curve most closely, since transitions
/// only take springs.
const REVEAL: Spring = Spring::new(0.14, 0.0);

/// The next card rising in: iOS's `ReviewCurve.cardIn` (the web's `@keyframes card-in`, a 0.32 s
/// `cubic-bezier(0.2, 0.8, 0.2, 1)`), as the spring that follows that curve most closely.
const CARD_IN: Spring = Spring::new(0.16, 0.0);

/// An answered card flying off: it starts away at once and eases out, on iOS's `ReviewCurve.flyOff` (the web's
/// `.review-card.leaving`), `cubic-bezier(0.25, 0.6, 0.4, 1)`, over `FLY_OFF`.
const FLY_OFF_ANIM: Anim = Anim::new(Curve::tween(FLY_OFF as f32, Easing::cubic(0.25, 0.6, 0.4, 1.0)));

/// The card fading as it goes, behind the movement (iOS's and the web's same curve for transform and opacity).
const FADE_OUT: Anim = Anim::new(Curve::tween(FLY_OFF as f32, Easing::cubic(0.25, 0.6, 0.4, 1.0)));

/// The deck's name and the hint fading with the card, evenly (the web's `.review-deck` and `.tap-hint.settling`).
const TEXT_FADE: Anim = Anim::new(Curve::tween(0.22, Easing::LINEAR));

/// The answer buttons taking (and letting go of) their colour: iOS's `ReviewCurve.button` (CSS's `ease` over 0.12 s).
const BUTTON: Anim = Anim::new(Curve::tween(0.12, Easing::EASE));

/// [`BUTTON`] for the buttons' icons, whose colours move on a spring.
const BUTTON_COLOUR: Spring = Spring::new(0.06, 0.0);

/// The answered card's edge taking its answer's colour: iOS's `outline-color 0.15s ease-out`, a touch longer so it
/// reads against the card already moving.
const EDGE_IN: Anim = Anim::new(Curve::tween(0.18, Easing::EASE_OUT));

/// A side (or the tags) fading in as it slides `dy` points down into its place, and back out as it's hidden.
fn revealing(dy: f32) -> Transition {
    Transition::offset(0.0, dy).combined(Transition::opacity()).springs(SpringPair::with_exit_ratio(REVEAL, 0.8))
}

/// A card coming in: it fades in, up from 16 points below, at its size.
fn arriving() -> Transition {
    Transition::asymmetric(Transition::offset(0.0, 16.0).combined(Transition::opacity()), Transition::opacity())
        .springs(SpringPair::with_exit_ratio(CARD_IN, 0.75))
}

/// The colour an answer takes: remembered is the success ink, forgot the danger, as each button's icon has it.
fn answer_colour(k: &Kit, remembered: bool) -> rune::Color {
    if remembered { k.c.success.icon } else { k.c.danger.icon }
}

/// The glow an answered card takes in its answer's colour, as iOS's `SwipeEdge`: a hairline edge with a glow around
/// it, fading in as the card goes. It fills with the card's own colour so the shadow's inside, which Rune draws
/// behind a transparent element, doesn't show through the card.
fn answer_edge(remembered: bool, shown: bool, k: &Kit) -> El {
    let colour = answer_colour(k, remembered);
    el().key("answer-edge")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .radius(theme::REVIEW_RADIUS)
        .fill(k.c.layer_01)
        .border(1.0, colour)
        .shadow(0.0, 8.0, colour.alpha(0.5))
        .opacity(if shown { 1.0 } else { 0.0 })
        .opacity_motion(EDGE_IN)
        // The card flies in a layer that lands its changes at once; the edge's colour catches up on its own.
        .animate_changes()
}

/// Whether an answer can be given yet: for a moment after one, the answered card is still flying off and the next
/// hasn't come up, so an answer then would grade a card that isn't there yet (iOS's `busy`).
fn settled(clock: f64, r: &Review) -> bool {
    r.leaving.as_ref().is_none_or(|left| clock - left.at >= FLY_OFF)
}

pub fn view(s: &App, r: &Review, k: &Kit, layer: &mut Vec<El>, cx: &mut Cx<App>) -> (El, Toolbar) {
    let title = match &r.deck {
        Some(id) => s.deck_name(id).to_string(),
        None => "Review".to_string(),
    };
    let remaining = r.next.remaining;
    let counts = match &r.cram {
        Some(cram) => format!("{} left", cram.left()),
        None => format::counts(remaining.due, remaining.new),
    };
    let counts = text(counts).key("remaining").font(theme::COUNT).color(k.c.text_muted);
    let mut tools = row().key("review-tools").gap(12.0).center_y().child(counts);
    // The inbox is a place in the sidebar, not something to finish.
    if !r.inbox() {
        tools = tools.child(
            Button::new("Done")
                .variant(ButtonVariant::Ghost)
                .help("End review")
                .shortcut(keys::hint(Action::EndReview))
                .on_press(|s: &mut App| s.end_review())
                .build(k)
                .key("end-review"),
        );
    }
    let bar = toolbar(title).trailing(tools);

    // An answered card is still the one on screen while it flies off; the review has already moved on behind it.
    let showing = match &r.leaving {
        Some(left) => Some((&left.card, left.shown)),
        None => r.card().map(|card| (card, r.shown)),
    };
    let body = match (showing, r.inbox()) {
        (Some((card, shown)), _) => reviewing(s, r, card, shown, k, layer, cx),
        (None, _) if r.cram.is_some() => crammed(r, k),
        (None, true) => all_caught_up(r, k),
        (None, false) => caught_up(s, r, k),
    };
    (body, bar)
}

fn reviewing(s: &App, r: &Review, card: &CardView, shown: usize, k: &Kit, layer: &mut Vec<El>, cx: &mut Cx<App>) -> El {
    let leaving = r.leaving.as_ref().filter(|left| left.card.card.id == card.card.id);
    let revealed = shown >= card.sides.len();
    let editing = s.editor.as_ref().filter(|e| e.inline && leaving.is_none() && e.card.as_ref() == Some(&card.card.id));
    let style = MarkdownStyle::from_kit(k).body_font(theme::SIDE);
    let face = match editing {
        Some(e) => col().key("face").w_full().py(theme::SIDE_Y).child(editor::inline_sides(e, k)),
        None => sides(card, shown, &style, k, ""),
    };
    // A card on its way out stays here (hidden) so nothing reflows behind it, and is drawn above the window instead,
    // where the stage's scroll can't clip it and the toolbar can't cover it.
    let mut surface = theme::card(
        col().key("surface").w_full().child(face.clip_rounded(theme::REVIEW_RADIUS)),
        k,
        theme::REVIEW_RADIUS,
        true,
    )
    .transition(Transition::none());
    if leaving.is_some_and(|left| left.flying) {
        surface = surface.opacity(0.0).opacity_motion(Anim::INSTANT);
    }
    let flying = leaving.map(|left| {
        let face = sides(card, shown, &style, k, "flying-");
        flying_card(left, face, k, cx)
    });

    // Keyed by card (and by how many have been answered, so a card that comes round again in a cram comes in afresh),
    // so each card is a new one rather than the last one's text changing. A card flying off keeps the key it had, so
    // it's the same element being sent off rather than a new one appearing in its place.
    let fit = |el: El| if s.full_width_cards() { el } else { el.max_w(CARD_W) };
    let key_card = if leaving.is_some() { r.answered.saturating_sub(1) } else { r.answered };
    let mut stack =
        fit(col().key(format!("card-{}-{}", card.card.id, key_card)).w_full().gap(10.0).transition(arriving()));
    // The one motion in the app, as on iOS: showing a side grows the card to fit it, the next card rises in, and the
    // hint under it follows. Editing and deleting land at once, as everything else does.
    if r.revealing && editing.is_none() && !r.confirm_delete {
        stack = stack.animate_changes().motion(REVEAL);
    }
    // Which deck it's from: always in the inbox, and in a deck's review when it's from a subdeck. Pressing it (or
    // ⌃M, which shows it) picks another deck for the card; a card on its way out just says it.
    if r.deck.as_ref() != Some(&card.card.deck_id) || r.moving.open {
        let deck = if leaving.is_some() {
            deck_label(s, &card.card.deck_id, k)
        } else {
            deck_picker(s, r, &card.card.deck_id, k, layer).instant_changes()
        };
        // It fades with the card and the hint.
        stack = stack.child(
            row()
                .key("deck-fade")
                .child(deck)
                .opacity(if leaving.is_some() { 0.0 } else { 1.0 })
                .opacity_motion(TEXT_FADE)
                .motion(Spring::NONE)
                .transition(Transition::none()),
        );
    }
    stack = stack.child(surface);
    let hint = if editing.is_some() || r.confirm_delete {
        None
    } else if revealed {
        Some("Press Space if you remembered it, or F if you forgot")
    } else {
        Some("Click or press Space to show the next side")
    };
    if let Some(hint) = hint {
        stack = stack.child(
            text(hint)
                .key("hint")
                .font(k.ty.ui)
                .color(k.c.text_faint)
                .w_full()
                .align(rune::Align::Center)
                .mt(10.0)
                // Fades as the card is answered, as iOS's hint does.
                .opacity(if leaving.is_some() { 0.0 } else { 1.0 })
                .opacity_motion(TEXT_FADE)
                .transition(Transition::none()),
        );
    }

    // A card rising in has to animate at all: it's inside the window's instant changes, so the stage says otherwise
    // while the card has something to do.
    let mut pad = col().key("stage-pad").w_full().items_center().px(32.0).pt(16.0).pb(24.0);
    if r.revealing {
        pad = pad.animate_changes();
    }
    let stage = col()
        .key("stage")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(16.0)
        .child(pad.child(stack));
    let mut page = col().key("review").w_full().h_full().min_h(0.0).pb(24.0).gap(12.0).items_center().child(stage);
    if let Some(e) = &s.error {
        page = page.child(error_line(k, e));
    }
    let controls = match editing {
        Some(e) => editor::inline_footer(e, k),
        None if r.confirm_delete => confirm_delete(k),
        None => answers(r, k),
    };
    let page = page.child(col().key("controls").w_full().px(32.0).items_center().child(fit(controls)));
    if let Some(flying) = flying {
        layer.push(flying);
    }
    // A click anywhere but the buttons shows the next side.
    if revealed || editing.is_some() || r.confirm_delete || leaving.is_some() {
        page
    } else {
        page.on_press(|s: &mut App| s.reveal())
    }
}

/// A card's sides, split by dashed rules, and its tags once every side shows. `prefix` keeps a copy drawn elsewhere
/// (the one flying above the window) from sharing keys with the card in the stage.
fn sides(card: &CardView, shown: usize, style: &MarkdownStyle, k: &Kit, prefix: &str) -> El {
    let revealed = shown >= card.sides.len();
    let mut face = col().key(format!("{prefix}face")).w_full().py(theme::SIDE_Y);
    for (i, side) in card.sides.iter().enumerate().take(shown) {
        if i > 0 {
            face = face.child(theme::side_rule(k, &format!("{prefix}rule-{i}")).transition(revealing(0.0)));
        }
        face = face.child(
            col()
                .key(format!("{prefix}side-{i}"))
                .w_full()
                .px(theme::SIDE_X)
                .child(Markdown::new(side.clone()).render(style))
                .transition(revealing(-8.0)),
        );
    }
    // Tags would give the answer away, so they wait for the last side.
    if revealed && !card.card.tags.is_empty() {
        let tags = card.card.tags.iter().fold(
            row().key(format!("{prefix}tags")).w_full().px(theme::SIDE_X).pt(22.0).gap(6.0).flex_wrap(),
            |tags, t| tags.child(theme::tag(k, t)),
        );
        face = face.child(tags.transition(revealing(-8.0)));
    }
    face
}

/// The answered card on its way out, in the window's overlay layer: the first frame in its place, then off to its
/// answer's side, tilting, fading, its edge taking the answer's colour.
fn flying_card(left: &Leaving, face: El, k: &Kit, cx: &mut Cx<App>) -> El {
    let rect = cx.window_layout("surface").unwrap_or([0.0, 0.0, CARD_W, 0.0]);
    let width = cx.layout_with("stage", |rect| rect[2]).unwrap_or(CARD_W);
    let inner = col()
        .key("flying-card")
        .w_full()
        // The card's contents land whole; only the edge and the card's own fade animate.
        .instant_changes()
        .transition(Transition::none())
        .child(answer_edge(left.remembered, left.flying, k))
        .child(face.clip_rounded(theme::REVIEW_RADIUS));
    // The card itself is still; a wrapper around it carries the flight, so the contents don't fade in with it.
    let card = theme::card(inner, k, theme::REVIEW_RADIUS, true).transition(Transition::none());
    let mut motion = el().key("flying-motion").w_full().animate_changes().child(card);
    if left.flying {
        let dx = if left.remembered { width } else { -width } * 0.65;
        motion = motion
            .offset(dx, 0.0)
            .offset_motion(FLY_OFF_ANIM)
            .rotate((dx / 24.0).to_radians())
            .rotate_motion(FLY_OFF_ANIM)
            .opacity(0.0)
            .opacity_motion(FADE_OUT);
    }
    el().key("flying")
        .pin(Some(rect[0]), Some(rect[1]), None, None)
        .w(rect[2])
        // It's in place a frame before it goes (hidden), so its text has a layout when it starts.
        .opacity(if left.flying { 1.0 } else { 0.0 })
        .transition(Transition::none())
        .instant_changes()
        .child(motion)
}

/// The card's deck, which opens a searchable list of every deck to move it to.
fn deck_picker(s: &App, r: &Review, deck: &str, k: &Kit, layer: &mut Vec<El>) -> El {
    let trigger = deck_label(s, deck, k)
        .radius(k.radius.md)
        .hover(k.c.surface_hover)
        .help(format!("Move to another deck  {}", keys::hint(Action::Move)));
    let options: Vec<ComboboxOption> =
        s.deck_tree().iter().map(|(d, _)| ComboboxOption::new(&d.deck.id, s.deck_path(&d.deck.id))).collect();
    let picker =
        Combobox::new("move", &r.moving, &options, |s: &mut App| &mut s.review.as_mut().expect("reviewing").moving)
            .current(Some(deck))
            .trigger(trigger)
            .search_placeholder("Move to deck")
            .on_choose(|s: &mut App, deck: &str, _: &mut Tx| s.move_review_card(deck))
            .build(k, layer);
    row().key("deck-row").child(picker)
}

/// The card's deck as the picker's trigger has it, for a card on its way out, which can't be moved.
fn deck_label(s: &App, deck: &str, k: &Kit) -> El {
    row()
        .key("deck")
        .pl(4.0)
        .pr(6.0)
        .gap(6.0)
        .center_y()
        .child(icon(&icons::DECK, 16.0).key("icon").color(k.c.icon_faint))
        .child(text(s.deck_path(deck)).key("name").font(k.ty.ui).color(k.c.text_faint))
}

/// After D: asks, where the answers were, before the card goes.
fn confirm_delete(k: &Kit) -> El {
    row()
        .key("confirm-delete")
        .h(54.0)
        .gap(6.0)
        .center_y()
        .child(text("Delete this card?").key("ask").font(k.ty.body).color(k.c.text_base).mr(10.0))
        .child(
            Button::new("Keep")
                .variant(ButtonVariant::Ghost)
                .shortcut("Esc")
                .on_press(|s: &mut App| s.keep_review_card())
                .build(k)
                .key("keep"),
        )
        .child(
            Button::new("Delete card")
                .variant(ButtonVariant::Danger)
                .shortcut(keys::hint(Action::Delete))
                .on_press(|s: &mut App| s.delete_review_card())
                .build(k)
                .key("delete"),
        )
}

/// ✕ and ✓, as wide as the card can get, taking their answer's colour under the pointer. An answered card on its way
/// out takes its answer's colour, and the other answer fades, as iOS's swipe does.
fn answers(r: &Review, k: &Kit) -> El {
    let leaving = r.leaving.as_ref().map(|left| left.remembered);
    let buttons = ANSWERS.iter().fold(
        row().key("answers").gap(8.0).animate_changes().motion(BUTTON_COLOUR),
        |row, &(remembered, label, action)| {
            let (glyph, colour) =
                if remembered { (&icons::CHECK, k.c.success.icon) } else { (&icons::CROSS, k.c.danger.icon) };
            let chosen = leaving == Some(remembered);
            let dim = leaving.is_some() && !chosen;
            row.child(
                Button::new(label)
                    .variant(ButtonVariant::Neutral)
                    .control_size(ControlSize::ExtraLarge)
                    .shortcut(keys::hint(action))
                    .label_with(move |_, ink| {
                        icon(glyph, 22.0).key("icon").color(if chosen { colour } else { ink }).group_color(colour)
                    })
                    .on_press(move |s: &mut App| {
                        let ready = s.review.as_ref().is_none_or(|r| settled(s.clock, r));
                        if ready {
                            s.answer(remembered);
                        }
                    })
                    .build(k)
                    .hover_group()
                    .opacity(if dim { 0.4 } else { 1.0 })
                    .opacity_motion(BUTTON)
                    .h(54.0)
                    .grow(1.0)
                    .key(format!("answer-{}", label.to_lowercase())),
            )
        },
    );
    buttons.w_full()
}

/// A deck's review with nothing left in it.
fn caught_up(s: &App, r: &Review, k: &Kit) -> El {
    let title = if r.answered > 0 { "That’s everything for now" } else { "Nothing to review" };
    let detail = match s.next_due() {
        Some(at) => format!("The next card is due {}.", format::until(at, now_ms())),
        None => "Add cards and they’ll show up here.".to_string(),
    };
    col()
        .key("caught-up")
        .w_full()
        .h_full()
        .center()
        .gap(10.0)
        .px(32.0)
        .child(text(title).key("title").font(theme::DISPLAY).color(k.c.text_base))
        .child(text(detail).key("detail").font(k.ty.body.lh(21.0)).color(k.c.text_muted))
        .child(
            row()
                .key("actions")
                .pt(14.0)
                .gap(8.0)
                .child(
                    Button::new("Add cards")
                        .variant(ButtonVariant::Contrast)
                        .control_size(ControlSize::Large)
                        .help("New card")
                        .shortcut(keys::hint(Action::NewCard))
                        .on_press(|s: &mut App| s.new_card())
                        .build(k)
                        .key("add-cards"),
                )
                .child(
                    Button::new("Done")
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Large)
                        .shortcut(keys::hint(Action::Advance))
                        .on_press(|s: &mut App| s.end_review())
                        .build(k)
                        .key("done"),
                ),
        )
}

/// A cram with every card remembered once.
fn crammed(r: &Review, k: &Kit) -> El {
    let n = r.cram.as_ref().map_or(0, |c| c.remembered());
    let detail = match n {
        0 => "Nothing left to review.".to_string(),
        n => format!(
            "You remembered {}, in {} answer{}. Nothing was scheduled.",
            if n == 1 { "the card".to_string() } else { format!("all {n} cards") },
            r.answered,
            if r.answered == 1 { "" } else { "s" },
        ),
    };
    col()
        .key("crammed")
        .w_full()
        .h_full()
        .center()
        .gap(10.0)
        .px(32.0)
        .child(text("That’s every card").key("title").font(theme::DISPLAY).color(k.c.text_base))
        .child(text(detail).key("detail").font(k.ty.body.lh(21.0)).color(k.c.text_muted))
        .child(
            row().key("actions").pt(14.0).child(
                Button::new("Done")
                    .variant(ButtonVariant::Contrast)
                    .control_size(ControlSize::Large)
                    .shortcut(keys::hint(Action::Advance))
                    .on_press(|s: &mut App| s.end_review())
                    .build(k)
                    .key("done"),
            ),
        )
}

/// The inbox with nothing left: every deck is done for now.
fn all_caught_up(r: &Review, k: &Kit) -> El {
    let title = if r.answered > 0 { "That’s everything for now" } else { "All caught up" };
    col()
        .key("all-caught-up")
        .w_full()
        .h_full()
        .center()
        .gap(10.0)
        .px(32.0)
        .child(text(title).key("title").font(theme::DISPLAY).color(k.c.text_base))
        .child(
            text("Cards from every deck show up here when they’re due.")
                .key("detail")
                .font(k.ty.body.lh(21.0))
                .color(k.c.text_muted),
        )
}
