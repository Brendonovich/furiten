# Furiten

Spaced-repetition flashcards (Markdown cards, FSRS, offline-first, synced) for desktop, mobile and web, with a CLI,
HTTP API and MCP server, and no cloud of our own: devices sync with the user's desktop or a self-hosted
`furiten-hub`, both over Iroh. Read `PLAN.md` for the architecture and `docs/contract.md` for formats, protocol and
API.

## Layout

- `crates/srs`: FSRS-6 scheduler, no dependencies.
- `crates/core`: store (SQLite, device or hub), review queue, sync client and hub link, the hub's side over Iroh
  (`hub::host`, which `furiten-hub` and a hosting desktop both run), and the core's socket server, which runs inside
  another process (no daemon).
- `crates/cli`: the `furiten` binary, a client of the core. It starts the desktop app windowless when no core runs,
  and hosts the core itself for one command when there's no app.
- `apps/desktop`: the Rune desktop app. It hosts the core (`--background` for no window); its UI is a thin client.
  `FURITEN_CHANNEL=dev` at build time makes it Furiten Dev, beside Furiten (own bundle ID, library and prefs).
- `crates/ffi`: the core behind a C ABI (`include/furiten.h`), linked into the mobile apps.
- `apps/ios`: the SwiftUI app (iOS 26), native rather than themed: a sidebar with Review (every due card from
  every deck) above the decks (a drawer on iPhone, a split view on iPad), system colours and text styles, Liquid Glass
  for controls and navigation only, opaque grouped surfaces with the web app's lit edge for cards. Settings → Theme
  swaps the system colours for the web app's palette (`Furiten`, always dark), accent included (`palette.accent`, the
  desktop's), and keeps the system accent only where the system draws it (glass buttons and controls);
  screens paint from `\.palette` (`Views/Theme.swift`), never fixed colours. It has the web app's
  features and concepts one for one (`apps/web` is the reference; port changes there to here). It links the core
  (`crates/ffi`, built by `apps/ios/scripts/core.sh` before each build) and keeps a library of its own, offline, which
  syncs with the desktop it's paired with over Iroh (the desktop hosts: Settings → "Sync with your phone", or
  `furiten host pair`), or with a `furiten-hub`. XcodeGen spec in `project.yml`.
- `apps/web`: the web app in SolidJS 2 (pinned release candidate), mobile-first, over the HTTP API.
- `apps/site`: the landing page, a static SolidJS 2 site built from the web app's own components and stylesheet
  (imported from `apps/web/src`, so install both). Its benefits are review cards (`src/cards.ts`) that open in turn
  on a schedule (`src/schedule.ts`) or when tapped. Not deployed yet.
- `crates/hub`: `furiten-hub`, the self-hosted backend (Rust, Iroh): sync, pairing, devices. Its HTTP API and `/mcp`
  are planned. The wire format, the device side and the serving side live in `crates/core` (`hub` feature, on by
  default); this crate adds the data directory, control socket and CLI.
- `tests/conformance`: one HTTP API suite both backends must pass. Planned.
- `assets/icons`: the one set of line icons the three apps draw (20×20 viewBox): the web renders them from
  `apps/web/src/Icon.tsx`, the desktop from `apps/desktop/src/ui/icons.rs`, and iOS as template vector image assets
  (`apps/ios/scripts/icons.sh` copies them in). Change a drawing here, not per app.

## Commands

- `bun run check`: everything CI runs (Rust fmt, clippy, tests; the web and site typechecks and tests). Done means it passes.
- `cargo run -p furiten -- --help`; `cargo run -p furiten-desktop`.
- `apps/desktop/scripts/bundle.sh [--dmg] [--notarize] [--zip]` builds Furiten.app (ad hoc signed, or with
  `APPLE_SIGNING_IDENTITY`) with the CLI and the web app inside; `FURITEN_CHANNEL=dev` builds Furiten Dev.app instead.
  Developer ID builds update themselves from Anomaly's release service at `anoma.ly/release` (`apps/desktop/src/updater`,
  the same client as Hex's: it polls the channel's manifest, checks each download's size and SHA-256, verifies the
  unpacked app against `apps/desktop/assets/update-requirement.txt`, and swaps the bundle in at restart or quit; the
  registry entry lives in `anomalyco/release`).
- `apps/ios/scripts/deploy.sh [device]` builds the iOS app and installs it on a paired iPhone (Wi-Fi is fine).
  `apps/ios/scripts/simulate.sh [-pair <ticket>] [-screen inbox|decks|settings|search|deck|new|editor|cram|tag]` runs it in the
  booted simulator, paired with a desktop (a ticket from `furiten host pair`; try a scratch `FURITEN_HOME`), on that
  screen (DEBUG launch arguments, for screenshots; `-reveal YES` shows every side under review, `-tag <tag>` is the
  tag `-screen tag` opens, `-tagging <text>` types into the editor's tag field).
- `cd apps/web && bun run dev` serves the web app on :5173 (all interfaces, so a phone on the LAN or Tailscale can
  open it), proxying `/v1` to a desktop's web access (`furiten web on`, :8787).
  `cd apps/site && bun run dev` serves the landing page on :5174.
- `scripts/icons.sh` renders the iOS, macOS and home screen icons from `apps/web/public/icon.svg`, and the desktop's dev
  icon (the same card drawn as a blueprint) from `apps/web/public/icon-dev.svg` (macOS only).
- `cd apps/web && bun run build`, then `furiten web on`: the desktop app serves the built web app and the HTTP API on
  :8787 for a phone to use the desktop's library directly, no hub needed.

## Rules

- `docs/contract.md` is the source of truth. Change it in the same commit as code that changes behaviour.
- The CLI covers everything the desktop app can do, with `--json`. Add the command when you add the feature.
- Clients stay thin: domain logic goes in `crates/core` or `crates/srs`, never in an app.
- Pin every dependency to an exact version (Rune to a `rev`), and say why in the commit that adds one.
- Tests run offline and headless.
