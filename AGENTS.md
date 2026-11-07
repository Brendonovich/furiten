# Recall

Spaced-repetition flashcards (Markdown cards, FSRS, offline-first, synced) for desktop, mobile and web, with a CLI,
HTTP API and MCP server. Read `PLAN.md` for the architecture and `docs/contract.md` for formats, protocol and API.

## Layout

- `crates/srs`: FSRS-6 scheduler, no dependencies, also built to wasm for the server.
- `crates/core`: store (SQLite, device or hub), review queue, sync client and hub link, the hub's side over Iroh
  (`hub::host`, which `recall-hub` and a hosting desktop both run), and the core's socket server, which runs inside
  another process (no daemon).
- `crates/cli`: the `recall` binary, a client of the core. It starts the desktop app windowless when no core runs,
  and hosts the core itself for one command when there's no app.
- `apps/desktop`: the Rune desktop app. It hosts the core (`--background` for no window); its UI is a thin client.
- `crates/ffi`: the core behind a C ABI (`include/recall.h`), linked into the mobile apps.
- `apps/ios`: the SwiftUI app (iOS 26), native rather than themed: a sidebar with Review (every due card from
  every deck) above the decks (a drawer on iPhone, a split view on iPad), system colours and text styles, Liquid Glass
  for controls and navigation only, opaque grouped surfaces with the web app's lit edge for cards. It has the web app's
  features and concepts one for one (`apps/web` is the reference; port changes there to here). It links the core
  (`crates/ffi`, built by `apps/ios/scripts/core.sh` before each build) and keeps a library of its own, offline, which
  syncs with the desktop it's paired with over Iroh (the desktop hosts: Settings → "Sync with your phone", or
  `recall host pair`), with a `recall-hub`, or with Recall Cloud. XcodeGen spec in `project.yml`.
- `server`: Recall Cloud, the Cloudflare Worker and per-user Durable Object (TypeScript), deployed with Alchemy.
- `apps/web`: the web app in SolidJS 2 (pinned release candidate), mobile-first, over the HTTP API.
- `crates/hub`: `recall-hub`, the self-hosted backend (Rust, Iroh): sync, pairing, devices. Its HTTP API and `/mcp`
  are planned. The wire format, the device side and the serving side live in `crates/core` (`hub` feature, on by
  default); this crate adds the data directory, control socket and CLI.
- `tests/conformance`: one HTTP API suite both backends must pass. Planned.

## Commands

- `bun run check`: everything CI runs (Rust fmt, clippy, tests; server typecheck and tests). Done means it passes.
- `cargo run -p recall -- --help`; `cargo run -p recall-desktop`.
- `apps/ios/scripts/deploy.sh [device]` builds the iOS app and installs it on a paired iPhone (Wi-Fi is fine).
  `apps/ios/scripts/simulate.sh [-pair <ticket>] [-screen inbox|decks|settings|deck|new|editor]` runs it in the booted
  simulator, paired with a desktop (a ticket from `recall host pair`; try a scratch `RECALL_HOME`), on that screen
  (DEBUG launch arguments, for screenshots; `-reveal YES` shows every side under review).
- `cd server && bun run dev` runs the Worker locally; `cd apps/web && bun run dev` serves the web app on :5173
  (all interfaces, so a phone on the LAN or Tailscale can open it), proxying `/v1` to the Worker.
- `cd apps/web && bun run build`, then `recall web on`: the desktop app serves the built web app and the HTTP API on
  :8787 for a phone to use the desktop's library directly (no Cloud).

## Rules

- `docs/contract.md` is the source of truth. Change it in the same commit as code that changes behaviour.
- The CLI covers everything the desktop app can do, with `--json`. Add the command when you add the feature.
- Clients stay thin: domain logic goes in `crates/core` or `crates/srs`, never in an app.
- Pin every dependency to an exact version (Rune to a `rev`), and say why in the commit that adds one.
- Cloudflare resources are named `recall-<stage>-<name>` (Durable Objects are named by their Worker and class, so
  the per-user object is class `Users` on `recall-<stage>-api`). Deploys run only from CI: `dev` deploys dev, `production`
  (fast-forwarded from `dev`) deploys production.
- Tests run offline and headless.
