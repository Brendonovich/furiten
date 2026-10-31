//! A deck's screen: "Review" in the title bar, then the view switcher, the search and "New card", then its cards as
//! a grid, a list or a column. On first run, the sample deck with one action.

use rune::{El, Key, KeyEvent, Shadow, Tx, col, el, icon, row, text};
use rune_kit::shell::Toolbar;
use rune_kit::{
    Button, ButtonVariant, ControlSize, Keybind, KeybindVariant, Kit, Markdown, MarkdownStyle, Menu, MenuItem,
    TextInput,
};

use super::{cards, error_line, format, icons, keyed, toolbar};
use crate::app::{App, CardsView};
use crate::core::now_ms;
use crate::theme;

pub fn view(s: &App, k: &Kit, layer: &mut Vec<El>) -> (El, Toolbar) {
    if s.first_run {
        return (first_run(s, k), Toolbar::new());
    }
    let title = s.deck_summary().map_or("All decks".to_string(), |d| d.deck.name.clone());
    (body(s, k, &title, layer), toolbar(title).trailing(review(s, k)))
}

/// "Review" with how many cards are waiting, or when the next one comes due.
fn review(s: &App, k: &Kit) -> El {
    let (due, new) = s.counts();
    if due + new == 0 {
        let when = match s.next_due() {
            Some(at) => format!("Next card {}", format::until(at, now_ms())),
            None => "Nothing due".to_string(),
        };
        return text(when).key("next").font(k.ty.ui).color(k.c.text_faint);
    }
    let count = (due + new).to_string();
    Button::new("Review")
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Large)
        .help("Start review")
        .shortcut("Space")
        .label_with(move |font, ink| {
            row()
                .gap(8.0)
                .center_y()
                .child(text("Review").key("label").font(font).color(ink))
                .child(text(count).key("count").font(theme::COUNT).color(ink.alpha(0.6)))
        })
        .on_press(|s: &mut App| s.start_review())
        .build(k)
        .key("review")
}

fn view_icon(v: CardsView) -> &'static rune::Icon {
    match v {
        CardsView::Grid => &icons::GRID,
        CardsView::List => &icons::LIST,
        CardsView::Column => &icons::COLUMN,
    }
}

/// "Grid view ⌄" and its menu of the three views.
fn view_menu(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    let label = s.view.label();
    let chevron = k.glyph.chevron_down;
    let trigger = Button::new(label)
        .icon(view_icon(s.view))
        .variant(ButtonVariant::Ghost)
        .expanded(s.picking_view)
        .help("Lay out cards as a grid, a list or a column")
        .label_with(move |font, ink| {
            row()
                .gap(4.0)
                .center_y()
                .child(text(label).key("label").font(font).color(ink))
                .child(icon(chevron, 12.0).key("chevron").color(ink.alpha(0.6)))
        })
        .on_press(|s: &mut App| s.picking_view ^= true)
        .build(k)
        .key("view-button");
    let mut menu = Menu::new("view")
        .trigger(trigger)
        .min_width(184.0)
        .open(s.picking_view)
        .on_close(|s: &mut App| s.picking_view = false);
    for v in CardsView::ALL {
        menu = menu.item(
            MenuItem::new(v.label())
                .icon(view_icon(v))
                .shortcut(v.shortcut().0)
                .checked(s.view == v)
                .on_select_tx(move |s: &mut App, tx: &mut Tx| switch_view(s, v, tx)),
        );
    }
    menu.build(k, layer)
}

/// Lays the cards out another way. The content changes at once, nothing slides, and the selection stays in view.
fn switch_view(s: &mut App, v: CardsView, tx: &mut Tx) {
    tx.instant();
    s.set_view(v);
    if let Some(id) = s.current().map(|c| c.card.id.clone()) {
        tx.scroll_into_view(cards::row_key(s, &id));
    }
}

fn search(s: &App, k: &Kit, width: Option<f32>) -> El {
    let mut input = TextInput::new(&s.search)
        .placeholder("Search cards")
        .leading_icon(k.glyph.search)
        .width(width)
        .on_edit(|s: &mut App| &mut s.search)
        .on_change(|s: &mut App, tx: &mut Tx| s.search_edited(tx))
        .on_key(search_key);
    if !s.search.text.is_empty() {
        input = input.clear_button().on_action(|s: &mut App, _: &mut Tx| s.leave_search());
    } else if !s.search.focused {
        input = input.trailing(Keybind::new(&["/"]).variant(KeybindVariant::Ghost).build(k).key("search-key"));
    }
    input.build(k).key("search")
}

/// In the search field, the arrows move through the results, Return opens one and Escape clears or leaves.
fn search_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    match ev.key {
        Key::Escape => s.leave_search(),
        Key::Down => step(s, 0, 1, tx),
        Key::Up => step(s, 0, -1, tx),
        Key::Enter => {
            s.apply_search();
            if s.current().is_none() {
                step(s, 0, 0, tx);
            }
            if !s.open_selected() {
                return false;
            }
        }
        _ => return false,
    }
    true
}

fn step(s: &mut App, dx: i32, dy: i32, tx: &mut Tx) {
    if let Some(id) = s.move_selection(dx, dy) {
        // A scroll request glides unless its commit is instant.
        tx.instant();
        tx.scroll_into_view(cards::row_key(s, &id));
    }
}

/// The deck screen's keys while nothing (or a tile) has focus.
pub fn key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let plain = !(ev.mods.cmd || ev.mods.ctrl || ev.mods.alt);
    let (due, new) = s.counts();
    if s.first_run {
        match &ev.key {
            Key::Text(t) if t == " " && plain && due + new > 0 => s.start_review(),
            Key::Enter if plain && due + new > 0 => s.start_review(),
            _ => return false,
        }
        return true;
    }
    if let Some(v) = CardsView::ALL.into_iter().find(|v| ev.is_command(v.shortcut().1)) {
        switch_view(s, v, tx);
        return true;
    }
    if s.picking_view || !plain {
        return false;
    }
    match &ev.key {
        Key::Text(t) if t.eq_ignore_ascii_case("n") => s.new_card(),
        Key::Text(t) if t == "/" => s.focus_search(),
        Key::Text(t) if t == " " && due + new > 0 => s.start_review(),
        Key::Up => step(s, 0, -1, tx),
        Key::Down => step(s, 0, 1, tx),
        Key::Left => step(s, -1, 0, tx),
        Key::Right => step(s, 1, 0, tx),
        Key::Enter => {
            if !s.open_selected() {
                if due + new == 0 {
                    return false;
                }
                s.start_review();
            }
        }
        Key::Escape if s.detail_focused => s.detail_focused = false,
        Key::Escape if s.selected.is_some() => s.selected = None,
        _ => return false,
    }
    true
}

fn new_card(k: &Kit) -> El {
    Button::new("New card")
        .icon(&icons::PLUS)
        .variant(ButtonVariant::Ghost)
        .help("New card")
        .shortcut("N")
        .label_with(keyed("New card", "N", k, false))
        .on_press(|s: &mut App| s.new_card())
        .build(k)
        .key("new-card")
}

fn body(s: &App, k: &Kit, title: &str, layer: &mut Vec<El>) -> El {
    let column = s.view == CardsView::Column;
    // A fixed height, so switching to the column view (whose search moves down) doesn't move anything.
    let mut tools = row().key("tools").w_full().h(32.0).px(4.0).gap(10.0).center_y().child(view_menu(s, k, layer));
    if !column {
        tools = tools.child(col().key("search-box").grow(1.0).max_w(320.0).min_w(180.0).child(search(s, k, None)));
    }
    tools = tools.child(el().key("spacer").grow(1.0)).child(new_card(k));
    let mut page = col().key("deck").w_full().h_full().min_h(0.0).px(24.0).pt(4.0).gap(14.0).child(tools);
    if let Some(e) = &s.error {
        page = page.child(error_line(k, e));
    }
    if s.cards.is_empty() && s.query.is_empty() && s.search.text.is_empty() {
        return page.child(empty(k, title, s.deck.is_some()));
    }
    let none = s.cards.is_empty().then(|| no_results(s, k));
    let content = match (s.view, none) {
        (CardsView::Column, none) => cards::column(s, k, search(s, k, None), none),
        (_, Some(none)) => none,
        (CardsView::Grid, None) => cards::grid(s, k),
        (CardsView::List, None) => cards::list(s, k),
    };
    // One area for every view, so the grid knows its width before it lays out a row.
    page.child(
        col()
            .key(format!("cards-{:?}", s.view))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .on_layout(|s: &mut App, r: [f32; 4]| {
                if (s.cards_width - r[2]).abs() > 0.5 {
                    s.cards_width = r[2];
                }
            })
            .child(content),
    )
}

/// A search that found nothing, with the one way out.
fn no_results(s: &App, k: &Kit) -> El {
    col()
        .key("no-results")
        .w_full()
        .grow(1.0)
        .center()
        .gap(12.0)
        .child(text(format!("No cards match “{}”", s.search.text.trim())).font(k.ty.body_medium).color(k.c.text_base))
        .child(
            Button::new("Clear search")
                .variant(ButtonVariant::Neutral)
                .shortcut("Escape")
                .on_press(|s: &mut App| {
                    s.leave_search();
                })
                .build(k)
                .key("clear-search"),
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
                .shortcut("N")
                .on_press(|s: &mut App| s.new_card())
                .build(k)
                .key("add-card"),
        )
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
