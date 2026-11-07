//! Reviewing: the front, a click or Space reveals the next side, 1 forgot or 2 remembered at any side, and on to
//! the next card. A deck's review, or the inbox: every due card from every deck, from the top of the sidebar.

use rune::{El, ScrollSpec, col, icon, row, text};
use rune_kit::shell::Toolbar;
use rune_kit::{Badge, BadgeVariant, Button, ButtonVariant, ControlSize, Kit, Markdown, MarkdownStyle};

use super::{cards, error_line, format, icons, toolbar};
use crate::app::{ANSWERS, App, Review};
use crate::core::now_ms;
use crate::theme;

/// The card grows to fit its text between these, and the answer buttons are as wide as the widest.
const CARD_MIN_W: f32 = 420.0;
const CARD_MAX_W: f32 = 760.0;

pub fn view(s: &App, r: &Review, k: &Kit) -> (El, Toolbar) {
    let title = match &r.deck {
        Some(id) => s.deck_name(id).to_string(),
        None => "Review".to_string(),
    };
    let remaining = r.next.remaining;
    let counts =
        text(format::counts(remaining.due, remaining.new)).key("remaining").font(theme::COUNT).color(k.c.text_muted);
    let mut tools = row().key("review-tools").gap(12.0).center_y().child(counts);
    // The inbox is a place in the sidebar, not something to finish.
    if !r.inbox() {
        tools = tools.child(
            Button::new("Done")
                .variant(ButtonVariant::Ghost)
                .help("End review")
                .shortcut("Escape")
                .on_press(|s: &mut App| s.end_review())
                .build(k)
                .key("end-review"),
        );
    }
    let bar = toolbar(title).trailing(tools);

    let body = match (r.card(), r.inbox()) {
        (Some(_), _) => reviewing(s, r, k),
        (None, true) => all_caught_up(r, k),
        (None, false) => caught_up(s, r, k),
    };
    (body, bar)
}

fn reviewing(s: &App, r: &Review, k: &Kit) -> El {
    let card = r.card().expect("a card to review");
    let style = MarkdownStyle::from_kit(k).body_font(theme::REVIEW_FACE);
    let mut face = col().key("face").w_full().py(28.0).gap(22.0).radius(k.radius.xl).fill(k.c.layer_01);
    for (i, side) in card.sides.iter().enumerate().take(r.shown) {
        if i > 0 {
            face = face.child(cards::dotted(k, &format!("rule-{i}")));
        }
        face = face
            .child(col().key(format!("side-{i}")).w_full().px(32.0).child(Markdown::new(side.clone()).render(&style)));
    }
    // Tags would give the answer away, so they wait for the last side.
    if r.revealed() && !card.card.tags.is_empty() {
        let tags = card.card.tags.iter().fold(row().key("tags").w_full().px(32.0).gap(6.0).flex_wrap(), |tags, t| {
            tags.child(Badge::new(format!("#{t}")).variant(BadgeVariant::Subtle).build(k).key(format!("tag-{t}")))
        });
        face = face.child(tags);
    }
    let face = k.elevate(face, &k.elevation.raised);

    // Keyed by card, so each card is a new one rather than the last one's text changing.
    let mut stack = col().key(format!("card-{}", card.card.id)).min_w(CARD_MIN_W).max_w(CARD_MAX_W).gap(10.0);
    // Which deck it's from: always in the inbox, and in a deck's review when it's from a subdeck.
    if r.deck.as_ref() != Some(&card.card.deck_id) {
        stack = stack.child(
            row()
                .key("deck")
                .pl(4.0)
                .gap(6.0)
                .center_y()
                .child(icon(&icons::DECK, 16.0).key("icon").color(k.c.icon_faint))
                .child(text(s.deck_path(&card.card.deck_id)).key("name").font(k.ty.ui).color(k.c.text_faint)),
        );
    }
    stack = stack.child(face);
    if !r.revealed() {
        stack = stack.child(
            text("Click to reveal the next side")
                .key("hint")
                .font(k.ty.ui)
                .color(k.c.text_faint)
                .w_full()
                .align(rune::Align::Center)
                .mt(10.0),
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
    let page = page.child(answers(k));
    // A click anywhere but the buttons shows the next side.
    if r.revealed() { page } else { page.on_press(|s: &mut App| s.reveal()) }
}

/// ✕ and ✓, as wide as the card can get, taking their answer's colour under the pointer.
fn answers(k: &Kit) -> El {
    let buttons = ANSWERS.iter().fold(row().key("answers").gap(8.0), |r, &(remembered, label, key)| {
        let (glyph, colour) =
            if remembered { (&icons::CHECK, k.c.success.icon) } else { (&icons::CROSS, k.c.danger.icon) };
        r.child(
            Button::new(label)
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::ExtraLarge)
                .shortcut(key)
                .label_with(move |_, ink| icon(glyph, 22.0).key("icon").color(ink).group_color(colour))
                .on_press(move |s: &mut App| s.answer(remembered))
                .build(k)
                .hover_group()
                .h(54.0)
                .grow(1.0)
                .key(format!("answer-{}", label.to_lowercase())),
        )
    });
    col().key("controls").w_full().px(32.0).items_center().child(buttons.w_full().max_w(CARD_MAX_W))
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
                        .shortcut("cmd+n")
                        .on_press(|s: &mut App| s.new_card())
                        .build(k)
                        .key("add-cards"),
                )
                .child(
                    Button::new("Done")
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Large)
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
