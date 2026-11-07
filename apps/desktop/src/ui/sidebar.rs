use rune::{El, Tx, row, text};
use rune_kit::shell::{Sidebar, SidebarRow, SidebarSection};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Kit, Menu, MenuItem, Placement};

use super::icons;
use crate::app::App;
use crate::core::now_ms;
use crate::theme;

pub fn view(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    if s.connecting.is_some() {
        // Empty until the decks arrive, so nothing shows that's about to change.
        return Sidebar::new().label("Decks").build(k);
    }
    let mut decks = SidebarSection::new("Decks").key("decks").action(deck_menu(s, k, layer));
    for (d, depth) in s.deck_tree() {
        let id = d.deck.id.clone();
        decks = decks.row(
            SidebarRow::new(d.deck.name.clone())
                .icon(&icons::DECK)
                .depth(depth)
                .trailing(counts(k, d.due, d.new))
                .selected(s.deck.as_ref() == Some(&d.deck.id) && s.settings.is_none() && !s.in_inbox())
                .on_select(move |s: &mut App, _| s.select_deck(Some(id.clone())))
                .build(k)
                .key(format!("deck-{}", d.deck.id)),
        );
    }
    if let Some(edit) = &s.new_deck {
        decks = decks.row(
            SidebarRow::new("New deck")
                .icon(&icons::DECK)
                .renaming(
                    edit,
                    |s: &mut App| s.new_deck.as_mut().expect("adding a deck"),
                    |s: &mut App, _| s.commit_new_deck(),
                    |s: &mut App, _| s.new_deck = None,
                )
                .build(k)
                .key("deck-adding"),
        );
    }

    Sidebar::new().label("Decks").child(inbox_row(s, k)).section(decks).footer(sync_row(s, k)).build(k)
}

/// Review, above the decks: every due card from every deck, with how many are waiting.
fn inbox_row(s: &App, k: &Kit) -> El {
    let waiting = s.inbox_count();
    let mut count = row().key("counts").center_y();
    if waiting > 0 {
        count = count.child(text(waiting.to_string()).key("due").font(theme::COUNT).color(theme::accent(k)));
    }
    SidebarRow::new("Review")
        .icon(&icons::INBOX)
        .trailing(count)
        .selected(s.in_inbox())
        .on_select(|s: &mut App, _| s.open_inbox())
        .build(k)
        .key("inbox")
}

/// The "+" by Decks: a new deck, or decks from Mochi.
fn deck_menu(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    let add = IconButton::new(&icons::PLUS)
        .variant(IconButtonVariant::GhostMuted)
        .control_size(ControlSize::Small)
        .label("Add decks")
        .help("Add decks")
        .expanded(s.deck_menu)
        .on_press(|s: &mut App| s.deck_menu ^= true)
        .build(k)
        .key("deck-menu-button");
    Menu::new("deck-menu")
        .trigger(add)
        .placement(Placement::BottomEnd)
        .min_width(184.0)
        .open(s.deck_menu)
        .on_close(|s: &mut App| s.deck_menu = false)
        .item(MenuItem::new("New deck").icon(&icons::DECK).on_select(|s: &mut App| s.start_new_deck()))
        .item(
            MenuItem::new("Import from Mochi…")
                .icon(&icons::IMPORT)
                .on_select_tx(|s: &mut App, tx: &mut Tx| s.choose_mochi_export(tx)),
        )
        .build(k, layer)
        // The plus sits in the button's padding; this lines its edge up with the counts and the sidebar toggle.
        .mr(-4.0)
}

/// Due in the accent, new in faint, each only when there are some. Tabular, since they tick down as you review.
fn counts(k: &Kit, due: u64, new: u64) -> El {
    let mut r = row().key("counts").gap(6.0).center_y();
    if due > 0 {
        r = r.child(text(due.to_string()).key("due").font(theme::COUNT).color(theme::accent(k)));
    }
    if new > 0 {
        r = r.child(text(new.to_string()).key("new").font(theme::COUNT).color(k.c.text_faint));
    }
    r
}

fn sync_row(s: &App, k: &Kit) -> El {
    let st = &s.status;
    let detail = match (st.signed_in, st.last_sync, &st.sync_error) {
        (false, _, _) if st.hosting => "Your phone".to_string(),
        (false, _, _) => "Sign in".to_string(),
        (true, _, Some(_)) => "Sync failed".to_string(),
        (true, Some(at), None) => super::format::ago(at, now_ms()),
        (true, None, None) => "Not synced yet".to_string(),
    };
    let label = if st.signed_in || st.hosting { "Sync" } else { "Sync off" };
    SidebarRow::new(label)
        .icon(&icons::CLOUD)
        .detail(detail)
        .selected(s.settings.is_some())
        .on_select(|s: &mut App, _| s.open_settings())
        .build(k)
        .key("sync")
}
