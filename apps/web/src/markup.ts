// Markdown styled in place while you write it (apps/desktop/src/ui/markup.rs): bold reads bold, a heading reads large,
// code reads in mono on a tint. The marks that do it (`**`, `#`, backticks, a link's URL) stay in the text, faded, on
// the lines the caret or selection is on, and are hidden everywhere else, so the rest of the card reads as it will
// render. A bullet item's `-` always gives way to a drawn bullet, on the caret's line too. Numbers, task boxes, rules
// and HTML always show, faded, as they read the same rendered.

import type { SyntaxNode } from "@lezer/common";
import { GFM, parser } from "@lezer/markdown";

const md = parser.configure(GFM);

export type Range = { from: number; to: number };

export type Markup =
  /** Text styled as it renders. */
  | { kind: "style"; from: number; to: number; style: "strong" | "em" | "strike" | "code" | "link" | "url" | "quote" }
  /** A whole line styled, by the position of its start. */
  | { kind: "line"; at: number; style: "h1" | "h2" | "h3" | "h4" | "code" }
  /** A mark that shows, faded. */
  | { kind: "fade"; from: number; to: number }
  /** A mark that's hidden. */
  | { kind: "hide"; from: number; to: number }
  /** A bullet item's marker, indentation and all, drawn as a bullet at `depth`. */
  | { kind: "bullet"; from: number; to: number; depth: number };

/** How `src` is styled, with the marks on the lines touching `shown` showing; `null` hides them all. */
export function markup(src: string, shown: Range | null): Markup[] {
  const out: Markup[] = [];
  const fade = (from: number, to: number) => from < to && out.push({ kind: "fade", from, to });
  const mark = (from: number, to: number) => {
    if (from >= to) return;
    const visible = shown !== null && from <= shown.to && shown.from <= to;
    out.push({ kind: visible ? "fade" : "hide", from, to });
  };
  const children = (node: SyntaxNode, name: string) => node.getChildren(name);
  const lineStarts = (from: number, to: number) => {
    const starts = [src.lastIndexOf("\n", from - 1) + 1];
    for (let i = src.indexOf("\n", from); i !== -1 && i < to; i = src.indexOf("\n", i + 1)) starts.push(i + 1);
    return starts;
  };
  const spaces = (at: number) => {
    let end = at;
    while (src[end] === " " || src[end] === "\t") end++;
    return end;
  };

  md.parse(masked(src)).iterate({
    enter: ({ name, from, to, node }) => {
      switch (name) {
        case "StrongEmphasis":
        case "Emphasis":
          out.push({ kind: "style", from, to, style: name === "Emphasis" ? "em" : "strong" });
          for (const m of children(node, "EmphasisMark")) mark(m.from, m.to);
          break;
        case "Strikethrough":
          out.push({ kind: "style", from, to, style: "strike" });
          for (const m of children(node, "StrikethroughMark")) mark(m.from, m.to);
          break;
        case "InlineCode":
          out.push({ kind: "style", from, to, style: "code" });
          for (const m of children(node, "CodeMark")) mark(m.from, m.to);
          return false;
        case "Link":
        case "Image": {
          out.push({ kind: "style", from, to, style: "link" });
          const marks = children(node, "LinkMark");
          const [open, close, paren] = marks;
          // [text](url): the brackets and the URL fade.
          if (open && close && paren && src[paren.from] === "(") {
            mark(open.from, open.to);
            mark(close.from, to);
          }
          return false;
        }
        case "Autolink":
        case "URL":
          out.push({ kind: "style", from, to, style: "url" });
          return false;
        case "ATXHeading1":
        case "ATXHeading2":
        case "ATXHeading3":
        case "ATXHeading4":
        case "ATXHeading5":
        case "ATXHeading6":
        case "SetextHeading1":
        case "SetextHeading2": {
          const level = Math.min(Number(name.at(-1)), 4) as 1 | 2 | 3 | 4;
          for (const at of lineStarts(from, to)) out.push({ kind: "line", at, style: `h${level}` });
          const marks = children(node, "HeaderMark");
          if (name.startsWith("Setext")) {
            // The `===` underline: a line of its own, so it stays.
            for (const m of marks) fade(m.from, m.to);
          } else {
            const [first, last] = [marks[0], marks.length > 1 ? marks.at(-1) : undefined];
            if (first) mark(first.from, spaces(first.to));
            if (last) mark(last.from, last.to);
          }
          break;
        }
        case "Blockquote":
          out.push({ kind: "style", from, to, style: "quote" });
          break;
        case "QuoteMark":
          // The `>` and the space after it, as a heading's `#`.
          mark(from, to + (src[to] === " " ? 1 : 0));
          break;
        case "ListMark": {
          if (node.parent?.parent?.name === "BulletList" && bullet(src, from)) break;
          fade(from, to);
          break;
        }
        case "TaskMarker":
        case "HorizontalRule":
        case "HTMLBlock":
        case "HTMLTag":
          fade(from, to);
          return false;
        case "FencedCode":
        case "CodeBlock":
          for (const at of lineStarts(from, to)) {
            out.push({ kind: "line", at, style: "code" });
            const end = src.indexOf("\n", at) === -1 ? src.length : src.indexOf("\n", at);
            const line = src.slice(at, end).trimStart();
            if (name === "FencedCode" && (line.startsWith("```") || line.startsWith("~~~"))) mark(at, end);
          }
          return false;
      }
    },
  });

  for (const r of breaks(src)) if (emptyBullet(src.slice(r.from, r.to)) === null) fade(r.from, r.to);
  // Every bullet's marker, the empty ones the parser no longer sees included, gives way to a drawn bullet.
  for (const b of bullets(src)) out.push({ kind: "bullet", ...b });
  return out;
}

/** Each bullet item's marker (from its line's start through the `-` and its space), with its nesting depth. */
export function bullets(src: string): { from: number; to: number; depth: number }[] {
  const out: { from: number; to: number; depth: number }[] = [];
  md.parse(masked(src)).iterate({
    enter: ({ name, from, node }) => {
      if (name !== "ListMark" || node.parent?.parent?.name !== "BulletList") return;
      const marker = bullet(src, from);
      if (!marker) return;
      let depth = -1;
      for (let n: SyntaxNode | null = node.parent; n; n = n.parent) {
        if (n.name === "BulletList" || n.name === "OrderedList") depth++;
      }
      out.push({ ...marker, depth: Math.max(0, depth) });
    },
  });
  // Empty bullets, blanked for the parser: as deep as the bullet before at the same indent, one deeper under a
  // shallower one.
  for (const { at, line } of lines(src)) {
    const end = emptyBullet(line);
    if (end === null) continue;
    const indent = line.length - line.replace(/^[ \t]+/, "").length;
    const above = out.filter((b) => b.from < at).sort((a, b) => b.from - a.from)[0];
    let depth = 0;
    if (above) {
      const text = src.slice(above.from, above.to);
      const aboveIndent = text.length - text.replace(/^[ \t]+/, "").length;
      depth = indent === aboveIndent ? above.depth : indent > aboveIndent ? above.depth + 1 : Math.max(0, above.depth - 1);
    }
    out.push({ from: at, to: at + end, depth });
  }
  return out.sort((a, b) => a.from - b.from);
}

/** The lines of `src` holding the selection's `from` and `to`. */
export function shownLines(src: string, from: number, to: number): Range {
  const start = src.lastIndexOf("\n", from - 1) + 1;
  const end = src.indexOf("\n", to);
  return { from: start, to: end === -1 ? src.length : end };
}

/**
 * For a bullet item (`-`, `*` or `+`) at `item`, alone at the start of its line, the range from the line's start
 * through the marker and the space after it. Only once a space follows: a lone `-` may be the start of a `---` break.
 */
function bullet(src: string, item: number): Range | null {
  const start = src.lastIndexOf("\n", item - 1) + 1;
  if (!/^[ \t]*$/.test(src.slice(start, item)) || !"-*+".includes(src[item] ?? "x")) return null;
  let end = item + 1;
  while (src[end] === " " || src[end] === "\t") end++;
  return end > item + 1 ? { from: start, to: end } : null;
}

/**
 * The lines of only dashes, after any indent and before any trailing spaces: a side break as it's typed (`-`, `--`,
 * `---`), or an empty bullet (`- `). Markdown would read any of them under a paragraph as a setext heading's
 * underline, turning the paragraph into a heading.
 */
function breaks(src: string): Range[] {
  return lines(src)
    .filter(({ line }) => /^[ \t]*-+[ \t]*\r?$/.test(line))
    .map(({ at, line }) => ({ from: at, to: at + line.length }));
}

/** An empty bullet item, `- ` and nothing after: the length of the marker, through its spaces. */
function emptyBullet(line: string): number | null {
  const l = line.replace(/\r$/, "");
  return /^[ \t]*-[ \t]+$/.test(l) ? l.length : null;
}

/** `src` with its lines of only dashes blanked, character for character, so Markdown doesn't read one as a rule or a
 * setext heading's underline. */
function masked(src: string): string {
  let out = src;
  for (const r of breaks(src)) out = out.slice(0, r.from) + " ".repeat(r.to - r.from) + out.slice(r.to);
  return out;
}

function lines(src: string): { at: number; line: string }[] {
  let at = 0;
  return src.split("\n").map((line) => {
    const start = at;
    at += line.length + 1;
    return { at: start, line };
  });
}
