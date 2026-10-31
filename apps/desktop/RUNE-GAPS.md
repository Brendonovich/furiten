# Rune gaps

What the app needed from Rune (rev `77dd9132`) and didn't get, how to see it, and what the app does instead.

## A single-line field's placeholder wraps out of the field

`TextInput` builds a `single_line()` text input, but a placeholder wider than the field wraps onto a second line that
draws below the field's border.

Repro: `TextInput::new(&TextEdit::new("")).placeholder("Server address, like https://recall.example.com").build(&k)`
at its default 280 pt width.

Workaround: a short placeholder (`https://recall.example.com`) and `.width(None)` so the field fills its row.

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

## Notes (not gaps)

- A subtree swapped by changing its key, with `.transition(Transition::none())`, still slid sideways: its children
  ride their parent's animating edges. The review controls use `.instant_changes()` on their container so the
  buttons swap in one frame.
- A headless script's key step names the key with `"name"` (`{"type": "key", "name": "3"}`); `--help` doesn't say
  which field.
