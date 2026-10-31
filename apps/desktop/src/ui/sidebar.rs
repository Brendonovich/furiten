use rune::{El, row, text};
use rune_kit::shell::{Sidebar, SidebarRow, SidebarSection};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Kit};

use super::icons;
use crate::app::App;
use crate::core::now_ms;
use crate::theme;

pub fn view(s: &App, k: &Kit) -> El {
    if s.connecting.is_some() {
        // Empty until the decks arrive, so nothing shows that's about to change.
        return Sidebar::new().inset(0.0).label("Decks").build(k);
    }
    let (due, new) = s.decks.iter().fold((0, 0), |(d, n), x| (d + x.due, n + x.new));
    let all = SidebarRow::new("All decks")
        .icon(&icons::STACK)
        .trailing(counts(k, due, new))
        .selected(s.deck.is_none() && s.settings.is_none())
        .on_select(|s: &mut App, _| s.select_deck(None))
        .build(k)
        .key("all-decks");

    let add = IconButton::new(&icons::PLUS)
        .variant(IconButtonVariant::GhostMuted)
        .control_size(ControlSize::Small)
        .label("New deck")
        .help("New deck")
        .on_press(|s: &mut App| s.start_new_deck())
        .build(k)
        .key("new-deck")
        // The plus sits in the button's padding; this lines its edge up with the counts and the sidebar toggle.
        .mr(-4.0);
    let mut decks = SidebarSection::new("Decks").key("decks").action(add);
    for d in &s.decks {
        let id = d.deck.id.clone();
        decks = decks.row(
            SidebarRow::new(d.deck.name.clone())
                .icon(&icons::DECK)
                .trailing(counts(k, d.due, d.new))
                .selected(s.deck.as_ref() == Some(&d.deck.id) && s.settings.is_none())
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

    Sidebar::new().inset(0.0).label("Decks").header(all).section(decks).footer(sync_row(s, k)).build(k)
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
        (false, _, _) => "Sign in".to_string(),
        (true, _, Some(_)) => "Sync failed".to_string(),
        (true, Some(at), None) => super::format::ago(at, now_ms()),
        (true, None, None) => "Not synced yet".to_string(),
    };
    let label = if st.signed_in { "Sync" } else { "Sync off" };
    SidebarRow::new(label)
        .icon(&icons::CLOUD)
        .detail(detail)
        .selected(s.settings.is_some())
        .on_select(|s: &mut App, _| s.open_settings())
        .build(k)
        .key("sync")
}
