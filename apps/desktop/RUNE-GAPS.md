# Rune gaps

What the app needed from Rune (rev `77dd9132`) and didn't get, how to see it, and what the app does instead.

## A single-line field's placeholder wraps out of the field

`TextInput` builds a `single_line()` text input, but a placeholder wider than the field wraps onto a second line that
draws below the field's border.

Repro: `TextInput::new(&TextEdit::new("")).placeholder("Server address, like https://recall.example.com").build(&k)`
at its default 280 pt width.

Workaround: a short placeholder (`https://recall.example.com`) and `.width(None)` so the field fills its row. The
deck's "Search cards" field also wrapped in a narrow window, so its box has `min_w(180.0)`.

## `TextArea` can't take a font

`TextInput` has `text_style(..)`, `TextArea` doesn't: it always sets `k.ty.ui` at a 1.35 line height. The card
editor wants a monospace face so `---` and Markdown markers line up.

Repro: try to build `TextArea::new(&edit)` in `TextStyle::new(13.0, 400.0).mono()`.

Workaround: the editor uses the kit's UI face.

## `TextArea` can't fill the height it's given

A `TextArea` is sized by `rows(n)`, `auto_grow(min, max)` or `max_height(h)`; there's no way to make it take the
height its parent leaves, as a `grow(1.0)` column with `min_h(0.0)` and a scroll viewport inside would. The editor
wants the text area to run down the pane beside the preview.

Workaround: `rows(12).max_height(520.0)`, with the tags and deck fields placed under it.

## Headless runs can't wait for something outside the process

The headless clock is virtual and runs as fast as it can, so a scripted run can't wait for an event that arrives
from another process, such as the core's `changed` push after `recall add` in another terminal. Live refresh can only
be checked with a real window.

Repro: `recall-desktop --snap out.png --frames 600 &` then `recall add ...` a second later: the run has finished
before the event arrives.

Workaround: checked by hand with a windowed run and the CLI. A script step that waits in real time (or until the app's
waker fires) would let this be a test.

## Markdown can't be clamped to a height with a fade only when it overflows

A grid tile shows the card's front as Markdown up to a height, fading where it runs out. `Markdown::render` has no
line clamp, and `clip_fade` on a `max_h` box fades its bottom edge whether or not anything is hidden, so a short
front loses its last line to the fade. Text has `max_lines`, but Markdown is several text blocks.

Repro: `col().max_h(60.0).clip_fade(0.0, 0.0, 0.0, 24.0).child(Markdown::new("one line").render(&style))`: the one
line is faded.

Workaround: `src/ui/cards.rs` estimates the front's height from its length and the tile's width and adds the fade
only when the estimate overflows. A fade that follows hidden content (as `scroll_edge_fade` does for viewports) would
remove the guess.

## No dashed rule

`Divider` is solid only. `border_dash` dashes a box's border, but a one-point-tall box draws its top and bottom
edges with dashes running in opposite directions, so they don't line up.

Repro: `el().w_full().h(1.0).border(1.0, c).border_dash(3.0, 3.0)`.

Workaround: a one-point box with `clip_rounded(0.0)` around an eight-point dashed box, so only its top edge shows
(`dashed` in `src/ui/cards.rs`).

## A snapshot at the first frame comes before `on_layout`

The grid picks its column count from the card area's width, which it learns from `on_layout`. `--snap` without
`--at` draws the first frame, before that report has run the view again, so the grid shows its guess (three
columns) at any window size.

Repro: `recall-desktop --fixtures --snap out.png --scene grid-narrow --size 820x680` shows three columns; with
`--at 0.5`, two.

Workaround: `snap.sh` takes every scene at `--at 0.5`. A view-time read of a container's width before the first
layout (or a snapshot that waits for layout reports to settle) would fix the first frame too.

## Grid cells overflow without `min_w(0)`

`equal_columns(n)` makes `1fr` columns, which (as in CSS) won't shrink below their content's min width, so a footer
of unbreakable text pushed the third tile out of the row.

Repro: `row().equal_columns(3)` with children whose content is wider than a third.

Workaround: `min_w(0.0)` on every cell. `equal_columns` could use `minmax(0, 1fr)`.

## Notes (not gaps)

- A subtree swapped by changing its key, with `.transition(Transition::none())`, still slid sideways: its children
  ride their parent's animating edges. The review controls use `.instant_changes()` on their container so the
  buttons swap in one frame.
- A headless script's key step names the key with `"name"` (`{"type": "key", "name": "3"}`); `--help` doesn't say
  which field.
