//! A deck's cards laid out as a grid, a list or a column with a detail pane. Each tile shows the card's front, how
//! many sides it hides and when it's due. Rows are virtual: only the ones on screen are built.

use std::borrow::Cow;

use rune::{El, GrowthSpec, ScrollSpec, TextStyle, col, el, icon, memo, row, text};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Keybind, KeybindVariant, Kit, Markdown, MarkdownStyle};

use super::{format, icons};
use crate::app::{App, CardsView, GRID_GAP};
use crate::core::{CardView, State, now_ms};
use crate::theme;

/// Room around the tiles for the selection ring, so the scroll viewport doesn't clip it.
const RING_ROOM: f32 = 4.0;

/// The tallest a grid tile's front gets before it fades out.
const GRID_FACE_MAX: f32 = 132.0;
const LIST_FACE_MAX: f32 = 200.0;

/// When a card is due, as a footer says it, and whether that's now.
fn due(c: &CardView, now: i64) -> (String, bool) {
    match c.schedule.state {
        State::New => ("new".into(), false),
        _ if c.schedule.due <= now => ("due".into(), true),
        _ => (format::until_short(c.schedule.due, now), false),
    }
}

/// The type a front is set in: a short front shows big, like a flashcard; a long one steps down (iOS `.title`
/// medium, `.title3`, then `.body`).
fn face(front: &str) -> TextStyle {
    match format::plain(front).chars().count() {
        0..=24 => TextStyle::new(28.0, 500.0).lh(34.0).tracking(-0.3),
        25..=60 => TextStyle::new(20.0, 400.0).lh(25.0),
        _ => TextStyle::new(17.0, 400.0).lh(22.0),
    }
}

/// Roughly how tall `markdown` sets at `width` in `font`, to tell whether a clamped front needs its fade.
fn estimated_height(markdown: &str, width: f32, font: TextStyle) -> f32 {
    let per_line = (width / (font.size * 0.5)).max(8.0);
    let paragraphs: Vec<Vec<usize>> = markdown
        .split("\n\n")
        .map(|p| p.lines().map(|l| format::plain(l).chars().count()).filter(|&n| n > 0).collect::<Vec<_>>())
        .filter(|p| !p.is_empty())
        .collect();
    let lines: f32 = paragraphs.iter().flatten().map(|&n| (n as f32 / per_line).ceil()).sum();
    lines * font.line_px() + paragraphs.len().saturating_sub(1) as f32 * font.line_px() * 0.6
}

fn markdown(src: &str, font: TextStyle, k: &Kit) -> El {
    Markdown::new(src.to_owned()).render(&MarkdownStyle::from_kit(k).body_font(font).strong_weight(600.0))
}

/// Small capitals for a footer: "+ 1 HIDDEN SIDE", "DUE", "2 DAYS".
fn caps(label: &str, ink: rune::Color) -> El {
    text(label.to_uppercase()).font(theme::CAPS).color(ink)
}

/// When it's due and, on a full tile, how many sides it hides.
fn footer(c: &CardView, k: &Kit, full: bool) -> El {
    let (when, is_due) = due(c, now_ms());
    let faint = theme::tertiary(k);
    let mut footer = row().key("footer").w_full().gap(8.0).center_y();
    let more = c.sides.len().saturating_sub(1);
    if full && more > 0 {
        footer = footer.child(
            row()
                .key("hidden")
                .gap(3.0)
                .center_y()
                .child(icon(&icons::PLUS, 10.0).color(faint))
                .child(caps(&format::hidden_sides(more), faint)),
        );
    }
    footer
        .child(el().key("spacer").grow(1.0))
        .child(caps(&when, if is_due { theme::accent(k) } else { faint }).key("when"))
}

/// A tile's footer under the side rule, which runs out through the tile's padding `pad` to its edges.
fn tile_footer(c: &CardView, k: &Kit, pad: f32) -> El {
    col()
        .key("foot")
        .w_full()
        .gap(8.0)
        .child(col().key("rule-run").mx(-pad).child(theme::side_line(k, "rule")))
        .child(footer(c, k, true))
}

/// The ring around the selected tile, outside its box so nothing moves.
fn ring(e: El, k: &Kit, on: bool, strong: bool) -> El {
    if !on {
        return e;
    }
    e.outline(2.0, 1.0, if strong { theme::accent(k) } else { k.c.border_strong })
}

/// A card's tile in the grid or the list.
fn tile(s: &App, c: &CardView, k: &Kit, width: f32, list: bool) -> El {
    let (when, _) = due(c, now_ms());
    let selected = s.selected.as_deref() == Some(c.card.id.as_str());
    let strong = s.window_focused;
    let version = (&c.card.updated_at, c.schedule.reps, &when, selected, strong, list, width as i32);
    memo(format!("tile-{}", c.card.id), version, || {
        let front = c.sides.first().map_or("", String::as_str);
        let font = face(front);
        let pad = if list { 18.0 } else { 14.0 };
        let max = if list { LIST_FACE_MAX } else { GRID_FACE_MAX };
        let mut body =
            col().key("face").w_full().grow(1.0).min_w(0.0).max_h(max).child(markdown(front, font, k).key("front"));
        if estimated_height(front, width - 2.0 * pad, font) > max {
            body = body.clip_fade(0.0, 0.0, 0.0, 28.0);
        }
        let id = c.card.id.clone();
        let tile = col()
            .key(format!("tile-{}", c.card.id))
            .role("button")
            .label(format::title(front))
            .w_full()
            .min_w(0.0)
            .min_h(if list { 0.0 } else { 156.0 })
            .padding(pad)
            .gap(10.0)
            .child(body)
            .child(tile_footer(c, k, pad));
        let tile = theme::card(tile, k, theme::TILE_RADIUS, false);
        k.interactive(ring(tile, k, selected, strong)).on_press(move |s: &mut App| s.press_card(&id))
    })
}

/// The dashed "New card" tile that starts the grid, the list and the column.
fn new_tile(k: &Kit, key: &str, height: f32) -> El {
    k.interactive(
        row()
            .key(key.to_owned())
            .role("button")
            .label("New card")
            .help("New card  N")
            .w_full()
            .min_w(0.0)
            .h_full()
            .min_h(height)
            .center()
            .gap(8.0)
            .radius(theme::TILE_RADIUS)
            .border(1.0, k.c.border_base)
            .border_dash(5.0, 4.0)
            .child(icon(&icons::PLUS, 14.0).key("icon").color(k.c.icon_muted))
            .child(text("New card").key("label").font(k.ty.ui_medium).color(k.c.text_muted))
            .child(Keybind::new(&["N"]).variant(KeybindVariant::Neutral).build(k).key("key")),
    )
    .on_press(|s: &mut App| s.new_card())
}

fn scroller(key: &str, rows: El) -> El {
    col()
        .key(key.to_owned())
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(16.0)
        .scroll_margin(12.0)
        .child(rows)
}

/// The key of the grid row holding cell `r`, where cell 0 is the "New card" tile.
pub fn grid_row_key(cols: usize, row: usize) -> String {
    format!("grid-{cols}-{row}")
}

fn list_row_key(id: &str) -> String {
    format!("list-{id}")
}

fn column_row_key(id: &str) -> String {
    format!("column-{id}")
}

/// The row to scroll to so the card `id` shows.
pub fn row_key(s: &App, id: &str) -> String {
    match s.view {
        CardsView::Grid => {
            let i = s.cards.iter().position(|c| c.card.id == id).unwrap_or(0);
            grid_row_key(s.grid_columns(), (i + 1) / s.grid_columns())
        }
        CardsView::List => list_row_key(id),
        CardsView::Column => column_row_key(id),
    }
}

pub fn grid(s: &App, k: &Kit) -> El {
    let cols = s.grid_columns();
    let inner = s.cards_width - 2.0 * RING_ROOM;
    let tile_w = if inner > 0.0 { (inner - GRID_GAP * (cols - 1) as f32) / cols as f32 } else { 220.0 };
    let rows = (s.cards.len() + 1).div_ceil(cols);
    let k = k.clone();
    let list = col().key("grid").w_full().px(RING_ROOM).pt(RING_ROOM).growth(GrowthSpec::default()).virtual_rows(
        rows,
        196.0,
        move |_: &App, r| Cow::Owned(grid_row_key(cols, r)),
        move |s: &App, r| {
            let mut line = row().w_full().equal_columns(cols).gap(GRID_GAP).pb(GRID_GAP);
            for i in 0..cols {
                let cell = r * cols + i;
                line = line.child(match cell.checked_sub(1) {
                    None => new_tile(&k, "new-card-tile", 156.0),
                    Some(n) => match s.cards.get(n) {
                        Some(c) => tile(s, c, &k, tile_w, false),
                        None => el().key(format!("empty-{i}")).min_w(0.0),
                    },
                });
            }
            line
        },
    );
    scroller("grid-scroll", list)
}

pub fn list(s: &App, k: &Kit) -> El {
    let width = s.cards_width - 2.0 * RING_ROOM;
    let k = k.clone();
    let rows = col().key("list").w_full().px(RING_ROOM).pt(RING_ROOM).growth(GrowthSpec::default()).virtual_rows(
        s.cards.len() + 1,
        150.0,
        |s: &App, i| match i.checked_sub(1) {
            None => Cow::Borrowed("list-new"),
            Some(n) => Cow::Owned(list_row_key(&s.cards[n].card.id)),
        },
        move |s: &App, i| {
            let content = match i.checked_sub(1) {
                None => new_tile(&k, "new-card-bar", 52.0),
                Some(n) => tile(s, &s.cards[n], &k, width, true),
            };
            col().w_full().pb(GRID_GAP).child(content)
        },
    );
    scroller("list-scroll", rows)
}

/// A card in the column view's list: a line or two of its front and when it's due.
fn compact(s: &App, c: &CardView, k: &Kit) -> El {
    let (when, _) = due(c, now_ms());
    let selected = s.current().is_some_and(|cur| cur.card.id == c.card.id);
    let strong = !s.detail_focused && s.window_focused;
    let version = (&c.card.updated_at, c.schedule.reps, &when, selected, strong);
    memo(format!("compact-{}", c.card.id), version, || {
        let front = c.sides.first().map_or("", String::as_str);
        let id = c.card.id.clone();
        let tile = col()
            .key(format!("compact-{}", c.card.id))
            .role("button")
            .label(format::title(front))
            .w_full()
            .px(14.0)
            .py(12.0)
            .gap(8.0)
            .radius(k.radius.lg)
            .fill(if selected { k.c.layer_02 } else { k.c.layer_01 })
            .border(1.0, k.c.border_muted)
            .child(
                text(format::plain(front))
                    .key("front")
                    .font(TextStyle::new(14.0, 500.0).lh(20.0))
                    .color(k.c.text_base)
                    .max_lines(2),
            )
            .child(footer(c, k, false));
        k.interactive(ring(tile, k, selected, strong)).on_press(move |s: &mut App| s.press_card(&id))
    })
}

/// The column view: the search, "New card" and compact tiles on the left; every side of the current card on the right.
pub fn column(s: &App, k: &Kit, search: El, empty: Option<El>) -> El {
    let left_w = (s.cards_width / 3.0).clamp(232.0, 340.0);
    let kk = k.clone();
    let rows =
        col().key("column-list").w_full().px(RING_ROOM).pt(RING_ROOM).growth(GrowthSpec::default()).virtual_rows(
            s.cards.len() + 1,
            84.0,
            |s: &App, i| match i.checked_sub(1) {
                None => Cow::Borrowed("column-new"),
                Some(n) => Cow::Owned(column_row_key(&s.cards[n].card.id)),
            },
            move |s: &App, i| {
                let content = match i.checked_sub(1) {
                    None => new_tile(&kk, "new-card-compact", 44.0),
                    Some(n) => compact(s, &s.cards[n], &kk),
                };
                col().w_full().pb(10.0).child(content)
            },
        );
    let mut left = col()
        .key("column-left")
        .w(left_w)
        .h_full()
        .min_h(0.0)
        .gap(6.0)
        .child(col().key("column-search").w_full().px(RING_ROOM).child(search));
    left = match empty {
        Some(e) => left.child(e),
        None => left.child(scroller("column-scroll", rows)),
    };
    row().key("columns").w_full().grow(1.0).min_h(0.0).gap(20.0).items_stretch().child(left).child(detail(s, k))
}

/// Every side of the current card, then how its reviews have gone and its tags (the iOS app's `CardSides`).
fn detail(s: &App, k: &Kit) -> El {
    let Some(c) = s.current() else {
        return col().key("detail-none").grow(1.0).h_full().min_w(0.0);
    };
    let now = now_ms();
    let id = c.card.id.clone();
    let mut edit = Some(
        IconButton::new(&icons::PENCIL)
            .variant(IconButtonVariant::GhostMuted)
            .control_size(ControlSize::Large)
            .label("Edit card")
            .help("Edit card")
            .shortcut("Return")
            .on_press(move |s: &mut App| s.edit_card(&id))
            .build(k)
            .key("edit"),
    );
    let mut sheet = col().key("sides").w_full();
    for (i, side) in c.sides.iter().enumerate() {
        if i > 0 {
            sheet = sheet.child(theme::side_rule(k, &format!("rule-{i}")));
        }
        let side = markdown(side, theme::SIDE, k).key(format!("side-{i}"));
        sheet = sheet.child(match i {
            // The edit button sits beside the front, out of the way of the sides below.
            0 => row()
                .key("front")
                .w_full()
                .px(theme::SIDE_X)
                .gap(12.0)
                .items_start()
                .child(col().key("front-text").grow(1.0).min_w(0.0).child(side))
                .child(el().key("edit-room").mt(-6.0).mr(-10.0).children(edit.take())),
            _ => col().key(format!("side-box-{i}")).w_full().px(theme::SIDE_X).child(side),
        });
    }

    // One line: its reviews, lapses and when it's due, the due part in the accent when it's now.
    let s_ = &c.schedule;
    let (history, when, is_due) = match s_.state {
        State::New => (String::new(), "new, not reviewed yet".to_string(), false),
        _ => {
            let parts: Vec<String> = [
                (s_.reps > 0).then(|| format!("✓ {}", format::reviews(s_.reps))),
                (s_.lapses > 0).then(|| format::lapses(s_.lapses)),
            ]
            .into_iter()
            .flatten()
            .collect();
            let history = if parts.is_empty() { String::new() } else { parts.join(" · ") + " · " };
            if s_.due <= now {
                (history, "due now".to_string(), true)
            } else {
                (history, format!("due {}", format::until(s_.due, now)), false)
            }
        }
    };
    let ink = theme::secondary(k);
    let mut line = row().key("meta-line").w_full().center_y().flex_wrap();
    if !history.is_empty() {
        line = line.child(text(history.to_uppercase()).key("history").font(theme::META).color(ink));
    }
    line = line.child(text(when.to_uppercase()).key("due").font(theme::META).color(if is_due {
        theme::accent(k)
    } else {
        ink
    }));
    let mut meta = col().key("meta").w_full().px(theme::SIDE_X).pt(24.0).gap(10.0).child(line);
    if !c.card.tags.is_empty() {
        meta = meta.child(
            c.card.tags.iter().fold(row().key("tags").w_full().gap(6.0).flex_wrap(), |r, t| r.child(theme::tag(k, t))),
        );
    }

    let surface = col().key(format!("detail-{}", c.card.id)).w_full().py(theme::SIDE_Y).child(sheet).child(meta);
    let surface = theme::card(surface, k, theme::CARD_RADIUS, false);
    let surface = ring(surface, k, s.detail_focused, s.window_focused);
    col()
        .key("detail")
        .grow(1.0)
        .h_full()
        .min_w(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(16.0)
        .child(col().key("detail-pad").w_full().padding(RING_ROOM).pb(24.0).child(surface))
}
