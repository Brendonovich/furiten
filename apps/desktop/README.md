# Recall for desktop

The Rune desktop app. It's a thin client of the `recall core` daemon: every read and write goes through
`recall_core::client` over the core's Unix socket (see `docs/contract.md`), and it refetches whenever the core pushes
`changed`, so edits from the CLI or a sync show up live. It holds no store and does no scheduling.

This directory is its own Cargo workspace, so Rune's large dependency tree stays out of the root workspace. It builds
with the repo-wide Rust 1.95 pin (root `rust-toolchain.toml`), which Rune needs. One pin for both workspaces matters:
the app builds the root workspace's `recall`, and two toolchains would keep invalidating each other's builds.

## Run

```sh
cd apps/desktop
cargo run                          # connects to the core at $RECALL_HOME, starting `recall core` if needed
cargo run -- --fixtures            # sample data in memory, no core (first-run | lived-in | caught-up)
```

The app starts the core with `$RECALL_BIN`, else a `recall` next to its own binary, else `recall` on `PATH`. Debug
builds first run `cargo build -p recall` in the root workspace and set `RECALL_BIN` to it, so `cargo run` here always
uses the core from the same checkout. A core that's already running keeps serving until you `recall shutdown`. Point
`RECALL_HOME` at a scratch directory to try it without touching your own decks. If the core can't be reached the app says so and shows
sample data.

## Keys

| Where | Key | Does |
| --- | --- | --- |
| Anywhere | ⌘N | New card |
| Anywhere | ⌘, | Settings and sync |
| Anywhere | ⇧⌘B | Show or hide the sidebar |
| A deck | Space or Return | Start review |
| Review | Space or Return | Reveal the next side |
| Review | 1 / 2 / 3 / 4 | Again / Hard / Good / Easy (once every side is showing) |
| Review | Esc | End review |
| Editor | ⌘Return | Save card |
| Editor | Esc | Cancel |
| Sidebar | ↑ ↓, Return | Move between decks |

In the editor a line with only `---` starts the next side; the preview shows each side as it will be reviewed. Tags
are separated by commas or spaces.

## First run

The core seeds a "Recall basics" deck. While nothing has been reviewed, the app opens on that deck alone with one
action, Start review: no sidebar and no settings. The sidebar opens once the first review is finished.

## Screenshots

`./snap.sh` renders every screen headless from fixture scenes into `screenshots/` (light and dark, 1040×680 at 2×):
`first-run`, `review-front`, `review-revealed`, `review-graded`, `deck`, `editor`, `empty` and `settings`. One by
hand:

```sh
cargo run -- --fixtures --snap screenshots/review.png --scene review-front --size 1040x680 --theme dark
```

`scripts/first-review.json` drives the first-run flow from the keyboard (Return, then Space and a grade five times);
pass it with `--script` and `--snap` at any `--at` time to see a frame mid-transition.

## Layout

- `src/core/`: the only code that talks to the core. `Core::Live` wraps `recall_core::client::Client`;
  `Core::Fixture` is an in-memory stand-in for scenes, tests and `--fixtures` (it moves answered cards a fixed time
  ahead and is not a scheduler).
- `src/app.rs`: state and actions, each a call into `Core` and a refetch.
- `src/ui/`: the views (sidebar, deck, review, editor, settings), built from rune-kit components.
- `src/theme.rs`, `assets/theme.json`: Recall's palette, turned into a rune-kit `Kit`.
- `RUNE-GAPS.md`: what Rune didn't do, and how the app works around it.
