# Recall for desktop

The Rune desktop app. It hosts the core: the store and sync run on a thread of the app, serving the Unix socket the
CLI and MCP talk to (see `docs/contract.md`), so there's no separate core process. The UI is still a thin client of
that core: every read and write goes through `recall_core::client` over the socket, and it refetches whenever the core
pushes `changed`, so edits from the CLI or a sync show up live. The UI holds no store and does no scheduling.

This directory is its own Cargo workspace, so Rune's large dependency tree stays out of the root workspace. It builds
with the repo-wide Rust 1.95 pin (root `rust-toolchain.toml`), which Rune needs.

## Run

```sh
cd apps/desktop
cargo run                          # hosts the core at $RECALL_HOME and opens the window
cargo run -- --background          # hosts the core with no window (how the CLI starts the app)
cargo run -- --fixtures            # sample data in memory, no core (first-run | lived-in | caught-up)
```

Only one app runs per `RECALL_HOME`. Launching it while a core is running asks that app to show its window
(`app.open`) and exits. A `--background` app has no Dock icon until its window opens, and closing that window goes
back to no window while the core keeps serving; `recall shutdown` quits it. An app launched normally quits, and stops
its core, when its window closes. If the CLI is hosting the core itself (it does when it can't find the app), the app
runs as one more client of it.

To have the CLI start this checkout's app, build it and point `RECALL_APP` at `target/debug/recall-desktop`. Point
`RECALL_HOME` at a scratch directory to try it without touching your own decks. If the core can't be opened the app
says so and shows sample data.

The window opens before the core answers: the core starts on another thread once the first frame is up. A quick start
shows nothing in between; one that takes longer than 0.4 s says "Opening your decks…".

## Decks

The sidebar lists every deck with its subdecks indented under it, and the main pane always shows one of them (the
first, when the one it showed goes away); there's no view of every deck at once. The "+" by Decks opens a menu:
**New deck** names one in place, and **Import from Mochi…** asks for a `.mochi` export and has the core import it
(`import.mochi`, the same as `recall import mochi`), then opens the first deck it added with a line saying what came
in and what stayed behind.

**Review**, above the decks, is the inbox: every due card from every deck, with how many are waiting (`review.next`
without a deck). Each card says which deck it's from. When nothing's left it says "All caught up", and it picks up
cards as they come due or arrive from elsewhere.

## Review

A deck's review (Review in its title bar, or Space) and the inbox work the same way. The card sits near the top, as
wide as its text needs (420 to 760 pt), with the deck it's from above it (always in the inbox; in a deck's review
only for a subdeck's card). A click anywhere but the buttons, or Space, reveals the next side, under a dotted rule
across the card; "Click to reveal the next side" shows until they all are. The tags wait for the last side. The ✕
and ✓ buttons (or 1 and 2) are always there and answer at any side; each takes its answer's colour under the pointer.

## Card views

A deck's cards show as a **grid**, a **list** or a **column**, picked from the view menu under the title (or ⌘1, ⌘2,
⌘3). The choice is remembered per deck as a local preference (`rune::prefs`, app id `recall-desktop`), not in the
library.

- **Grid**: tiles in as many columns as fit (three in the default window), each with the card's front in Markdown
  (short fronts big, long ones smaller and faded where they run out), how many sides it hides and when it's due.
  The first tile is "New card".
- **List**: the same tiles, one per row, larger.
- **Column**: compact tiles and the search on the left; every side of the selected card on the right, with its
  reviews, lapses, when it's due and its tags. The pencil opens the editor; Delete stays in the editor.

The sort controls at the right of the row, beside "New card", order the cards Mochi's ways. The button shows the
sort's short name (Created, Updated, Name, Due, Reviewed, Interval, Retention, Random) and its menu the full ones:
**Date created** (the default), **Last updated**, **Alphanumeric** (by the front's first line, numbers by value),
**Due date**, **Last reviewed**, **Last interval**, **Retention rate** and **Random**. The arrow beside it flips the
direction (up for Oldest first, A to Z, …; down for Newest first, Z to A, …; its tooltip says which). Last updated
and Last reviewed start newest first, the rest ascending; cards never reviewed stay at the end either way. For Random
the arrow is a shuffle that reshuffles, as choosing Random again does. The core does the sorting (`cards.list` with
`sort`, `desc` and `seed`); the choice is remembered per deck like the view, and a remembered Random shuffles afresh
each time the deck opens. In a narrow pane the sort button shows only its icon, so "New card" still fits.

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
| Review | Space or Return (or a click) | Reveal the next side |
| Review | 1 / 2 | Forgot / Remembered, at any side |
| Review | Esc | End a deck's review |
| Editor | Return | New line; in a list, the next item (next number, empty task box), or ends the list on an empty one; on a line with only `---`, starts the next side there |
| Editor | Backspace (at the start of a side) | Join it to the side before |
| Editor | ↑ at a side's start / ↓ at its end | Move to the side before / after |
| Editor | ⌘Return | Save card |
| Editor | Esc | Cancel |
| Tag field | ↓ ↑, Return | Highlight a suggested tag, add it (while typing, the first match is highlighted) |
| Tag field | Esc | Close the suggestions (again: cancel) |
| Sidebar | ↑ ↓, Return | Move between decks |

## Editor

The editor is a dialog over the deck or review it was opened from. Its sides sit in a well at least 260 pt tall that
scrolls past about 400 pt (Settings → Experiments → "A field for each side" gives each side a box of its own
instead, the `editor-separate` scene). Clicking outside it cancels, as Esc does. Inside it is the card
as it reviews: its sides one under another, a dashed rule between, each side its
own field, all set alike and none of them named. A new card is one empty side; giving new cards a shape (a front and
a back, or "Word / Reading / Meaning") is for templates (PLAN.md, milestone 9), not the editor. Markdown is styled in
place as you write (bold reads bold, a heading reads large, code reads in mono on a tint, a link in the accent) while
the marks that do it (`**`, `#`, `>`, backticks, a link's URL) show, faded, only on the lines the caret or selection
is on, and are hidden elsewhere, so the rest reads as it will render. A bullet item's `- ` is always drawn as a
bullet, indented to its nesting, even on the caret's line: the caret steps over it, and Backspace at the item's start
deletes the space, so the plain `-` shows again. Numbers, task boxes, rules and HTML always show. It parses with the same parser the cards render with (`src/ui/markup.rs`). Return is a new line. Typing `---` on a line of its own and pressing Return
splits the side there, with what followed the caret starting the next side, and pasting a whole card splits it into
its sides; Backspace at the start of a side joins it back. **Add side** adds an empty one at the end. Empty sides are
dropped on save. Tags are chips in one field under the card: a comma, space or Return after a word makes it a chip
(a leading `#` is dropped, and a tag is only there once), Backspace in the empty field takes the last chip off, and
each chip's × removes it. A word still being typed saves too. While the field has the caret a menu under it (or above,
when there's no room) suggests tags already in the library that the card hasn't got, those used in the card's deck
(or its subdecks) first, then the rest, each part the most recently used (by when a card carrying them last changed)
first (`tags.list` with `deck_id`, fetched again when the card moves to another deck). Until you type it offers the
first eight of those, then every tag containing what's typed, those starting with it first. While you type, the first match is highlighted, so Return fills it in; ↓ ↑
move the highlight, and a click adds any. With nothing typed nothing is highlighted until ↓ or ↑. To add exactly
what you typed when it matches another tag, end it with a comma or space, or press Esc first. Esc closes the menu
until you type again.

## First run

The core seeds a "Recall basics" deck. While nothing has been reviewed, the app opens on that deck alone with one
action, Start review: no sidebar and no settings. The sidebar opens once the first review is finished.

## Screenshots

`./snap.sh` renders every screen headless from fixture scenes into `screenshots/` (light and dark, 1040×680 at 2×):
`first-run`, `review-front`, `review-revealed`, `review-graded`, `inbox`, `inbox-revealed`, `grid`, `grid-selected`, `grid-narrow` (820 wide),
`list`, `column`, `view-menu`, `sort-menu`, `search`, `deck-menu`, `connecting`, `editor`, `tag-menu`, `empty` and
`settings`.
Each is taken at 0.5 s, once the first layout has told the grid its width. One by hand:

```sh
cargo run -- --fixtures --snap screenshots/review.png --scene review-front --size 1040x680 --theme dark --at 0.5
```

`scripts/first-review.json` drives the first-run flow from the keyboard (Return, then Space and an answer five times);
pass it with `--script` and `--snap` at any `--at` time to see a frame mid-transition.

## Layout

- `src/core/`: the only code that talks to the core, and where the app hosts it (`Core::host`) and finds another app
  already running (`running`). `Core::Live` wraps `recall_core::client::Client`;
  `Core::Fixture` is an in-memory stand-in for scenes, tests and `--fixtures` (it moves answered cards a fixed time
  ahead and is not a scheduler).
- `src/app.rs`: state and actions, each a call into `Core` and a refetch.
- `src/editor.rs`: the card being written, one field per side, and how sides split and join.
- `src/ui/`: the views (sidebar, deck, review, editor, settings), built from rune-kit components; `cards.rs` has the
  grid, list and column.
- `src/theme.rs`: Anomaly's theme (`rune-theme-opencode`: kit tokens, Inter and IBM Plex Mono) plus Recall's own type styles.
- `RUNE-GAPS.md`: what Rune didn't do, and how the app works around it.
