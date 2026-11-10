import { createContext, createStore } from "solid-js";
import { type Api, ApiError, api, type CardQuery, type Session } from "./api.ts";
import { DEFAULT_VIEW, parseSort, type SortState, sortParam } from "./routes.ts";
import type { CardView, DeckSummary, View } from "./types.ts";

// What the screens show. Where you are (the screen, deck, search, sort, view and any open sheet) is the URL's
// (routes.ts); this holds the data for it, from the server, refetched after every write and whenever the page comes
// back, so other devices' changes show up. Nothing here decides scheduling or order.

type AppState = {
  session: Session | null;
  decks: DeckSummary[];
  loaded: boolean;
  cards: CardView[];
  /** The deck `cards` came from, and the exact query, so a deck's page knows when they're its own and current. */
  cardsFor: string | null;
  cardsKey: string | null;
  drawer: boolean;
  notice: string | null;
  lastSync: number | null;
};

/** A deck's view and sort as last picked, so its sidebar link opens it that way. */
export type DeckPrefs = { view: View; sort: Pick<SortState, "by" | "desc"> };

const SESSION = "furiten.session";

const stored = <T>(key: string): T | null => {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? null : (JSON.parse(raw) as T);
  } catch {
    return null;
  }
};
const store = (key: string, value: unknown) =>
  value === null ? localStorage.removeItem(key) : localStorage.setItem(key, JSON.stringify(value));

const queryKey = (q: CardQuery) => JSON.stringify([q.deck_id, q.q ?? "", q.tag ?? "", q.sort, !!q.desc, q.seed ?? null]);

export function createApp() {
  // Store writes land on the next flush, so code that writes and then fetches reads these plain copies instead.
  let current = stored<Session>(SESSION);
  let shown: CardQuery | null = null;
  const [state, setState] = createStore<AppState>({
    session: current,
    decks: [],
    loaded: false,
    cards: [],
    cardsFor: null,
    cardsKey: null,
    drawer: false,
    notice: null,
    lastSync: null,
  });
  const [prefs, setPrefs] = createStore<Record<string, DeckPrefs>>({});

  const client = (): Api | null => (current ? api(current) : null);

  const fail = (e: unknown) => {
    if (e instanceof ApiError && e.code === "unauthorized") {
      signOut();
      setState((s) => {
        s.notice = "That account’s token was refused. Sign in again.";
      });
      return;
    }
    setState((s) => {
      s.notice = e instanceof Error ? e.message : String(e);
    });
  };

  async function refreshDecks() {
    const c = client();
    if (!c) return;
    try {
      const decks = await c.decks();
      setState((s) => {
        s.decks = decks;
        s.loaded = true;
        s.lastSync = Date.now();
      });
    } catch (e) {
      fail(e);
    }
  }

  /** Shows the cards for `q`, a deck page's query from its URL; `null` when no deck's page is open. */
  async function showCards(q: CardQuery | null) {
    shown = q;
    if (!q) return;
    if (state.cardsFor !== q.deck_id) {
      setState((s) => {
        s.cards = [];
        s.cardsFor = null;
        s.cardsKey = null;
      });
    }
    await fetchCards(q);
  }

  async function fetchCards(q: CardQuery) {
    const c = client();
    if (!c) return;
    const key = queryKey(q);
    try {
      const cards = await c.cards(q);
      // A slower, older request mustn't overwrite a newer one.
      if (!shown || queryKey(shown) !== key) return;
      setState((s) => {
        s.cards = cards;
        s.cardsFor = q.deck_id;
        s.cardsKey = key;
      });
    } catch (e) {
      fail(e);
    }
  }

  /** Whether the cards on hand are the ones for `q`. */
  const cardsAre = (q: CardQuery) => state.cardsKey === queryKey(q);

  async function refresh() {
    await Promise.all([refreshDecks(), shown ? fetchCards(shown) : undefined]);
  }

  /** Runs a write, then refetches everything it may have changed. */
  async function write<T>(f: (c: Api) => Promise<T>): Promise<T | undefined> {
    const c = client();
    if (!c) return undefined;
    try {
      const result = await f(c);
      await refresh();
      return result;
    } catch (e) {
      fail(e);
      return undefined;
    }
  }

  const prefsOf = (deck: string): DeckPrefs =>
    prefs[deck] ?? {
      view: stored<View>(`furiten.view.${deck}`) ?? DEFAULT_VIEW,
      sort: parseSort(stored<string>(`furiten.sort.${deck}`) ?? undefined),
    };

  /** Remembers a deck's view and sort as they're picked. */
  function remember(deck: string, p: DeckPrefs) {
    const was = prefsOf(deck);
    if (was.view === p.view && was.sort.by === p.sort.by && was.sort.desc === p.sort.desc) return;
    setPrefs((s) => {
      s[deck] = { view: p.view, sort: { by: p.sort.by, desc: p.sort.desc } };
    });
    store(`furiten.view.${deck}`, p.view);
    store(`furiten.sort.${deck}`, sortParam(p.sort) ?? "created");
  }

  // A desktop pushes changes as server-sent events; Furiten Cloud doesn't, so there the 30 s poll does the job.
  let events: EventSource | null = null;
  function listen(session: Session | null) {
    events?.close();
    events = null;
    if (!session || !isDesktop(session)) return;
    events = new EventSource(`${session.server}/v1/events?token=${encodeURIComponent(session.token)}`);
    events.onmessage = () => void refresh();
  }

  function signIn(session: Session) {
    store(SESSION, session);
    setState((s) => {
      s.session = session;
      s.notice = null;
      s.loaded = false;
    });
    current = session;
    listen(session);
    void refresh();
  }

  function signOut() {
    store(SESSION, null);
    current = null;
    shown = null;
    listen(null);
    setState((s) => {
      s.session = null;
      s.decks = [];
      s.cards = [];
      s.cardsFor = null;
      s.cardsKey = null;
      s.loaded = false;
    });
  }

  listen(current);

  return {
    state,
    setState,
    client,
    refresh,
    refreshDecks,
    showCards,
    cardsAre,
    write,
    fail,
    prefsOf,
    remember,
    signIn,
    signOut,
  };
}

export type App = ReturnType<typeof createApp>;
export const AppContext = createContext<App>();

/** A desktop's web access token (`rw_`), as opposed to a Furiten Cloud account's (`rc_`). */
export const isDesktop = (session: Session) => session.token.startsWith("rw_");

/** Reads a desktop's sign-in link (`http://host:8787/#token=rw_…`) into a session. */
export function sessionFromLink(link: string): Session | null {
  try {
    const url = new URL(link.trim());
    const token = new URLSearchParams(url.hash.slice(1)).get("token");
    return token ? { server: url.origin === location.origin ? "" : url.origin, token } : null;
  } catch {
    return null;
  }
}

/** The deck tree: each deck followed by its subdecks, alphabetical at every level; orphans go to the top. */
export function deckTree(decks: DeckSummary[]): { deck: DeckSummary; depth: number }[] {
  const ids = new Set(decks.map((d) => d.id));
  const children = new Map<string | null, DeckSummary[]>();
  for (const d of decks) {
    const parent = d.parent_id && ids.has(d.parent_id) ? d.parent_id : null;
    children.set(parent, [...(children.get(parent) ?? []), d]);
  }
  const out: { deck: DeckSummary; depth: number }[] = [];
  const walk = (parent: string | null, depth: number) => {
    for (const d of (children.get(parent) ?? []).sort((a, b) => a.name.localeCompare(b.name))) {
      out.push({ deck: d, depth });
      walk(d.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

/** Cards to review across every deck: the top-level decks' counts, which include their subdecks'. */
export function inboxCount(decks: DeckSummary[]): number {
  const ids = new Set(decks.map((d) => d.id));
  return decks
    .filter((d) => !d.parent_id || !ids.has(d.parent_id))
    .reduce((n, d) => n + d.due + d.new, 0);
}

/** A deck's name with its parents': "Languages / Spanish". */
export function deckPath(decks: DeckSummary[], id: string): string {
  const names: string[] = [];
  let at: string | null = id;
  const seen = new Set<string>();
  while (at && !seen.has(at)) {
    seen.add(at);
    const d = decks.find((x) => x.id === at);
    if (!d) break;
    names.unshift(d.name);
    at = d.parent_id;
  }
  return names.join(" / ");
}
