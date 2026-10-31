# Recall

Spaced-repetition flashcards (Markdown cards, FSRS, offline-first, synced) for desktop, mobile and web, with a CLI,
HTTP API and MCP server. Read `PLAN.md` for the architecture and `docs/contract.md` for formats, protocol and API.

## Layout

- `crates/srs`: FSRS-6 scheduler, no dependencies, also built to wasm for the server.
- `crates/core`: store (SQLite), review queue, sync client, and the `recall core` socket server.
- `crates/cli`: the `recall` binary. `recall core` runs the core; every other command is a client of it.
- `apps/desktop`: the Rune desktop app, a thin client of the core.
- `server`: Recall Cloud, the Cloudflare Worker and per-user Durable Object (TypeScript), deployed with Alchemy.
- `crates/hub`: `recall-hub`, the self-hosted backend (Rust, Iroh). Planned.
- `tests/conformance`: one HTTP API suite both backends must pass. Planned.

## Commands

- `bun run check`: everything CI runs (Rust fmt, clippy, tests; server typecheck and tests). Done means it passes.
- `cargo run -p recall -- --help`; `cargo run -p recall-desktop`.
- `cd server && bun run dev` runs the Worker locally.

## Rules

- `docs/contract.md` is the source of truth. Change it in the same commit as code that changes behaviour.
- The CLI covers everything the desktop app can do, with `--json`. Add the command when you add the feature.
- Clients stay thin: domain logic goes in `crates/core` or `crates/srs`, never in an app.
- Pin every dependency to an exact version (Rune to a `rev`), and say why in the commit that adds one.
- Cloudflare resources are named `recall-<stage>-<name>`. Deploys run only from CI: `dev` deploys dev, `production`
  (fast-forwarded from `dev`) deploys production.
- Tests run offline and headless.
