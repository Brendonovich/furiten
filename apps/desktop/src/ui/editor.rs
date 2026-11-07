//! The card editor, a dialog over the window: the card as it reviews, one side under another with a dashed rule between, each its own field
//! with its Markdown styled in place. A card is just its sides, in order; nothing here names them. Return is a new line; a line with only `---` then Return starts the next
//! side. Tags and deck underneath.

use rune::{El, Key, KeyEvent, ScrollSpec, TextStyle, Tx, col, el, icon, row, text, text_input};
use rune_kit::{Button, ButtonVariant, ControlSize, Dialog, Kit, Placement, PopupItem, PopupList, Select, TextInput};

use super::cards::dashed_in;
use super::markup::{self, Palette};
use super::{error_line, icons};
use crate::app::{App, Editor};
use crate::editor::RECENT_TAGS;

/// Every side is set alike: what each one is for is the writer's to decide.
const FACE: TextStyle = TextStyle::new(17.0, 400.0).lh(26.0);
/// How wide the dialog gets, so lines stay readable; a narrow window narrows it.
const DIALOG_W: f32 = 620.0;
/// How tall the sides get before they scroll.
const SIDES_MAX_H: f32 = 400.0;
/// The sides' well is at least this tall, so a new card doesn't open as a sliver.
const SIDES_MIN_H: f32 = 260.0;

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

/// Keys in side `id`'s field before it edits: Backspace at its start joins it to the side before, and the arrows at
/// either end move into the next field.
fn side_key(s: &mut App, id: u32, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let plain = !(ev.mods.cmd || ev.mods.ctrl || ev.mods.alt || ev.mods.shift);
    let e = editing(s);
    let Some(edit) = e.side_mut(id) else { return false };
    let (at, collapsed, len) = (edit.focus, edit.anchor == edit.focus, edit.text.len());
    match ev.key {
        Key::Backspace if plain && collapsed && at == 0 => e.join_previous(id),
        // A bullet's `- ` is hidden behind the bullet drawn for it, so the caret steps over it whole. Backspace at the
        // item's start isn't special: it deletes the space, and `-item` is no bullet, so the dash shows again.
        Key::Left if plain && collapsed => match markup::bullet_marker(&edit.text, at) {
            Some(m) if at == m.end => {
                edit.set_caret(m.start);
                true
            }
            _ => key(s, ev, tx),
        },
        Key::Right if plain && collapsed => match markup::bullet_marker(&edit.text, at) {
            Some(m) if at == m.start => {
                edit.set_caret(m.end);
                true
            }
            _ => key(s, ev, tx),
        },
        Key::Up if plain && collapsed && at == 0 => match e.neighbour(id, -1) {
            Some(prev) => {
                let end = e.side_mut(prev).map_or(0, |p| p.text.len());
                e.focus(prev, end);
                true
            }
            None => false,
        },
        Key::Down if plain && collapsed && at == len => match e.neighbour(id, 1) {
            Some(next) => {
                e.focus(next, 0);
                true
            }
            None => false,
        },
        _ => key(s, ev, tx),
    }
}

fn side_field(e: &Editor, i: usize, k: &Kit) -> El {
    let side = &e.sides[i];
    let id = side.id;
    let placeholder = if e.sides.len() == 1 { "Write in Markdown" } else { "Empty side" };
    let shown = markup::shown_lines(&side.edit);
    // What the field draws: the side's text, with its bullet items laid out as bullets. Edits still go to the side.
    let mut drawn = side.edit.clone();
    drawn.set_blocks(markup::blocks(&side.edit.text));
    let field = text_input(&drawn, FACE)
        .key("input")
        .w_full()
        .color(k.c.text_base)
        .placeholder(placeholder, k.c.text_faint)
        .caret(k.c.text_base, k.selection())
        .spans(markup::spans(&side.edit.text, FACE.size, &Palette::from_kit(k), shown))
        .on_edit(move |s: &mut App| editing(s).side_mut(id).expect("a side on screen"))
        .on_input_key(move |s: &mut App, ev: &KeyEvent, tx: &mut Tx| side_key(s, id, ev, tx))
        .on_submit(move |s: &mut App, _: &mut Tx| editing(s).newline(id))
        .on_input_change(|s: &mut App, _: &mut Tx| editing(s).split());
    col().key(format!("side-{id}")).w_full().child(field)
}

/// Keys in the tag draft: ↓ and ↑ move through the tag menu, Return adds the highlighted tag (or else what's typed),
/// Escape closes the menu before it cancels the editor, and Backspace when the draft is empty takes the last chip off.
fn tag_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let plain = !(ev.mods.cmd || ev.mods.ctrl || ev.mods.alt || ev.mods.shift);
    let e = editing(s);
    let moved = match ev.key {
        Key::Down if plain => e.move_tag_active(1),
        Key::Up if plain => e.move_tag_active(-1),
        _ => false,
    };
    if moved {
        if let Some(tag) = e.tag_active.and_then(|i| e.tag_suggestions().get(i).map(|t| t.to_string())) {
            tx.scroll_into_view(format!("{TAG_MENU}-option-{tag}"));
        }
        return true;
    }
    match ev.key {
        Key::Enter if plain => e.enter_tag() || key(s, ev, tx),
        Key::Backspace if plain => e.pop_tag(),
        Key::Escape => e.close_tag_menu() || key(s, ev, tx),
        _ => key(s, ev, tx),
    }
}

const TAG_MENU: &str = "tag-menu";

/// Tags in use, under the tag field while it has the caret: the recently used ones, or those matching what's typed.
fn tag_menu(e: &Editor, k: &Kit, layer: &mut Vec<El>) {
    let suggestions = e.tag_suggestions();
    let active = e.tag_active.filter(|&i| i < suggestions.len());
    let items: Vec<PopupItem> = suggestions.iter().map(|t| PopupItem::new(*t, *t)).collect();
    PopupList::new(TAG_MENU, "tags")
        .placement(Placement::BottomStart)
        .width(240.0)
        .max_height(28.0 * RECENT_TAGS as f32)
        .label("Tags")
        .items(items)
        .active(active)
        .on_active(|s: &mut App, i| editing(s).tag_active = Some(i))
        .on_choose(|s: &mut App, tag: &str, _: &mut Tx| editing(s).choose_tag(tag))
        .build(k, layer);
}

fn tag_chip(k: &Kit, tag: &str) -> El {
    let t = tag.to_owned();
    let remove = el()
        .key(format!("remove-{tag}"))
        .role("button")
        .label(format!("Remove {tag}"))
        .size(16.0, 16.0)
        .center()
        .radius(k.radius.sm)
        .hover(k.c.surface_hover)
        .child(icon(k.glyph.close, 12.0).color(k.c.text_muted))
        .on_press(move |s: &mut App| editing(s).remove_tag(&t));
    row()
        .key(format!("tag-{tag}"))
        .gap(2.0)
        .center_y()
        .h(24.0)
        // About 7 points of space around what's drawn on every side: the × glyph is smaller than its press area.
        .pl(7.0)
        .pr(2.0)
        .radius(k.radius.md)
        .fill(k.c.layer_03)
        .child(text(tag.to_owned()).font(k.ty.ui).color(k.c.text_base))
        .child(remove)
}

/// The card's tags as chips in one field, with the next one typed after them.
fn tags_field(e: &Editor, k: &Kit) -> El {
    let placeholder = if e.tags.is_empty() { "Add tags" } else { "Add tag" };
    let draft = TextInput::new(&e.tag_draft)
        .plain()
        .width(None)
        .placeholder(placeholder)
        .on_edit(|s: &mut App| &mut editing(s).tag_draft)
        .on_change(|s: &mut App, _: &mut Tx| editing(s).take_typed_tags())
        .on_key(tag_key)
        .build(k)
        .key("tag-draft");
    let border = if e.tag_draft.focused { k.c.border_focus } else { k.c.border_base };
    let mut field = row()
        .key("tags")
        .grow(1.0)
        .min_w(0.0)
        .min_h(36.0)
        .padding(5.0)
        .gap(6.0)
        .flex_wrap()
        .center_y()
        .radius(k.radius.md)
        .fill(k.c.surface_base)
        .border(1.0, border);
    for tag in &e.tags {
        field = field.child(tag_chip(k, tag));
    }
    field.child(col().key("tag-draft-box").grow(1.0).min_w(90.0).child(draft))
}

/// The sides in one box, a dashed rule between them and Add side as its footer.
fn sides_well(e: &Editor, k: &Kit) -> El {
    let mut sides = col().key("sides").w_full().gap(16.0);
    for i in 0..e.sides.len() {
        if i > 0 {
            // Stronger than a card's rules: here it's where one side's field ends and the next begins.
            sides = sides.child(dashed_in(k.c.text_faint, &format!("rule-{}", e.sides[i].id)));
        }
        sides = sides.child(side_field(e, i, k));
    }
    // The sides sit in a well of their own, so it's plain where to write. A long card scrolls inside it rather than
    // growing the dialog past the window; Add side is the well's footer.
    let scroll = col()
        .key("sides-scroll")
        .w_full()
        .min_h(SIDES_MIN_H)
        .max_h(SIDES_MAX_H)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(12.0)
        .child(col().key("sides-pad").w_full().padding(20.0).child(sides));
    let add = row().key("add-side").w_full().px(8.0).py(6.0).child(
        Button::new("Add side")
            .icon(&icons::PLUS)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .on_press(|s: &mut App| editing(s).add_side())
            .build(k)
            .key("add"),
    );
    col()
        .key("sides-well")
        .w_full()
        .radius(k.radius.lg)
        .clip_rounded(k.radius.lg)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .child(scroll)
        .child(el().key("sides-rule").w_full().h(1.0).fill(k.c.border_base))
        .child(add)
}

/// Experiment ([`App::separate_sides`]): each side in a box of its own, the one being written outlined, and Add side
/// under them. Pressing anywhere in a box writes at the end of that side.
fn separate_sides(e: &Editor, k: &Kit) -> El {
    let mut boxes = col().key("sides").w_full().min_h(SIDES_MIN_H).gap(10.0);
    let last = e.sides.len() - 1;
    for (i, side) in e.sides.iter().enumerate() {
        let (id, len) = (side.id, side.edit.text.len());
        let border = if side.edit.focused { k.c.border_focus } else { k.c.border_base };
        let mut b = col()
            .key(format!("side-box-{id}"))
            .w_full()
            .padding(16.0)
            .radius(k.radius.lg)
            .fill(k.c.surface_base)
            .border(1.0, border)
            .child(side_field(e, i, k))
            .on_press(move |s: &mut App| editing(s).focus(id, len));
        if i == last {
            // The last box takes the room left, so a short card still opens with space to write.
            b = b.grow(1.0);
        }
        boxes = boxes.child(b);
    }
    let scroll = col()
        .key("sides-scroll")
        .w_full()
        .max_h(SIDES_MAX_H)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(12.0)
        .child(boxes);
    let add = row().key("add-side").w_full().child(
        Button::new("Add side")
            .icon(&icons::PLUS)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .on_press(|s: &mut App| editing(s).add_side())
            .build(k)
            .key("add"),
    );
    col().key("sides-separate").w_full().gap(8.0).child(scroll).child(add)
}

/// The editor's dialog, over whatever the window showed when it opened.
pub fn dialog(s: &App, e: &Editor, k: &Kit, layer: &mut Vec<El>) {
    let title = if e.card.is_some() { "Edit card" } else { "New card" };

    let sides = if s.separate_sides() { separate_sides(e, k) } else { sides_well(e, k) };

    let names: Vec<&str> = s.decks.iter().map(|d| d.deck.name.as_str()).collect();
    // Its list goes in a layer of its own, added after the dialog: an anchored overlay only finds an anchor (the
    // trigger, inside the dialog's panel) that comes before it.
    let mut picker = Vec::new();
    let deck = Select::new("deck", &names)
        .icon(&icons::DECK)
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
                s.load_editor_tags();
            }
        })
        // Keeps the key `Select` gave it (`deck-trigger`): its list is placed against that.
        .build(k, &mut picker);
    let fields = row().key("fields").w_full().gap(12.0).center_y().child(tags_field(e, k)).child(deck);

    let mut footer = row().key("footer").w_full().gap(6.0).center_y().pt(4.0);
    if e.card.is_some() {
        let label = if e.confirm_delete { "Delete this card" } else { "Delete card" };
        footer = footer.child(
            Button::new(label)
                .variant(if e.confirm_delete { ButtonVariant::Danger } else { ButtonVariant::GhostMuted })
                .rolling("delete-label")
                .on_press(|s: &mut App| s.delete_card())
                .build(k)
                .key("delete"),
        );
    }
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
    footer = footer.child(el().key("footer-spacer").grow(1.0)).child(cancel).child(save);

    let mut body = col().key("write").w_full().px(16.0).pb(16.0).gap(12.0).child(sides).child(fields);
    if let Some(err) = &e.error {
        body = body.child(error_line(k, err));
    }
    body = body.child(footer);

    Dialog::new("editor", title)
        .open(true)
        .width(DIALOG_W)
        .fit()
        // A click outside is Cancel.
        .click_outside(true)
        .on_close(|s: &mut App| s.cancel_editor())
        .child(body)
        .build(k, layer);
    layer.extend(picker);
    // Like the deck list, after the dialog: it anchors to the tag field inside it.
    if e.tag_menu_open() {
        tag_menu(e, k, layer);
    }
}
