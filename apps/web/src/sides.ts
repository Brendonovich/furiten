// The card editor's side editing, as plain functions (apps/desktop/src/editor.rs): a card is its sides joined by
// `---` lines, and the editor gives each side a field of its own.

const SEPARATOR = /^\s*---\s*$/;

/** Splits content into sides at `---` lines. */
export function splitSides(content: string): string[] {
  const sides: string[] = [];
  let current: string[] = [];
  for (const line of content.split("\n")) {
    if (SEPARATOR.test(line)) {
      sides.push(current.join("\n"));
      current = [];
    } else {
      current.push(line);
    }
  }
  sides.push(current.join("\n"));
  return sides;
}

/** What's saved: the non-empty sides joined by `---` lines. */
export function joinSides(sides: string[]): string {
  return sides
    .map((s) => s.replace(/^\n+|\s+$/g, ""))
    .filter((s) => s.trim() !== "")
    .join("\n---\n");
}

/**
 * Return pressed at `caret` in `text`. A line that is only `---` splits the side there; inside a list item the list
 * continues (an empty item ends it). `null` means a plain newline.
 */
export function onReturn(
  text: string,
  caret: number,
): { kind: "split"; before: string; after: string } | { kind: "edit"; text: string; caret: number } | null {
  const lineStart = text.lastIndexOf("\n", caret - 1) + 1;
  const lineEnd = text.indexOf("\n", caret) === -1 ? text.length : text.indexOf("\n", caret);
  const line = text.slice(lineStart, lineEnd);
  if (SEPARATOR.test(line) && caret === lineEnd) {
    return {
      kind: "split",
      before: text.slice(0, Math.max(0, lineStart - 1)),
      after: text.slice(lineEnd + 1),
    };
  }
  const item = /^(\s*)([-*+]|(\d+)([.)]))(\s+)(\[[ xX]\]\s+)?/.exec(line);
  if (!item || caret < lineStart + item[0].length) return null;
  if (line.slice(item[0].length).trim() === "") {
    // An empty item ends the list.
    const next = text.slice(0, lineStart) + text.slice(lineEnd);
    return { kind: "edit", text: next, caret: lineStart };
  }
  const [, indent, bullet, number, delim, gap, task] = item;
  const marker = number === undefined ? bullet : `${Number(number) + 1}${delim}`;
  const insert = `\n${indent}${marker}${gap}${task ? "[ ] " : ""}`;
  return { kind: "edit", text: text.slice(0, caret) + insert + text.slice(caret), caret: caret + insert.length };
}

/** Pasting splits on every `---` line in the pasted text. */
export function pasteSides(text: string, start: number, end: number, pasted: string): string[] | null {
  if (!pasted.split("\n").some((l) => SEPARATOR.test(l))) return null;
  return splitSides(text.slice(0, start) + pasted + text.slice(end));
}

/** Tags as the server stores them: lowercase, trimmed, `#` stripped. */
export function normalizeTag(tag: string): string {
  return tag.trim().replace(/^#+/, "").trim().toLowerCase();
}
