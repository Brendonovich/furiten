import type { Sort, View } from "./types.ts";

// The app's places and their state, as URLs. Every screen, overlay, filter and tab lives in the address, so reloads,
// the back button and shared links land where you were:
//
//   /                               Review: every due card from every deck
//   /settings                       Sync and account
//   /decks/:deck                    A deck's cards
//   /decks/:deck/review             Reviewing that deck, over its page
//
// A deck's page (and its review) keeps the rest in the query: `q` (search), `tag` (only cards with it), `view` (grid, list, column), `sort`
// (`name`, `name-desc`…), `seed` (a random sort's order), `card` (the card open in column view), `edit` (the editor:
// a card's id, or `new`) and `actions` (the deck's menu, at a step). Defaults are left out.

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
};

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

/** A deck page's query with only its lasting state (search, tag, view, sort), dropping overlays. */
export function deckQuery(search: Search): string {
  return queryString({
    q: readText(search, "q") || undefined,
    tag: readText(search, "tag") || undefined,
    view: one(search.view),
    sort: one(search.sort),
    seed: one(search.seed),
  });
}
