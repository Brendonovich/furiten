# Vendored Rune

[Rune](https://github.com/anomalyco/rune) at rev `77dd9132a8b64d0ef55b0e95ce4628bdfac47eeb` (MIT, see `LICENSE`),
cut down to what the desktop app builds: `rune`, `rune-kit`, `rune-motion`, `rune-staging` and
`rune-theme-opencode`, without their tests, examples, benches or READMEs.

Changes from upstream:

- **The text caret blinks** (anomalyco/rune#54): `crates/rune/src/runtime/caret_blink.rs`, with its field in
  `runtime.rs` and its use where `runtime/build.rs` paints the caret.
- **Even padding around leading icons** (rune-kit): a `Button`, `SidebarRow`, `MenuItem`, inline `Select` or `TextInput`
  with a leading icon puts it as far from the left edge as from the top and bottom. Upstream used the label padding (up to
  14 pt beside a 6 pt gap above).
- **`Select::icon`** (rune-kit): an icon before the value in the inline trigger.
- **The caret on an empty last line takes no indent** (`crates/rune/src/text/cursor.rs`): Parley lays out no line
  after a final line break, so the caret borrowed the line above's indent, drawing indented after a bullet item.
- **A line break in a paragraph is a line break** (`crates/rune-kit/src/markdown/parser.rs`): a soft break sets as
  `\n`, not a space, as GitHub comments and the web app's `breaks: true` render it. Upstream follows CommonMark.
- **`SplitView::push_main`** (rune-kit): the open sidebar pushes the main column past the window's trailing edge,
  keeping its width, rather than narrowing it. The app turns it on in windows too narrow to share.
- **`SplitView::sidebar_shown`** (rune-kit): how far open the sidebar is drawn (0 to 1), for an app that steps the
  motion itself a frame at a time. The panes lay out at each step instead of springing to where they end up, so the main
  pane's contents reflow as it resizes.
- **`El::animate_changes`** (`crates/rune/src/el/motion.rs`, with its field in `el.rs` and its use in
  `runtime/flatten.rs` and `runtime/memo_reuse.rs`): a subtree animates again under an ancestor's `instant_changes`,
  so the review card can grow to a side it reveals while the rest of the app lands every change at once.
- **`Toolbar::instant_contents`** (rune-kit, `shell/toolbar.rs`): the toolbar's slots land their changes at once
  while the band moves with a sliding sidebar, so the app can animate the sidebar and nothing else in the shell.
- Each `Cargo.toml` spells out `edition` and `license` instead of inheriting them from Rune's workspace, and drops the
  test, example and bench targets and dev-dependencies. `frame-profile`, `hot` (rune) and `mermaid`, `frame-profile`
  (rune-kit) are declared empty: their dependencies aren't vendored.

Once #54 lands, go back to a git `rev` in `apps/desktop/Cargo.toml` and delete this folder.
