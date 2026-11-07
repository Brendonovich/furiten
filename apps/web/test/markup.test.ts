import { describe, expect, test } from "bun:test";
import { bullets, type Markup, markup, shownLines } from "../src/markup.ts";

const text = (src: string, ms: Markup[], kind: "fade" | "hide") =>
  ms.flatMap((m) => (m.kind === kind ? [src.slice(m.from, m.to)] : []));
const all = (src: string) => ({ from: 0, to: src.length });

describe("markup, as the desktop styles it", () => {
  test("marks fade and what they mark is styled", () => {
    const src = "# Title\n\n**la** *merienda* `code` [link](https://x.y)\n\n- item\n> quote";
    const ms = markup(src, all(src));
    expect(text(src, ms, "fade")).toEqual(["# ", "**", "**", "*", "*", "`", "`", "[", "](https://x.y)", "> "]);
    const styled = (style: string) => ms.flatMap((m) => (m.kind === "style" && m.style === style ? [src.slice(m.from, m.to)] : []));
    expect(styled("strong")).toEqual(["**la**"]);
    expect(styled("code")).toEqual(["`code`"]);
    expect(ms).toContainEqual({ kind: "line", at: 0, style: "h1" });
  });

  test("only the caret's line shows its marks", () => {
    const src = "# Title\n**la** *merienda*\n- item";
    const at = src.indexOf("la");
    const ms = markup(src, shownLines(src, at, at));
    expect(text(src, ms, "fade")).toEqual(["**", "**", "*", "*"]);
    expect(text(src, ms, "hide")).toEqual(["# "]);
    expect(text(src, markup(src, null), "hide")).toEqual(["# ", "**", "**", "*", "*"]);
  });

  test("a side break still being typed is not a heading underline", () => {
    for (const line of ["-", "--", "---"]) {
      const src = `píng guǒ\napple\n${line}`;
      const ms = markup(src, null);
      expect(ms.some((m) => m.kind === "line")).toBe(false);
      expect(text(src, ms, "fade")).toEqual([line]);
    }
    const src = "apple\n- ";
    expect(markup(src, null).some((m) => m.kind === "line")).toBe(false);
    expect(bullets(src)).toEqual([{ from: 6, to: 8, depth: 0 }]);
  });

  test("bullets draw as bullets; numbers, task boxes and a lone dash stay", () => {
    const src = "Intro\n- one\n  - nested\n1. first\n- [ ] task\n-";
    expect(bullets(src).map((b) => [src.slice(b.from, b.to), b.depth])).toEqual([
      ["- ", 0],
      ["  - ", 1],
      ["- ", 0],
    ]);
    expect(text(src, markup(src, all(src)), "fade")).toEqual(["1.", "[ ]", "-"]);
    expect(bullets("- one\n  - two\n  - ").at(-1)?.depth).toBe(1);
  });

  test("plain text has no markup", () => {
    expect(markup("苹果 píng guǒ", null)).toEqual([]);
  });
});
