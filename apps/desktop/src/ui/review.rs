//! Reviewing: the front, Space to reveal, 1–4 to grade, and on to the next card.

use rune::{El, col, row, text};
use rune_kit::shell::Toolbar;
use rune_kit::{Button, ButtonVariant, ControlSize, Divider, Kit, Markdown, MarkdownStyle};

use super::{error_line, format, keyed, toolbar};
use crate::app::{App, RATINGS, Review};
use crate::core::now_ms;
use crate::theme;

pub fn view(s: &App, r: &Review, k: &Kit) -> (El, Toolbar) {
    let title = s.deck_summary().map_or("All decks".to_string(), |d| d.deck.name.clone());
    let remaining = r.next.remaining;
    let counts =
        text(format::counts(remaining.due, remaining.new)).key("remaining").font(theme::COUNT).color(k.c.text_muted);
    let done = Button::new("Done")
        .variant(ButtonVariant::Ghost)
        .help("End review")
        .shortcut("Escape")
        .on_press(|s: &mut App| s.end_review())
        .build(k)
        .key("end-review");
    let bar = toolbar(title).trailing(row().key("review-tools").gap(12.0).center_y().child(counts).child(done));

    let body = match r.card() {
        Some(_) => reviewing(s, r, k),
        None => caught_up(s, r, k),
    };
    (body, bar)
}

fn reviewing(s: &App, r: &Review, k: &Kit) -> El {
    let card = r.card().expect("a card to review");
    let style = MarkdownStyle::from_kit(k).body_font(theme::CARD_FACE);
    let mut face = col()
        .key(format!("card-{}", card.card.id))
        .w_full()
        .max_w(620.0)
        .padding(32.0)
        .gap(20.0)
        .radius(k.radius.xl)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .child(col().key("side-0").w_full().child(Markdown::new(card.sides[0].clone()).render(&style)));
    for (i, side) in card.sides.iter().enumerate().take(r.shown).skip(1) {
        face = face.child(
            col()
                .key(format!("side-{i}"))
                .w_full()
                .gap(20.0)
                .child(Divider::horizontal().build(k))
                .child(Markdown::new(side.clone()).render(&style)),
        );
    }

    let controls = if r.revealed() { grades(k) } else { reveal(k) };
    let mut page = col()
        .key("review")
        .w_full()
        .h_full()
        .min_h(0.0)
        .px(32.0)
        .pt(40.0)
        .pb(28.0)
        .gap(24.0)
        .items_center()
        .child(col().key("stage").w_full().grow(1.0).min_h(0.0).items_center().child(face));
    if let Some(e) = &s.error {
        page = page.child(error_line(k, e));
    }
    page.child(
        col()
            .key("controls")
            .items_center()
            .gap(12.0)
            // Keyed by card and phase, so a press doesn't leave focus on a button that carries over.
            .child(controls.key(format!("controls-{}-{}", card.card.id, r.revealed())))
            .child(last_answer(r, k)),
    )
}

fn reveal(k: &Kit) -> El {
    row().gap(10.0).center_y().child(
        Button::new("Show answer")
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::ExtraLarge)
            .label_with(keyed("Show answer", "Space", k, true))
            .on_press(|s: &mut App| s.reveal())
            .build(k)
            .key("reveal"),
    )
}

fn grades(k: &Kit) -> El {
    RATINGS.iter().fold(row().gap(8.0).center_y(), |r, &(rating, label)| {
        r.child(
            Button::new(label)
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::ExtraLarge)
                .label_with(keyed(label, &rating.to_string(), k, false))
                .on_press(move |s: &mut App| s.grade(rating))
                .build(k)
                .key(format!("grade-{rating}")),
        )
    })
}

/// "Good · back in 10 minutes": the card you just answered, scheduled.
fn last_answer(r: &Review, k: &Kit) -> El {
    let Some(last) = r.last else { return row().key("last").h(16.0) };
    let label = RATINGS.iter().find(|(n, _)| *n == last.rating).map_or("", |(_, l)| l);
    row().key("last").h(16.0).center_y().child(
        text(format!("{label} · back {}", format::until(last.due, now_ms())))
            .key("last-text")
            .font(theme::COUNT)
            .color(k.c.text_faint),
    )
}

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
