import type { CardQuery } from "./api.ts";
import type { Sort, View } from "./types.ts";

// The app's places and their state, as URLs. Every screen, overlay, filter and tab lives in the address, so reloads,
// the back button and shared links land where you were:
//
//   /                               Review: every due card from every deck
//   /settings                       Sync and account
//   /decks/:deck                    A deck's cards
//   /decks/:deck/review             Reviewing that deck, over its page
//   /decks/:deck/cram               Cramming the cards its page lists (with its search, tag and sort), over it
//   /tags/:tag                      Every card with a tag, from every deck
//   /tags/:tag/cram                 Cramming the cards its page lists (with its search and sort), over it
//
// A deck's page (and its review or cram) keeps the rest in the query: `q` (search), `tag` (only cards with it), `view` (grid, list, column), `sort`
// (`name`, `name-desc`…), `seed` (a random sort's order), `card` (the card open in column view), `edit` (the editor:
// a card's id, or `new`) and `actions` (the deck's menu, at a step). A tag's page keeps the same but `tag`, `actions`
// and `edit=new`. Defaults are left out.
//
// Search, through every deck's cards, is a layer over whichever of these is open rather than a place of its own:
// `find` (what's typed, empty until something is) opens it over the page, `cram=find` crams what it found, and `edit`
// edits one of its cards. Closing it drops them, back to the page as it was.

export type SortState = { by: Sort; desc: boolean; seed: number | null };
export type DeckStep = "menu" | "rename" | "subdeck" | "move" | "delete";
export type SignInMode = "desktop" | "cloud";

/** Raw search params, as the router gives them. */
export type Search = Partial<Record<string, string | string[]>>;

export const VIEWS: View[] = ["grid", "list", "column"];
export const SORT_KEYS: Sort[] = ["created", "updated", "name", "due", "reviewed", "interval", "retention", "random"];
const STEPS: DeckStep[] = ["menu", "rename", "subdeck", "move", "delete"];
/** Sorts that start newest-first when picked (apps/desktop/src/app.rs). */
const DESC_FIRST: Sort[] = ["updated", "reviewed"];

export const DEFAULT_VIEW: View = "grid";
export const DEFAULT_SORT: SortState = { by: "created", desc: false, seed: null };

export const paths = {
  inbox: () => "/",
  settings: () => "/settings",
  deck: (id: string, search = "") => `/decks/${encodeURIComponent(id)}${search}`,
  review: (id: string, search = "") => `/decks/${encodeURIComponent(id)}/review${search}`,
  deckCram: (id: string, search = "") => `/decks/${encodeURIComponent(id)}/cram${search}`,
  tag: (tag: string, search = "") => `/tags/${encodeURIComponent(tag)}${search}`,
  tagCram: (tag: string, search = "") => `/tags/${encodeURIComponent(tag)}/cram${search}`,
};

/** What keys a tag's page among the pages of cards (its cards, view and sort), apart from any deck's id. */
export const tagKey = (tag: string) => `#${tag}`;

/** What the search layer is searching for (empty while nothing's typed), or null when it's closed. */
export const readFind = (search: Search): string | null => one(search.find) ?? null;
/** Whether a cram of what the search layer found is open over it. */
export const readFindCram = (search: Search): boolean => readFind(search) !== null && one(search.cram) === "find";

/**
 * `url` (a path and query) with the search layer searching for `find`, and with `cram`, cramming what it found; with
 * `find` null, the layer closed, and what was open over it.
 */
export function withFind(url: string, find: string | null, cram = false): string {
  const [path = "", query = ""] = url.split("?");
  const q = new URLSearchParams(query);
  if (find === null) {
    q.delete("find");
    q.delete("cram");
    q.delete("edit");
  } else {
    q.set("find", find);
    if (cram) q.set("cram", "find");
    else q.delete("cram");
  }
  const s = q.toString();
  return s ? `${path}?${s}` : path;
}

/** The cards a deck's page lists for its URL: its search, tag filter and sort. A cram of the page asks for the same. */
export function deckCards(deck: string, search: Search): CardQuery {
  const sort = readSort(search);
  return {
    deck_id: deck,
    q: readText(search, "q").trim() || undefined,
    tag: readText(search, "tag") || undefined,
    sort: sort.by,
    desc: sort.desc,
    seed: sort.seed ?? undefined,
  };
}

/** The cards a tag's page lists for its URL: its search and sort. A cram of the page asks for the same. */
export function tagCards(tag: string, search: Search): CardQuery {
  const sort = readSort(search);
  return {
    tag,
    q: readText(search, "q").trim() || undefined,
    sort: sort.by,
    desc: sort.desc,
    seed: sort.seed ?? undefined,
  };
}

/** The cards the search layer finds for `q`, before it groups them by deck (state.ts, `byDeck`). */
export const searchCards = (q: string): CardQuery => ({ q, sort: "updated", desc: true });

const one = (v: string | string[] | undefined): string | undefined => (Array.isArray(v) ? v[0] : v);
const pick = <T extends string>(all: readonly T[], v: string | undefined): T | null =>
  v !== undefined && (all as readonly string[]).includes(v) ? (v as T) : null;

export const readView = (search: Search): View => pick(VIEWS, one(search.view)) ?? DEFAULT_VIEW;

/** `name-desc` into a sort; anything unknown is the default. */
export function parseSort(raw: string | undefined, seed?: string): SortState {
  const [key, dir] = (raw ?? "").split("-");
  const by = pick(SORT_KEYS, key);
  if (!by) return DEFAULT_SORT;
  const n = seed === undefined ? NaN : Number(seed);
  return { by, desc: dir === "desc", seed: by === "random" && Number.isInteger(n) && n >= 0 ? n : null };
}

export const readSort = (search: Search): SortState => parseSort(one(search.sort), one(search.seed));

/** The `sort` param for a sort: undefined for the default, so it's left out of the URL. */
export function sortParam(s: Pick<SortState, "by" | "desc">): string | undefined {
  if (s.by === DEFAULT_SORT.by && s.desc === DEFAULT_SORT.desc) return undefined;
  return s.desc ? `${s.by}-desc` : s.by;
}

export const newSeed = () => Math.floor(Math.random() * 0xffff_ffff);

/** Picking the current sort keeps its direction; picking another starts in its default direction. Random reshuffles. */
export function nextSort(current: SortState, by: Sort, desc?: boolean): SortState {
  return {
    by,
    desc: desc ?? (by === current.by ? current.desc : DESC_FIRST.includes(by)),
    seed: by === "random" ? newSeed() : null,
  };
}

/** The params that set a sort, clearing a seed it doesn't use. */
export const sortParams = (s: SortState) => ({ sort: sortParam(s), seed: s.seed ?? undefined });
export const viewParam = (v: View) => (v === DEFAULT_VIEW ? undefined : v);

export const readStep = (search: Search): DeckStep | null => pick(STEPS, one(search.actions));
export const readText = (search: Search, key: string): string => one(search[key]) ?? "";
export const readSignIn = (search: Search): SignInMode => (one(search.via) === "cloud" ? "cloud" : "desktop");

/** A query string from params, leaving out empty ones: `?view=list&sort=name`, or `""`. */
export function queryString(params: Record<string, string | number | undefined | null>): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== null && v !== "") q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : "";
}

/** A deck or tag page's query with only its lasting state (search, tag, view, sort), dropping overlays. */
export function deckQuery(search: Search): string {
  return queryString({
    q: readText(search, "q") || undefined,
    tag: readText(search, "tag") || undefined,
    view: one(search.view),
    sort: one(search.sort),
    seed: one(search.seed),
  });
}
