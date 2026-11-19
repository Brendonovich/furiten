# Furiten: plan

Furiten is a spaced-repetition flashcards app in the style of Mochi: cards are Markdown, `---` splits the sides,
decks nest, everything works offline and syncs. It runs on Mac, Windows, Linux, iOS, Android and the web, and agents
drive it through a CLI, an HTTP API and an MCP server.

## Architecture

```
 desktop (Mac/Win/Linux)        mobile                          web
┌────────────────────────────┐  ┌────────────────────────────┐  ┌────────────────────────────┐
│ furiten CLI, MCP           │  │ SwiftUI app, Compose app   │  │ SolidJS 2 app, no library  │
│ Unix socket, JSON lines    │  │ core linked in (C ABI):    │  │ of its own: the desktop's  │
│                            │  │ SQLite·FSRS·sync, offline, │  │ web access serves it the   │
│ Rune app, window optional  │  │ syncs when it's online     │  │ HTTP API and the app on    │
│   └ core: SQLite·FSRS·sync │  └─────────────┬──────────────┘  │ :8787 (a hub can too)      │
└─────────────┬──────────────┘                                  └────────────────────────────┘

              │                               │
                one hub per library, or none:
              ▼                               ▼

┌───────────────────────────────────────────────────────────────────────────────┐
│ the hub: a hosting desktop (Settings → "Sync with your phone"), or furiten-hub│
│ self-hosted on a NAS, Pi or VPS · sync and pairing over Iroh (no ports, DNS or│
│ TLS) · both run crates/core's store, merge and hub side, so they behave alike │
└───────────────────────────────────────────────────────────────────────────────┘
```

- **One core.** `crates/core` owns the domain: the store, the review queue, sync. On desktop the Rune app hosts it
  and serves one socket; the app's window and the CLI are thin clients of it, so the CLI can do everything the app
  does. There's no separate daemon: when the CLI or an agent needs the core and nothing is running, the app starts
  without a window (`--background`), and opening it later shows that same app's window. iOS and Android link the
  same crate.
- **One scheduler.** `crates/srs` is FSRS-6 with no dependencies. Schedules are derived by folding reviews, which
  makes offline reviews on two devices merge without conflicts.
- **One backend, one protocol.** A library syncs with exactly one hub: `furiten-hub`, a Rust binary networked with
  Iroh, so it runs behind NAT with nothing to configure and only paired devices can reach it. It's an always-online
  hub rather than a mesh, because flashcards need a phone and a sleeping laptop to converge. The hub is
  `crates/core`'s own store and merge, so a hosting desktop and a `furiten-hub` are the same thing; a conformance
  suite keeps the HTTP API honest against both ways of serving it.
- **Your desktop can be the hub.** With no `furiten-hub`, the desktop app hosts its own
  library over the hub's Iroh protocol (Settings → "Sync with your phone", `furiten host pair`): a phone scans a QR
  code, keeps a library of its own, reviews and edits offline, and syncs when both are online. The desktop's store
  serves sync itself, so there's one copy, and the host code is `crates/core`'s, shared with `furiten-hub`.
- **Native shells, Rune where it's ready.** Rune draws the desktop app. iOS is SwiftUI, Android is Jetpack Compose,
  and the web is a SolidJS 2 app, because Rune's iOS target only embeds, it has no Android target, and its web target is
  experimental. Each moves to Rune surfaces as those targets mature.

The exact formats, protocol and endpoints are in [docs/contract.md](docs/contract.md).

## First run

The aha moment is answering your first card and watching it get scheduled. The first launch opens straight into a
sample deck ("Furiten basics", five cards that teach the app: `---`, grading keys, the CLI) with one button, Start
review. No sign-up, no choices. Sync, import and settings come after the first review.

## Milestones

1. **Core and CLI.** `crates/srs`, `crates/core`, `furiten` CLI with `--json` on every command, the core hosted by
   the desktop app (windowless when only the CLI needs it).
2. **Desktop app in Rune.** Deck list, review screen answered from the keyboard, card editor, first-run sample deck.
3. **Sync end to end.** Two cores against one hub converge; the desktop app shows sync state.
4. **Self-hosted hub.** `furiten-hub` with Iroh sync and pairing, `furiten sync login --hub <ticket>`, live change
   notifications, its HTTP API and MCP, and `tests/conformance` passing against every way of serving it.
   `furiten mcp` over stdio on every device.
5. **iOS (SwiftUI)** over the core through a C ABI: its own library, offline, synced with the desktop it's paired
   with (hosting) or a `furiten-hub`. **Android (Compose)** the same way.
6. **Web.** SolidJS 2 app (`apps/web`) over the HTTP API, mobile-first: the sidebar pulls over from the left, with Review (every
   due card from every deck, the first screen) above the decks; review reveals sides with a tap and answers with a swipe
   at any side (right Remembered, left Forgot) or, once every side shows, the two buttons. It runs against the
   desktop: with web access
   on, the desktop app serves the HTTP API and the built web app on :8787, so a phone uses its library live, no hub
   needed; `furiten-hub --http` can serve it the same way later. No Mochi import from the browser yet: the core
   reads the file (`furiten import mochi`, the app's menu). Solid 2 is still a release candidate (`solid-js@2.0.0-rc.20` on the `next` tag, with
   `@solidjs/web@2.0.0-rc.20`, and `@solidjs/router@2.0.0-next.44`), so pin exact versions and bump deliberately until
   it's stable. Every screen, sheet, filter and tab is a URL (`apps/web/src/routes.ts`): `/`, `/settings`,
   `/decks/:deck` and `/decks/:deck/review`, with search, view, sort, the open card, the editor and the deck menu in
   the query, so reloads, Back and shared links land where you were. Whatever serves the app answers unknown paths
   with `index.html`.
7. Import from Anki (`.apkg`) and Mochi (`.mochi`), cloze cards, images, real sign-in, published decks.
8. **Templates.** A card is just its sides, in order; the app names none of them. A template is what gives a deck's
   new cards a shape (say "Word / Reading / Meaning", or a front and a back), prefilled into the editor's sides. Mochi
   templates import into it rather than being flattened.

## Decisions

- FSRS-6 over SM-2: better retention per review, and it's what Anki now defaults to. Written ourselves (it's a page
  of maths) so there's nothing to keep up with.
- Last-writer-wins per deck and card, union for reviews. Field-level merge isn't worth it until someone loses an edit.
- The HTTP API is served by the devices that hold the library: a desktop's web access with its own token, and
  `furiten-hub tokens create` for a hub's. No accounts anywhere.
- A desktop that hosts is a hub, not a peer: phones sync with it and never with each other, so the merge rules
  and the protocol are the hub's. The cost is that a phone only syncs while the desktop app is running somewhere;
  someone who wants an always-on hub runs `furiten-hub` instead.
- Self-hosting is a Rust hub over Iroh, not device-to-device sync: pure P2P only syncs when two devices are online
  together, which suits a password manager but not a daily review habit. Iroh is the transport only (not
  `iroh-docs`), so our merge rules stay in one place. See [research/iroh-sync.md](research/iroh-sync.md).
- Cards have N sides and the app doesn't call them front and back: side 0 is just what review shows first. Shapes
  like front/back belong to templates (milestone 8), not to every card.
- One backend language: Rust, where the core already is. The HTTP API exists once, in `crates/core`, served by a
  desktop's web access and by `furiten-hub --http`; Furiten Cloud (a second implementation, in TypeScript on
  Cloudflare) was removed before launch — two of everything to keep aligned, and a deploy of our own to keep alive,
  for a hub the user already owns.

## Open

- Windows has no Unix socket path in std; the desktop core needs a named pipe there.
- Iroh on iOS needs libc pinned to 0.2.189 (`crates/ffi`): 0.2.190 made `in6_ifreq` and the routing socket
  constants macOS-only, and Iroh's `netdev` and `netwatch` use them. Unpin once libc puts them back on iOS.
- A desktop that hosts and a phone can't also use a `furiten-hub`: one hub per library. Should hosting hand
  over to a hub later (the desktop signs in, its phones re-pair)?
- Moving a library between hubs: export and re-import, or a one-shot `furiten sync migrate`?
- Hubs and wall-clock LWW: switch `updated_at` to a hybrid logical clock before many devices share a hub?
- Where the internal project site lives once there's a domain.
