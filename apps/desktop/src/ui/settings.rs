//! Sync sign-in and the core's status. Reached from the sidebar after the first review, never before.

use rune::{Color, El, col, el, row, text};
use rune_kit::shell::Toolbar;
use rune_kit::{Button, ButtonVariant, Kit, Switch, TextInput};

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
                text("Keep your decks on all your devices. Furiten works fully offline without it.")
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
                                .placeholder("https://furiten.example.com")
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

    let about = format!("Furiten core {} · {}", st.version, st.home);
    let mut page = col().key("settings").w_full().h_full().px(24.0).pt(8.0).gap(28.0).child(
        col()
            .key("column")
            .w_full()
            .max_w(520.0)
            .gap(28.0)
            .child(sync)
            .child(phones(s, settings, k))
            .child(web(settings, k))
            .child(review(s, k))
            .child(experiments(s, k))
            .child(
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

/// Hosting: this app is the library's hub, and paired phones keep their own copy and sync with it over Iroh.
fn phones(s: &App, settings: &Settings, k: &Kit) -> El {
    let h = &settings.host;
    let label = "Sync with your phone";
    let what = "Your phone keeps its own copy of your decks, so you can review and add cards anywhere, even offline. \
                It syncs with this computer whenever both are online, on any network.";
    let mut section = col()
        .key("phones")
        .w_full()
        .gap(10.0)
        .child(text("Your phone").key("title").font(k.ty.body_medium).color(k.c.text_base));
    let text_col = col()
        .key("text")
        .grow(1.0)
        .min_w(0.0)
        .gap(2.0)
        .child(text(label).key("label").font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(what).key("description").font(k.ty.ui).color(k.c.text_muted).wrap());
    let elsewhere = s.status.signed_in && !h.enabled;
    let mut head = row().key("host-row").w_full().gap(16.0).center_y().child(text_col);
    if !elsewhere {
        head = head.child(
            Switch::new(h.enabled)
                .label(label)
                .hide_label()
                .on_toggle(|s: &mut App| s.toggle_host())
                .build(k)
                .key("host-switch"),
        );
    }
    section = section.child(head);
    if elsewhere {
        let server = s.status.server.clone().unwrap_or_default();
        section = section.child(
            text(format!("This library syncs with {server}. Sign your phone in there instead."))
                .key("elsewhere")
                .font(k.ty.ui)
                .color(k.c.text_faint)
                .wrap(),
        );
        return section;
    }
    if let Some(e) = settings.host_error.as_ref().or(h.error.as_ref()) {
        section = section.child(error_line(k, e));
    }
    if let Some((ticket, _)) = &settings.ticket {
        let copy = ticket.ticket.clone();
        let details = col()
            .key("details")
            .grow(1.0)
            .min_w(0.0)
            .gap(8.0)
            .child(text("Scan this with Furiten on your phone").key("scan").font(k.ty.ui_medium).color(k.c.text_base))
            .child(
                text("In Furiten’s Settings on the phone, choose Pair with your computer. The code works once, for 10 minutes.")
                    .key("how")
                    .font(k.ty.ui)
                    .color(k.c.text_muted)
                    .wrap(),
            )
            .child(
                row()
                    .key("ticket-actions")
                    .gap(8.0)
                    .pt(4.0)
                    .child(
                        Button::new("Copy code")
                            .variant(ButtonVariant::Neutral)
                            .on_press_tx(move |_: &mut App, tx| tx.copy(copy.clone()))
                            .build(k)
                            .key("copy-ticket"),
                    )
                    .child(
                        Button::new("Done")
                            .variant(ButtonVariant::Neutral)
                            .on_press(|s: &mut App| s.hide_ticket())
                            .build(k)
                            .key("hide-ticket"),
                    ),
            );
        // A link, so the phone's Camera app opens Furiten to pair. The ticket in uppercase fits the QR code's
        // alphanumeric mode, a smaller code; tickets read case-insensitively.
        let link = format!("furiten://pair/{}", ticket.ticket.to_uppercase());
        section = section.child(row().key("ticket").w_full().gap(20.0).child(qr(&link)).child(details));
    } else {
        let title = if h.devices.is_empty() { "Pair a phone" } else { "Pair another phone" };
        section = section.child(
            row().key("pair").pt(4.0).child(
                Button::new(title)
                    .variant(ButtonVariant::Contrast)
                    .on_press(|s: &mut App| s.pair_phone())
                    .build(k)
                    .key("pair-phone"),
            ),
        );
    }
    let now = now_ms();
    for device in &h.devices {
        let state = match (h.running, device.connected, device.last_sync) {
            (true, true, _) => "Connected".to_string(),
            (_, _, Some(at)) => format!("Synced {}", format::ago(at, now)),
            _ => "Not connected".to_string(),
        };
        let id = device.device.id.clone();
        section = section.child(
            row()
                .key(format!("device-{}", device.device.id))
                .w_full()
                .gap(16.0)
                .center_y()
                .child(
                    col()
                        .key("text")
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(2.0)
                        .child(text(device.device.name.clone()).key("name").font(k.ty.ui_medium).color(k.c.text_base))
                        .child(text(state).key("state").font(k.ty.ui).color(k.c.text_faint)),
                )
                .child(
                    Button::new("Remove")
                        .variant(ButtonVariant::Neutral)
                        .on_press(move |s: &mut App| s.remove_device(&id))
                        .build(k)
                        .key("remove"),
                ),
        );
    }
    section
}

/// Web access: this app serves the library to a phone or browser on the network, signed in by link or QR code.
fn web(settings: &Settings, k: &Kit) -> El {
    let w = &settings.web;
    let label = "Use this library from your phone";
    let what = "Serves your decks to browsers on your network and Tailscale, without Furiten Cloud. Scan the code or \
                open the link on your phone.";
    let switch = Switch::new(w.enabled)
        .label(label)
        .hide_label()
        .on_toggle(|s: &mut App| s.toggle_web())
        .build(k)
        .key("web-switch");
    let text_col = col()
        .key("text")
        .grow(1.0)
        .min_w(0.0)
        .gap(2.0)
        .child(text(label).key("label").font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(what).key("description").font(k.ty.ui).color(k.c.text_muted).wrap());
    let mut section = col()
        .key("web")
        .w_full()
        .gap(10.0)
        .child(text("Web access").key("title").font(k.ty.body_medium).color(k.c.text_base))
        .child(row().key("web-row").w_full().gap(16.0).center_y().child(text_col).child(switch));
    if let Some(e) = settings.web_error.as_ref().or(w.error.as_ref()) {
        section = section.child(error_line(k, e));
    }
    if w.enabled
        && w.running
        && let (Some(link), Some(token)) = (&w.link, &w.token)
    {
        let link_copy = link.clone();
        let token_text = if settings.show_token { token.clone() } else { format!("rw_{}", "•".repeat(20)) };
        let mut details = col().key("details").grow(1.0).min_w(0.0).gap(8.0);
        for (i, url) in w.urls.iter().enumerate() {
            let color = if i == 0 { k.c.text_base } else { k.c.text_faint };
            details =
                details.child(text(url.clone()).key(format!("url-{i}")).font(k.ty.ui).color(color).selectable(true));
        }
        details = details
            .child(
                text(format!("Token {token_text}"))
                    .key("token")
                    .font(k.ty.mono)
                    .color(k.c.text_muted)
                    .truncate_middle(),
            )
            .child(
                row()
                    .key("web-actions")
                    .gap(8.0)
                    .pt(4.0)
                    .child(
                        Button::new("Copy link")
                            .variant(ButtonVariant::Neutral)
                            .on_press_tx(move |_: &mut App, tx| tx.copy(link_copy.clone()))
                            .build(k)
                            .key("copy-link"),
                    )
                    .child(
                        Button::new(if settings.show_token { "Hide token" } else { "Show token" })
                            .variant(ButtonVariant::Neutral)
                            .on_press(|s: &mut App| {
                                if let Some(settings) = s.settings.as_mut() {
                                    settings.show_token = !settings.show_token;
                                }
                            })
                            .build(k)
                            .key("show-token"),
                    ),
            );
        section = section.child(row().key("web-access").w_full().gap(20.0).child(qr(link)).child(details));
    }
    section
}

/// `data` as a QR code: dark runs of each row drawn as boxes on a white tile, with the standard quiet zone.
fn qr(data: &str) -> El {
    const MODULE: f32 = 4.0;
    let Ok(code) = qrcode::QrCode::new(data.as_bytes()) else { return el().key("qr") };
    let width = code.width();
    let colors = code.to_colors();
    let mut rows = col().key("rows");
    for y in 0..width {
        let mut line = row().key(format!("r{y}")).h(MODULE);
        let mut x = 0;
        while x < width {
            let dark = colors[y * width + x] == qrcode::Color::Dark;
            let run = (x..width).take_while(|&i| (colors[y * width + i] == qrcode::Color::Dark) == dark).count();
            let cell = el().key(format!("c{x}")).size(run as f32 * MODULE, MODULE);
            line = line.child(if dark { cell.fill(Color::BLACK) } else { cell });
            x += run;
        }
        rows = rows.child(line);
    }
    col().key("qr").padding(4.0 * MODULE).fill(Color::WHITE).radius(8.0).child(rows)
}

/// How the review lays cards out.
fn review(s: &App, k: &Kit) -> El {
    let label = "Full-width cards";
    let what = "Cards under review fill the window, instead of a fixed column down the middle.";
    let switch = Switch::new(s.full_width_cards())
        .label(label)
        .hide_label()
        .on_toggle(|s: &mut App| s.toggle_full_width_cards())
        .build(k)
        .key("full-width");
    let text_col = col()
        .key("text")
        .grow(1.0)
        .min_w(0.0)
        .gap(2.0)
        .child(text(label).key("label").font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(what).key("description").font(k.ty.ui).color(k.c.text_muted).wrap());
    col()
        .key("review")
        .w_full()
        .gap(10.0)
        .child(text("Review").key("title").font(k.ty.body_medium).color(k.c.text_base))
        .child(row().key("full-width-row").w_full().gap(16.0).center_y().child(text_col).child(switch))
}

/// Designs being tried out, off until switched on here.
fn experiments(s: &App, k: &Kit) -> El {
    let label = "A field for each side";
    let what = "The card editor puts each side in a box of its own, instead of one box with rules between.";
    let switch = Switch::new(s.separate_sides())
        .label(label)
        .hide_label()
        .on_toggle(|s: &mut App| s.toggle_separate_sides())
        .build(k)
        .key("separate-sides");
    let text_col = col()
        .key("text")
        .grow(1.0)
        .min_w(0.0)
        .gap(2.0)
        .child(text(label).key("label").font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(what).key("description").font(k.ty.ui).color(k.c.text_muted).wrap());
    col()
        .key("experiments")
        .w_full()
        .gap(10.0)
        .child(text("Experiments").key("title").font(k.ty.body_medium).color(k.c.text_base))
        .child(row().key("separate-sides-row").w_full().gap(16.0).center_y().child(text_col).child(switch))
}
