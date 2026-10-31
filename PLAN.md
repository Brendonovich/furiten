# Recall: plan

Recall is a spaced-repetition flashcards app in the style of Mochi: cards are Markdown, `---` splits the sides,
decks nest, everything works offline and syncs. It runs on Mac, Windows, Linux, iOS, Android and the web, and agents
drive it through a CLI, an HTTP API and an MCP server.

## Architecture

```
 desktop (Mac/Win/Linux)                         mobile                      web
┌─────────────────────────────┐   ┌───────────────────────────────┐   ┌─────────────────┐
│ Rune app      recall CLI    │   │ SwiftUI app    Compose app    │   │ DOM app (Solid) │
│      \          /           │   │      │              │         │   │                 │
│   Unix socket, JSON lines   │   │  core linked in (no daemons   │   │                 │
│          │                  │   │  on iOS/Android), via C ABI   │   │                 │
│   recall core (Rust)        │   │  recall core (Rust)           │   │                 │
│   SQLite · FSRS · sync      │   │  SQLite · FSRS · sync         │   │                 │
└──────────┬──────────────────┘   └──────────────┬────────────────┘   └───────┬─────────┘
           │  POST /v1/sync                      │  POST /v1/sync             │ HTTP API
           ▼                                     ▼                            ▼
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ Cloudflare Worker (Effect, deployed with Alchemy)  /health  /v1/*  /mcp                   │
│   → one Durable Object per user, SQLite storage: decks, cards, reviews, change log        │
│   → crates/srs compiled to wasm, so the server schedules exactly like every client        │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

- **One core.** `crates/core` owns the domain: the store, the review queue, sync. On desktop it runs as its own
  process (`recall core`) and the app and CLI are thin clients over one socket, so the CLI can do everything the app
  does. iOS and Android can't keep a background process, so they link the same crate.
- **One scheduler.** `crates/srs` is FSRS-6 with no dependencies. Schedules are derived by folding reviews, which
  makes offline reviews on two devices merge without conflicts. The Worker runs the same code as wasm.
- **The server only holds what devices share.** Each user's Durable Object is the sync hub and the endpoint for
  agents that aren't on the user's machine (the HTTP API and MCP).
- **Native shells, Rune where it's ready.** Rune draws the desktop app. iOS is SwiftUI, Android is Jetpack Compose,
  and the web is a DOM app, because Rune's iOS target only embeds, it has no Android target, and its web target is
  experimental. Each moves to Rune surfaces as those targets mature.

The exact formats, protocol and endpoints are in [docs/contract.md](docs/contract.md).

## First run

The aha moment is answering your first card and watching it get scheduled. The first launch opens straight into a
sample deck ("Recall basics", five cards that teach the app: `---`, grading keys, the CLI) with one button, Start
review. No sign-up, no choices. Sync, import and settings come after the first review.

## Milestones

1. **Core and CLI.** `crates/srs`, `crates/core`, `recall` CLI with `--json` on every command, `recall core` daemon.
2. **Server.** Worker, per-user Durable Object, HTTP API, MCP, sync, `/health`; `dev` and `production` stages with
   Alchemy, deployed only from CI.
3. **Desktop app in Rune.** Deck list, review screen with keyboard grading, card editor, first-run sample deck.
4. **Sync end to end.** Two cores against a local Worker converge; the desktop app shows sync state.
5. **iOS (SwiftUI)** over the core through a C ABI. **Android (Compose)** the same way.
6. **Web.** DOM app over the HTTP API; offline later with the core in wasm.
7. Import from Anki (`.apkg`) and Mochi (`.mochi`), cloze cards, images, real sign-in, published decks.

## Decisions

- FSRS-6 over SM-2: better retention per review, and it's what Anki now defaults to. Written ourselves (it's a page
  of maths) so the wasm build stays tiny and there's nothing to keep up with.
- Last-writer-wins per deck and card, union for reviews. Field-level merge isn't worth it until someone loses an edit.
- Interim token accounts so the API and MCP are usable now; real sign-in is milestone 7.

## Open

- Windows has no Unix socket path in std; the desktop core needs a named pipe there.
- Where the internal project site lives once there's a domain.
