//! The card editor: Markdown on the left, its sides previewed on the right, tags and deck underneath.

use rune::{El, Key, KeyEvent, ScrollSpec, TextStyle, Tx, col, row, text};
use rune_kit::shell::Toolbar;
use rune_kit::{Button, ButtonVariant, Kit, Markdown, MarkdownStyle, Select, TextArea, TextInput};

use super::{error_line, toolbar};
use crate::app::{App, Editor};
use crate::core::sides;

const PREVIEW_FACE: TextStyle = TextStyle::new(15.0, 400.0).lh(23.0);

/// ⌘Return saves and Escape cancels, from any field in the editor.
pub fn key(s: &mut App, ev: &KeyEvent, _: &mut Tx) -> bool {
    match ev.key {
        Key::Enter if ev.mods.cmd => s.save_editor(),
        Key::Escape => s.cancel_editor(),
        _ => return false,
    }
    true
}

fn editing(s: &mut App) -> &mut Editor {
    s.editor.as_mut().expect("editing a card")
}

pub fn view(s: &App, e: &Editor, k: &Kit, layer: &mut Vec<El>) -> (El, Toolbar) {
    let title = if e.card.is_some() { "Edit card" } else { "New card" };
    let cancel = Button::new("Cancel")
        .variant(ButtonVariant::Ghost)
        .shortcut("Escape")
        .on_press(|s: &mut App| s.cancel_editor())
        .build(k)
        .key("cancel");
    let save = Button::new("Save card")
        .variant(ButtonVariant::Contrast)
        .help("Save card")
        .shortcut("cmd+return")
        .on_press(|s: &mut App| s.save_editor())
        .build(k)
        .key("save");
    let bar = toolbar(title).trailing(row().key("editor-tools").gap(6.0).center_y().child(cancel).child(save));

    let content = TextArea::new(&e.content)
        .placeholder("What is the capital of France?\n---\nParis")
        .width(None)
        .rows(12)
        .max_height(520.0)
        .on_edit(|s: &mut App| &mut editing(s).content)
        .on_key(key)
        .build(k)
        .key("content");
    let hint =
        text("A line with only --- starts the next side.").key("hint").font(k.ty.caption()).color(k.c.text_faint);

    let tags = TextInput::new(&e.tags)
        .placeholder("Tags, separated by commas")
        .width(None)
        .on_edit(|s: &mut App| &mut editing(s).tags)
        .on_key(key)
        .build(k)
        .key("tags");
    let names: Vec<&str> = s.decks.iter().map(|d| d.deck.name.as_str()).collect();
    let deck = Select::new("deck", &names)
        .current(Some(s.deck_name(&e.deck)))
        .open(e.picking_deck)
        .on_toggle(|s: &mut App| editing(s).picking_deck ^= true)
        .on_close(|s: &mut App| editing(s).picking_deck = false)
        .on_choose(|s: &mut App, name: &str, _: &mut Tx| {
            let id = s.decks.iter().find(|d| d.deck.name == name).map(|d| d.deck.id.clone());
            let e = editing(s);
            e.picking_deck = false;
            if let Some(id) = id {
                e.deck = id;
            }
        })
        .build(k, layer)
        .key("deck-picker");
    let fields = row()
        .key("fields")
        .w_full()
        .gap(12.0)
        .center_y()
        .child(col().key("tags-field").grow(1.0).min_w(0.0).child(tags))
        .child(deck);
    let mut left = col().key("write").grow(1.0).min_w(0.0).gap(10.0).child(content).child(hint).child(fields);
    if let Some(err) = &e.error {
        left = left.child(error_line(k, err));
    }

    let mut page = col().key("editor").w_full().h_full().min_h(0.0).px(24.0).pt(8.0).pb(24.0).gap(16.0).child(
        row().key("panes").w_full().grow(1.0).min_h(0.0).gap(20.0).items_start().child(left).child(preview(e, k)),
    );
    if e.card.is_some() {
        let label = if e.confirm_delete { "Delete this card" } else { "Delete card" };
        page = page.child(
            row().key("danger").child(
                Button::new(label)
                    .variant(if e.confirm_delete { ButtonVariant::Danger } else { ButtonVariant::GhostMuted })
                    .rolling("delete-label")
                    .on_press(|s: &mut App| s.delete_card())
                    .build(k)
                    .key("delete"),
            ),
        );
    }
    (page, bar)
}

fn preview(e: &Editor, k: &Kit) -> El {
    let style = MarkdownStyle::from_kit(k).body_font(PREVIEW_FACE);
    let parts = sides(&e.content.text);
    let mut list = col().key("sides").w_full().gap(12.0);
    if parts.iter().all(|p| p.is_empty()) {
        list = list.child(
            col()
                .key("side-empty")
                .w_full()
                .padding(20.0)
                .radius(k.radius.lg)
                .border(1.0, k.c.border_muted)
                .child(text("Each side of the card shows here as you write.").font(k.ty.ui).color(k.c.text_faint)),
        );
    }
    for (i, side) in parts.iter().enumerate().filter(|(_, p)| !p.is_empty()) {
        let label = match i {
            0 => "Front".to_string(),
            1 => "Back".to_string(),
            n => format!("Side {}", n + 1),
        };
        list = list.child(
            col()
                .key(format!("side-{i}"))
                .w_full()
                .padding(20.0)
                .gap(8.0)
                .radius(k.radius.lg)
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_muted)
                .child(text(label).key("label").font(k.ty.small_medium).color(k.c.text_faint))
                .child(Markdown::new(side.clone()).render(&style)),
        );
    }
    col()
        .key("preview")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(16.0)
        .child(list)
}
