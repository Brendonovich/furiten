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

The window opens before the core answers: the connection (which may start the core) runs on another thread once the
first frame is up. A quick connection shows nothing in between; one that takes longer than 0.4 s says "Opening your
decks…".

## Card views

A deck's cards show as a **grid**, a **list** or a **column**, picked from the view menu under the title (or ⌘1, ⌘2,
⌘3). The choice is remembered per deck, and for All decks, as a local preference (`rune::prefs`, app id
`recall-desktop`), not in the library.

- **Grid**: tiles in as many columns as fit (three in the default window), each with the card's front in Markdown
  (short fronts big, long ones smaller and faded where they run out), how many sides it hides and when it's due.
  The first tile is "New card".
- **List**: the same tiles, one per row, larger.
- **Column**: compact tiles and the search on the left; every side of the selected card on the right, with its
  reviews, lapses, when it's due and its tags. The pencil opens the editor; Delete stays in the editor.

Search filters through the core (`cards.list` with `q`) after a short pause in typing, and the results follow the
core's `changed` events like everything else. Rows are virtual: only the tiles on screen are built. A deck lists at
most its first 500 cards.

## Keys

| Where | Key | Does |
| --- | --- | --- |
| Anywhere | ⌘N | New card |
| A deck | N | New card |
| A deck | ⌘1 / ⌘2 / ⌘3 | Grid / List / Column view |
| A deck | / | Search; Esc clears it, Esc again leaves it |
| A deck | ← → ↑ ↓ | Move the selection (by grid position in the grid) |
| A deck | Return | Open the selected card: the editor in Grid and List; in Column, the detail, then the editor |
| A deck | Esc | Clear the selection |
| Anywhere | ⌘, | Settings and sync |
| Anywhere | ⇧⌘B | Show or hide the sidebar |
| A deck | Space (or Return with nothing selected) | Start review |
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
`first-run`, `review-front`, `review-revealed`, `review-graded`, `grid`, `grid-selected`, `grid-narrow` (820 wide),
`list`, `column`, `view-menu`, `search`, `all-decks`, `connecting`, `editor`, `empty` and `settings`. Each is taken at
0.5 s, once the first layout has told the grid its width. One by hand:

```sh
cargo run -- --fixtures --snap screenshots/review.png --scene review-front --size 1040x680 --theme dark --at 0.5
```

`scripts/first-review.json` drives the first-run flow from the keyboard (Return, then Space and a grade five times);
pass it with `--script` and `--snap` at any `--at` time to see a frame mid-transition.

## Layout

- `src/core/`: the only code that talks to the core. `Core::Live` wraps `recall_core::client::Client`;
  `Core::Fixture` is an in-memory stand-in for scenes, tests and `--fixtures` (it moves answered cards a fixed time
  ahead and is not a scheduler).
- `src/app.rs`: state and actions, each a call into `Core` and a refetch.
- `src/ui/`: the views (sidebar, deck, review, editor, settings), built from rune-kit components; `cards.rs` has the
  grid, list and column.
- `src/theme.rs`: Anomaly's theme (`rune-theme-opencode`: kit tokens, Inter and IBM Plex Mono) plus Recall's own type styles.
- `RUNE-GAPS.md`: what Rune didn't do, and how the app works around it.
