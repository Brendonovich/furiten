import { describe, expect, test } from "bun:test";
import { DEFAULT_SORT, deckCards, deckQuery, nextSort, parseSort, paths, queryString, readFind, readFindCram, readStep, readView, sortParam, tagCards, withFind } from "../src/routes.ts";

describe("routes", () => {
  test("paths encode deck ids", () => {
    expect(paths.deck("a b")).toBe("/decks/a%20b");
    expect(paths.review("x", "?view=list")).toBe("/decks/x/review?view=list");
    expect(paths.deckCram("a b", "?tag=x")).toBe("/decks/a%20b/cram?tag=x");
    expect(paths.tagCram("a/b")).toBe("/tags/a%2Fb/cram");
    expect(paths.tag("a/b", "?view=list")).toBe("/tags/a%2Fb?view=list");
  });

  test("search is a layer over any page", () => {
    expect(readFind({})).toBeNull();
    expect(readFind({ find: "" })).toBe("");
    expect(withFind("/decks/x?view=list", "")).toBe("/decks/x?view=list&find=");
    expect(withFind("/decks/x?view=list&find=", "a b")).toBe("/decks/x?view=list&find=a+b");
    const cram = withFind("/?find=cat", "cat", true);
    expect(cram).toBe("/?find=cat&cram=find");
    expect(readFindCram({ find: "cat", cram: "find" })).toBe(true);
    expect(readFindCram({ cram: "find" })).toBe(false);
    expect(withFind(cram, "cat")).toBe("/?find=cat");
    expect(withFind("/tags/t?find=cat&cram=find&edit=c1", null)).toBe("/tags/t");
  });

  test("a deck's cram lists what its page lists", () => {
    expect(deckCards("d", { q: " cat ", tag: "verbs", sort: "random", seed: "7", view: "list", edit: "new" })).toEqual({
      deck_id: "d",
      q: "cat",
      tag: "verbs",
      sort: "random",
      desc: false,
      seed: 7,
    });
  });

  test("a tag's cram lists what its page lists, which has no tag filter", () => {
    expect(tagCards("t", { q: " cat ", tag: "verbs", sort: "due-desc", view: "column", card: "c1" })).toEqual({
      tag: "t",
      q: "cat",
      sort: "due",
      desc: true,
      seed: undefined,
    });
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
