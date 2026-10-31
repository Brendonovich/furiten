//! Sync sign-in and the core's status. Reached from the sidebar after the first review, never before.

use rune::{El, col, row, text};
use rune_kit::shell::Toolbar;
use rune_kit::{Button, ButtonVariant, Kit, TextInput};

use super::{error_line, format, toolbar};
use crate::app::{App, Settings};
use crate::core::now_ms;

pub fn view(s: &App, settings: &Settings, k: &Kit) -> (El, Toolbar) {
    let st = &s.status;
    let mut sync = col()
        .key("sync")
        .w_full()
        .gap(10.0)
        .child(text("Sync").key("title").font(k.ty.body_medium).color(k.c.text_base));
    if st.signed_in {
        let server = st.server.clone().unwrap_or_default();
        let last = st
            .last_sync
            .map_or("Not synced yet".to_string(), |at| format!("Last synced {}", format::ago(at, now_ms())));
        sync = sync
            .child(text(format!("Signed in to {server}")).key("server").font(k.ty.ui).color(k.c.text_muted))
            .child(text(last).key("last").font(k.ty.ui).color(k.c.text_faint))
            .child(
                row().key("actions").pt(4.0).child(
                    Button::new("Sync now")
                        .variant(ButtonVariant::Neutral)
                        .on_press(|s: &mut App| s.sync_now())
                        .build(k)
                        .key("sync-now"),
                ),
            );
        if let Some(e) = &st.sync_error {
            sync = sync.child(error_line(k, &format!("The last sync failed: {e}")));
        }
    } else {
        sync = sync
            .child(
                text("Keep your decks on all your devices. Recall works fully offline without it.")
                    .key("lede")
                    .font(k.ty.ui)
                    .color(k.c.text_muted)
                    .wrap(),
            )
            .child(
                row()
                    .key("sign-in")
                    .w_full()
                    .gap(8.0)
                    .center_y()
                    .child(
                        col().key("server-field").grow(1.0).min_w(0.0).child(
                            TextInput::new(&settings.server)
                                .placeholder("https://recall.example.com")
                                .width(None)
                                .on_edit(|s: &mut App| &mut s.settings.as_mut().expect("settings").server)
                                .on_submit(|s: &mut App, _| s.sign_in())
                                .build(k)
                                .key("server"),
                        ),
                    )
                    .child(
                        Button::new("Sign in")
                            .variant(ButtonVariant::Contrast)
                            .on_press(|s: &mut App| s.sign_in())
                            .build(k)
                            .key("sign-in-button"),
                    ),
            );
    }
    if let Some(e) = &settings.error {
        sync = sync.child(error_line(k, e));
    }

    let about = format!("Recall core {} · {}", st.version, st.home);
    let mut page = col().key("settings").w_full().h_full().px(24.0).pt(8.0).gap(28.0).child(
        col().key("column").w_full().max_w(520.0).gap(28.0).child(sync).child(
            col()
                .key("about")
                .gap(4.0)
                .child(text("About").key("title").font(k.ty.body_medium).color(k.c.text_base))
                .child(text(about).key("core").font(k.ty.caption()).color(k.c.text_faint).truncate_middle()),
        ),
    );
    if let Some(why) = &s.offline {
        page = page.child(super::deck::offline_note(k, why));
    } else if s.core.is_fixture() {
        page = page.child(
            text("Running on sample data: changes last until the app quits.")
                .key("fixtures")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    }
    (page, toolbar("Settings"))
}
