//! Reviewing: the front, a click or Space reveals the next side, and Space on the last is Remembered; F or ✕ is
//! Forgot and ⇧Space or ✓ Remembered at any side, and on to the next card. E edits the card where it is, D deletes
//! it and ⌃M moves it. A deck's review, the inbox (every due card from every deck, from the top of the sidebar), or a
//! cram of the cards a deck's list showed, which answers nothing to the core.

use rune::{El, KeyEvent, ScrollSpec, Spring, SpringPair, Transition, Tx, col, icon, row, text};
use rune_kit::shell::Toolbar;
use rune_kit::{Button, ButtonVariant, Combobox, ComboboxOption, ControlSize, Kit, Markdown, MarkdownStyle};

use super::{editor, error_line, format, icons, toolbar};
use crate::app::{ANSWERS, App, Review};
use crate::core::now_ms;
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
    if r.card().is_none() {
        // Caught up: Space finishes a deck's review, as Esc does.
        if !inbox && (pressed(Action::Advance) || pressed(Action::EndReview)) {
            s.end_review();
            return true;
        }
        return false;
    }
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
        Some(Action::Advance) => s.advance(),
        Some(Action::Reveal) => s.reveal(),
        Some(Action::Hide) => s.hide(),
        Some(Action::Remember) => s.answer(true),
        Some(Action::Forget) => s.answer(false),
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

/// A side (or the tags) fading in as it slides `dy` points down into its place, and back out as it's hidden.
fn revealing(dy: f32) -> Transition {
    Transition::offset(0.0, dy).combined(Transition::opacity()).springs(SpringPair::with_exit_ratio(REVEAL, 0.8))
}

pub fn view(s: &App, r: &Review, k: &Kit, layer: &mut Vec<El>) -> (El, Toolbar) {
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

    let body = match (r.card(), r.inbox()) {
        (Some(_), _) => reviewing(s, r, k, layer),
        (None, _) if r.cram.is_some() => crammed(r, k),
        (None, true) => all_caught_up(r, k),
        (None, false) => caught_up(s, r, k),
    };
    (body, bar)
}

fn reviewing(s: &App, r: &Review, k: &Kit, layer: &mut Vec<El>) -> El {
    let card = r.card().expect("a card to review");
    let editing = s.editor.as_ref().filter(|e| e.inline && e.card.as_ref() == Some(&card.card.id));
    let style = MarkdownStyle::from_kit(k).body_font(theme::SIDE);
    let mut face = col().key("face").w_full().py(theme::SIDE_Y);
    if let Some(e) = editing {
        face = face.child(editor::inline_sides(e, k));
    } else {
        for (i, side) in card.sides.iter().enumerate().take(r.shown) {
            if i > 0 {
                face = face.child(theme::side_rule(k, &format!("rule-{i}")).transition(revealing(0.0)));
            }
            face = face.child(
                col()
                    .key(format!("side-{i}"))
                    .w_full()
                    .px(theme::SIDE_X)
                    .child(Markdown::new(side.clone()).render(&style))
                    .transition(revealing(-8.0)),
            );
        }
        // Tags would give the answer away, so they wait for the last side.
        if r.revealed() && !card.card.tags.is_empty() {
            let tags = card
                .card
                .tags
                .iter()
                .fold(row().key("tags").w_full().px(theme::SIDE_X).pt(22.0).gap(6.0).flex_wrap(), |tags, t| {
                    tags.child(theme::tag(k, t))
                });
            face = face.child(tags.transition(revealing(-8.0)));
        }
    }
    // A side is laid out at its place straight away while the card grows to it, so the card keeps it inside.
    let face = theme::card(
        col().key("surface").w_full().child(face.clip_rounded(theme::REVIEW_RADIUS)),
        k,
        theme::REVIEW_RADIUS,
        true,
    );

    // Keyed by card, so each card is a new one rather than the last one's text changing.
    let fit = |el: El| if s.full_width_cards() { el } else { el.max_w(CARD_W) };
    let mut stack = fit(col().key(format!("card-{}", card.card.id)).w_full().gap(10.0));
    // The one motion in the app, as on iOS: showing a side grows the card to fit it, and the hint under it follows.
    // A new card, editing and deleting land at once, as everything else does.
    if r.revealing && editing.is_none() && !r.confirm_delete {
        stack = stack.animate_changes().motion(REVEAL);
    }
    // Which deck it's from: always in the inbox, and in a deck's review when it's from a subdeck. Pressing it (or
    // ⌃M, which shows it) picks another deck for the card.
    if r.deck.as_ref() != Some(&card.card.deck_id) || r.moving.open {
        let picker = deck_picker(s, r, &card.card.deck_id, k, layer);
        stack = stack.child(picker.instant_changes().motion(Spring::NONE).transition(Transition::none()));
    }
    stack = stack.child(face);
    let hint = if editing.is_some() || r.confirm_delete {
        None
    } else if r.revealed() {
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
                .transition(Transition::none()),
        );
    }

    let stage = col()
        .key("stage")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(16.0)
        .child(col().key("stage-pad").w_full().items_center().px(32.0).pt(16.0).pb(24.0).child(stack));
    let mut page = col().key("review").w_full().h_full().min_h(0.0).pb(24.0).gap(12.0).items_center().child(stage);
    if let Some(e) = &s.error {
        page = page.child(error_line(k, e));
    }
    let controls = match editing {
        Some(e) => editor::inline_footer(e, k),
        None if r.confirm_delete => confirm_delete(k),
        None => answers(k),
    };
    let page = page.child(col().key("controls").w_full().px(32.0).items_center().child(fit(controls)));
    // A click anywhere but the buttons shows the next side.
    if r.revealed() || editing.is_some() || r.confirm_delete { page } else { page.on_press(|s: &mut App| s.reveal()) }
}

/// The card's deck, which opens a searchable list of every deck to move it to.
fn deck_picker(s: &App, r: &Review, deck: &str, k: &Kit, layer: &mut Vec<El>) -> El {
    let trigger = row()
        .key("deck")
        .pl(4.0)
        .pr(6.0)
        .gap(6.0)
        .center_y()
        .radius(k.radius.md)
        .hover(k.c.surface_hover)
        .help(format!("Move to another deck  {}", keys::hint(Action::Move)))
        .child(icon(&icons::DECK, 16.0).key("icon").color(k.c.icon_faint))
        .child(text(s.deck_path(deck)).key("name").font(k.ty.ui).color(k.c.text_faint));
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

/// ✕ and ✓, as wide as the card can get, taking their answer's colour under the pointer.
fn answers(k: &Kit) -> El {
    let buttons = ANSWERS.iter().fold(row().key("answers").gap(8.0), |r, &(remembered, label, action)| {
        let (glyph, colour) =
            if remembered { (&icons::CHECK, k.c.success.icon) } else { (&icons::CROSS, k.c.danger.icon) };
        r.child(
            Button::new(label)
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::ExtraLarge)
                .shortcut(keys::hint(action))
                .label_with(move |_, ink| icon(glyph, 22.0).key("icon").color(ink).group_color(colour))
                .on_press(move |s: &mut App| s.answer(remembered))
                .build(k)
                .hover_group()
                .h(54.0)
                .grow(1.0)
                .key(format!("answer-{}", label.to_lowercase())),
        )
    });
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
