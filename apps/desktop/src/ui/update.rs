//! The update panel (App › Check for Updates…): what the release service's channel holds for this install, live
//! from the updater's worker. A background check that stages a release opens it once by itself.

use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize, Dialog, Kit};

use crate::app::App;
use crate::updater::Status;

const PANEL_W: f32 = 420.0;

pub fn dialog(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let name = crate::identity::name();
    let running = crate::updater::VERSION;
    let mut body = col().key("body").w_full().gap(16.0).px(20.0).pb(20.0);
    let mut actions: Option<El> = None;
    match s.updater.as_ref().map(|u| u.status()) {
        None | Some(Status::Off) => {
            body = body.child(
                text("This copy of the app doesn't update itself. New releases are on GitHub.")
                    .key("off")
                    .font(k.ty.ui)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        }
        Some(Status::Checking) => {
            body = body.child(text("Checking for updates…").key("checking").font(k.ty.ui).color(k.c.text_muted));
        }
        Some(Status::Current) => {
            body = body.child(
                text(format!("{name} {running} is up to date."))
                    .key("current")
                    .font(k.ty.ui)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        }
        Some(Status::Failed) => {
            body =
                body.child(text("Couldn't check for updates.").key("failed").font(k.ty.ui).color(k.c.danger.fg).wrap());
            body = body.child(
                text("Try again from the menu later.").key("failed-later").font(k.ty.ui).color(k.c.text_muted).wrap(),
            );
        }
        Some(Status::Downloading) => {
            body = body.child(text("Downloading the update…").key("downloading").font(k.ty.ui).color(k.c.text_muted));
        }
        Some(Status::Ready { version }) => {
            body = body.child(
                text(format!("{name} {version} is ready to install."))
                    .key("ready")
                    .font(k.ty.ui)
                    .color(k.c.text_base)
                    .wrap(),
            );
            actions = Some(
                row()
                    .key("actions")
                    .pt(4.0)
                    .gap(8.0)
                    .child(
                        Button::new("Restart to Update")
                            .variant(ButtonVariant::Contrast)
                            .control_size(ControlSize::Large)
                            .on_press(|s: &mut App| s.restart_into_update())
                            .build(k)
                            .key("restart"),
                    )
                    .child(
                        Button::new("Later")
                            .variant(ButtonVariant::Neutral)
                            .control_size(ControlSize::Large)
                            .on_press(|s: &mut App| s.update = false)
                            .build(k)
                            .key("later"),
                    ),
            );
        }
        Some(Status::Available { version, url }) => {
            body = body.child(
                text(format!("{name} {version} is available."))
                    .key("available")
                    .font(k.ty.ui)
                    .color(k.c.text_base)
                    .wrap(),
            );
            let url = url.clone();
            actions = Some(
                row()
                    .key("actions")
                    .pt(4.0)
                    .gap(8.0)
                    .child(
                        Button::new("Download")
                            .variant(ButtonVariant::Contrast)
                            .control_size(ControlSize::Large)
                            .on_press_tx(move |_: &mut App, tx: &mut rune::Tx| {
                                let _ = tx.open_url(&url);
                            })
                            .build(k)
                            .key("download"),
                    )
                    .child(
                        Button::new("Later")
                            .variant(ButtonVariant::Neutral)
                            .control_size(ControlSize::Large)
                            .on_press(|s: &mut App| s.update = false)
                            .build(k)
                            .key("later"),
                    ),
            );
        }
    }
    if let Some(actions) = actions {
        body = body.child(actions);
    }
    Dialog::new("update", "Update")
        .open(true)
        .width(PANEL_W)
        .fit()
        .click_outside(true)
        .on_close(|s: &mut App| s.update = false)
        .child(body)
        .build(k, layer);
}
