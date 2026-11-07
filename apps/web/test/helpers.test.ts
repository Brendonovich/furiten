import { describe, expect, test } from "bun:test";
import { ago, counts, plain, until, untilShort } from "../src/format.ts";
import { joinSides, onReturn, pasteSides, splitSides } from "../src/sides.ts";
import { deckTree } from "../src/state.ts";
import type { DeckSummary } from "../src/types.ts";

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

describe("format, worded as on the desktop", () => {
  test("until", () => {
    expect(until(30_000, 0)).toBe("in under a minute");
    expect(until(MIN, 0)).toBe("in 1 minute");
    expect(until(10 * MIN, 0)).toBe("in 10 minutes");
    expect(until(3 * HOUR, 0)).toBe("in 3 hours");
    expect(until(DAY, 0)).toBe("tomorrow");
    expect(until(4 * DAY, 0)).toBe("in 4 days");
    expect(until(90 * DAY, 0)).toBe("in 3 months");
  });

  test("untilShort, ago and counts", () => {
    expect(untilShort(12 * MIN, 0)).toBe("12 min");
    expect(untilShort(HOUR, 0)).toBe("1 hour");
    expect(untilShort(2 * DAY, 0)).toBe("2 days");
    expect(ago(0, 30_000)).toBe("just now");
    expect(ago(0, 2 * HOUR)).toBe("2 h ago");
    expect(counts(0, 0)).toBe("Nothing due");
    expect(counts(3, 5)).toBe("3 due · 5 new");
    expect(counts(0, 5)).toBe("5 new");
  });

  test("plain", () => {
    expect(plain("# **Bold** and `code`\n- item")).toBe("Bold and code item");
  });
});

describe("sides", () => {
  test("split and join round-trip, dropping empty sides", () => {
    expect(splitSides("a\n---\nb\n---\nc")).toEqual(["a", "b", "c"]);
    expect(splitSides("only")).toEqual(["only"]);
    expect(joinSides(["a", "", "b\n"])).toBe("a\n---\nb");
  });

  test("Return after a --- line splits the side", () => {
    expect(onReturn("front\n---", 9)).toEqual({ kind: "split", before: "front", after: "" });
    expect(onReturn("plain", 5)).toBeNull();
  });

  test("Return continues lists and an empty item ends one", () => {
    expect(onReturn("- one", 5)).toEqual({ kind: "edit", text: "- one\n- ", caret: 8 });
    expect(onReturn("1. one", 6)).toEqual({ kind: "edit", text: "1. one\n2. ", caret: 10 });
    expect(onReturn("- [x] done", 10)).toEqual({ kind: "edit", text: "- [x] done\n- [ ] ", caret: 17 });
    expect(onReturn("- one\n- ", 8)).toEqual({ kind: "edit", text: "- one\n", caret: 6 });
  });

  test("pasting splits on --- lines", () => {
    expect(pasteSides("", 0, 0, "q\n---\na")).toEqual(["q", "a"]);
    expect(pasteSides("x", 1, 1, "y")).toBeNull();
  });
});

test("the deck tree nests and sorts, with orphans at the top", () => {
  const deck = (id: string, name: string, parent_id: string | null): DeckSummary => ({
    id,
    name,
    parent_id,
    created_at: 0,
    updated_at: 0,
    deleted: false,
    cards: 0,
    due: 0,
    new: 0,
  });
  const tree = deckTree([deck("b", "Beta", null), deck("c", "Child", "a"), deck("a", "Alpha", null), deck("o", "Orphan", "gone")]);
  expect(tree.map((t) => [t.deck.name, t.depth])).toEqual([
    ["Alpha", 0],
    ["Child", 1],
    ["Beta", 0],
    ["Orphan", 0],
  ]);
});
