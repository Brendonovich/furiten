//! A deck's screen: its counts, "Review", and its cards. On first run, the sample deck with one action.

use rune::{El, ScrollSpec, Shadow, col, el, row, text};
use rune_kit::shell::Toolbar;
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, Markdown, MarkdownStyle};

use super::{error_line, format, icons, keyed, toolbar};
use crate::app::App;
use crate::core::{CardView, State, now_ms};
use crate::theme;

pub fn view(s: &App, k: &Kit) -> (El, Toolbar) {
    if s.first_run {
        return (first_run(s, k), Toolbar::new());
    }
    let title = s.deck_summary().map_or("All decks".to_string(), |d| d.deck.name.clone());
    let new_card = Button::new("New card")
        .icon(&icons::PLUS)
        .variant(ButtonVariant::Ghost)
        .help("New card")
        .shortcut("cmd+n")
        .on_press(|s: &mut App| s.new_card())
        .build(k)
        .key("new-card");
    (body(s, k, &title), toolbar(title).trailing(new_card))
}

fn body(s: &App, k: &Kit, title: &str) -> El {
    let (due, new) = s.counts();
    let mut summary = col().key("summary").gap(4.0).grow(1.0).shrink().min_w(0.0).child(
        text(format!("{} · {}", format::cards(s.cards.len() as u64), format::counts(due, new)))
            .key("counts")
            .font(theme::COUNT.lh(18.0))
            .color(k.c.text_muted),
    );
    if due + new == 0
        && let Some(at) = s.next_due()
    {
        summary = summary.child(
            text(format!("Next card {}", format::until(at, now_ms()))).key("next").font(k.ty.ui).color(k.c.text_faint),
        );
    }
    let mut head = row().key("head").w_full().gap(12.0).center_y().child(summary);
    if due + new > 0 {
        head = head.child(
            Button::new("Review")
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::Large)
                .help("Start review")
                .shortcut("Space")
                .on_press(|s: &mut App| s.start_review())
                .build(k)
                .key("review"),
        );
    }
    let mut page = col().key("deck").w_full().h_full().min_h(0.0).px(24.0).pt(8.0).gap(16.0).child(head);
    if let Some(e) = &s.error {
        page = page.child(error_line(k, e));
    }
    if s.cards.is_empty() {
        return page.child(empty(k, title, s.deck.is_some()));
    }
    let list =
        s.cards.iter().fold(col().key("cards").w_full().gap(2.0).pb(24.0), |list, c| list.child(card_row(s, c, k)));
    page.child(
        col()
            .key("cards-scroll")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .scroll_edge_fade(16.0)
            .child(list),
    )
}

fn empty(k: &Kit, title: &str, in_deck: bool) -> El {
    let what = if in_deck { format!("No cards in {title} yet") } else { "No cards yet".to_string() };
    col()
        .key("empty")
        .w_full()
        .grow(1.0)
        .center()
        .gap(12.0)
        .child(text(what).font(k.ty.body_medium).color(k.c.text_base))
        .child(text("Write a question, a line with ---, then the answer.").font(k.ty.ui).color(k.c.text_muted))
        .child(
            Button::new("Add card")
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::Large)
                .shortcut("cmd+n")
                .on_press(|s: &mut App| s.new_card())
                .build(k)
                .key("add-card"),
        )
}

fn card_row(s: &App, c: &CardView, k: &Kit) -> El {
    let now = now_ms();
    let (when, ink) = match c.schedule.state {
        State::New => ("New".to_string(), k.c.text_faint),
        _ if c.schedule.due <= now => ("Due".to_string(), theme::accent(k)),
        _ => (format::until(c.schedule.due, now), k.c.text_faint),
    };
    let back = c.sides.get(1).map(|b| format::title(b)).unwrap_or_default();
    let mut lines = col().key("lines").grow(1.0).shrink().min_w(0.0).gap(2.0).child(
        text(format::title(c.sides.first().map_or("", String::as_str)))
            .key("front")
            .font(k.ty.ui)
            .color(k.c.text_base)
            .truncate(),
    );
    if !back.is_empty() {
        lines = lines.child(text(back).key("back").font(k.ty.caption()).color(k.c.text_faint).truncate());
    }
    let mut meta = row().key("meta").gap(8.0).center_y();
    if s.deck.is_none() {
        meta = meta
            .child(text(s.deck_name(&c.card.deck_id).to_owned()).key("deck").font(k.ty.small).color(k.c.text_faint));
    }
    for tag in c.card.tags.iter().take(2) {
        meta = meta.child(text(format!("#{tag}")).font(k.ty.small).color(k.c.text_faint));
    }
    meta = meta.child(text(when).key("when").font(theme::COUNT).color(ink));
    let id = c.card.id.clone();
    k.interactive(
        row()
            .key(format!("card-{}", c.card.id))
            .role("button")
            .label(format::title(c.sides.first().map_or("", String::as_str)))
            .w_full()
            .px(12.0)
            .py(8.0)
            .gap(16.0)
            .center_y()
            .radius(k.radius.md)
            .child(lines)
            .child(meta),
    )
    .on_press(move |s: &mut App| s.edit_card(&id))
}

/// The very first screen: the sample deck and one thing to do.
fn first_run(s: &App, k: &Kit) -> El {
    let deck = s.deck_summary();
    let name = deck.map_or("Recall basics".to_string(), |d| d.deck.name.clone());
    let count = deck.map_or(0, |d| d.cards);
    let front = s.cards.first().and_then(|c| c.sides.first()).cloned().unwrap_or_default();
    let start = row().key("start-row").gap(10.0).center_y().child(
        Button::new("Start review")
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::ExtraLarge)
            .label_with(keyed("Start review", "↩", k, true))
            .on_press(|s: &mut App| s.start_review())
            .build(k)
            .key("start-review")
            .default_button(),
    );
    let mut page = col()
        .key("first-run")
        .w_full()
        .h_full()
        .center()
        .gap(28.0)
        .px(32.0)
        .child(stack(k, &front))
        .child(
            col()
                .key("intro")
                .items_center()
                .gap(8.0)
                .child(text(name).key("title").font(theme::DISPLAY).color(k.c.text_base))
                .child(
                    text(format!(
                        "{count} cards that show you how Recall works. Reveal each answer, then say how well you knew it."
                    ))
                    .key("lede")
                    .font(k.ty.body.lh(21.0))
                    .color(k.c.text_muted)
                    .wrap()
                    .max_w(360.0)
                    .align(rune::Align::Center),
                ),
        )
        .child(start);
    if let Some(why) = &s.offline {
        page = page.child(offline_note(k, why).align(rune::Align::Center));
    }
    page
}

/// Said plainly when the core couldn't be reached and the app shows sample data instead.
pub fn offline_note(k: &Kit, why: &str) -> El {
    text(format!("Couldn’t start the Recall core, so this is sample data. {why}"))
        .key("offline")
        .font(k.ty.caption())
        .color(k.c.text_faint)
        .wrap()
        .max_w(420.0)
}

/// A small stack of cards with the first question on top: a taste of what's inside.
fn stack(k: &Kit, front: &str) -> El {
    let sheet = |key: &str| {
        k.elevate(
            el().key(key.to_owned())
                .absolute(0.0, 0.0)
                .size(300.0, 172.0)
                .radius(k.radius.xl)
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_muted),
            &[Shadow {
                y: 6.0,
                blur: 18.0,
                color: k.c.black.alpha(if k.dark { 0.35 } else { 0.07 }),
                ..Shadow::default()
            }],
        )
    };
    let top = col()
        .key("top")
        .absolute(0.0, 0.0)
        .size(300.0, 172.0)
        .padding(22.0)
        .gap(10.0)
        .radius(k.radius.xl)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .child(text("Front").key("side").font(k.ty.small_medium).color(k.c.text_faint))
        .child(Markdown::new(front.to_owned()).render(
            &MarkdownStyle::from_kit(k).body_font(rune::TextStyle { weight: 500.0, ..theme::CARD_FACE.lh(26.0) }),
        ));
    el().key("stack")
        .size(300.0, 188.0)
        .child(sheet("back-2").rotate(0.07).offset(10.0, 10.0).opacity(0.6))
        .child(sheet("back-1").rotate(-0.04).offset(-6.0, 6.0))
        .child(k.elevate(
            top,
            &[Shadow {
                y: 10.0,
                blur: 28.0,
                color: k.c.black.alpha(if k.dark { 0.4 } else { 0.10 }),
                ..Shadow::default()
            }],
        ))
}
