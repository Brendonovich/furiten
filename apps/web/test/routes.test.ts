import { describe, expect, test } from "bun:test";
import { DEFAULT_SORT, deckQuery, nextSort, parseSort, paths, queryString, readStep, readView, sortParam } from "../src/routes.ts";

describe("routes", () => {
  test("paths encode deck ids", () => {
    expect(paths.deck("a b")).toBe("/decks/a%20b");
    expect(paths.review("x", "?view=list")).toBe("/decks/x/review?view=list");
  });

  test("sort round-trips through its param, defaults left out", () => {
    expect(parseSort(undefined)).toEqual(DEFAULT_SORT);
    expect(parseSort("nonsense-desc")).toEqual(DEFAULT_SORT);
    expect(parseSort("name-desc")).toEqual({ by: "name", desc: true, seed: null });
    expect(parseSort("random", "42")).toEqual({ by: "random", desc: false, seed: 42 });
    expect(parseSort("name", "42").seed).toBeNull();
    expect(sortParam({ by: "created", desc: false })).toBeUndefined();
    expect(sortParam({ by: "created", desc: true })).toBe("created-desc");
    expect(sortParam(parseSort("due-desc"))).toBe("due-desc");
  });

  test("picking a sort", () => {
    const name = parseSort("name-desc");
    expect(nextSort(name, "name").desc).toBe(true);
    expect(nextSort(name, "updated").desc).toBe(true);
    expect(nextSort(name, "due").desc).toBe(false);
    expect(nextSort(name, "random").seed).toBeNumber();
  });

  test("view, step and queries", () => {
    expect(readView({})).toBe("grid");
    expect(readView({ view: "column" })).toBe("column");
    expect(readView({ view: "bogus" })).toBe("grid");
    expect(readStep({ actions: "move" })).toBe("move");
    expect(readStep({ actions: "x" })).toBeNull();
    expect(queryString({ a: "1", b: undefined, c: "" })).toBe("?a=1");
    expect(queryString({})).toBe("");
    expect(deckQuery({ q: "hi", view: "list", edit: "new", actions: "menu", card: "c1" })).toBe("?q=hi&view=list");
  });
});
