//! The keyboard shortcuts panel (`?`): every key in `keys::SECTIONS`, by where it works.

use rune::{El, ScrollSpec, col, el, row, text};
use rune_kit::{Dialog, Keybind, KeybindVariant, Kit};

use crate::app::App;
use crate::keys::{self, SECTIONS};

const PANEL_W: f32 = 520.0;
const PANEL_MAX_H: f32 = 460.0;

pub fn dialog(k: &Kit, layer: &mut Vec<El>) {
    let mac = cfg!(target_os = "macos");
    let mut body = col().key("sections").w_full().gap(20.0).px(20.0).pb(20.0);
    for section in SECTIONS {
        let mut rows = col()
            .key(format!("section-{}", section.title))
            .w_full()
            .gap(8.0)
            .child(text(section.title).key("title").font(k.ty.small_medium).color(k.c.text_muted));
        for (i, (label, chords)) in keys::rows(section).into_iter().enumerate() {
            // One cap per chord, so alternatives (⌘\ or ⇧⌘B) read apart.
            let caps = chords.iter().enumerate().fold(row().key("keys").gap(6.0).center_y(), |r, (j, c)| {
                let cap = c.caps(mac).join(if mac { "" } else { "+" });
                r.child(Keybind::new(&[cap]).variant(KeybindVariant::Neutral).build(k).key(format!("chord-{j}")))
            });
            rows = rows.child(
                row()
                    .key(format!("row-{i}"))
                    .w_full()
                    .gap(12.0)
                    .center_y()
                    .child(text(label).key("label").font(k.ty.ui).color(k.c.text_base).wrap().grow(1.0).min_w(0.0))
                    .child(caps),
            );
        }
        body = body.child(rows);
    }
    let scroll = col()
        .key("shortcuts-scroll")
        .w_full()
        .max_h(PANEL_MAX_H)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(12.0)
        .child(body)
        .child(el().key("end").h(1.0));
    Dialog::new("shortcuts", "Keyboard shortcuts")
        .open(true)
        .width(PANEL_W)
        .fit()
        .click_outside(true)
        .on_close(|s: &mut App| s.shortcuts = false)
        .child(scroll)
        .build(k, layer);
}
