// The HTTP API client (docs/contract.md, "HTTP API"). The app holds no domain logic: queues, schedules, sorting and
// tag ordering all come from the server.
import type { CardView, Deck, DeckSummary, NextReview, Sort, TagSummary } from "./types.ts";

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

export type Session = { server: string; token: string };

/** `""` is this page's own origin, which proxies to the Worker in dev and is the Worker in production. */
export const sameOrigin = "";

const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;

async function call<T>(session: Session, method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(session.server + path, {
    method,
    headers: {
      authorization: `Bearer ${session.token}`,
      "x-recall-timezone": timezone,
      ...(body === undefined ? {} : { "content-type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  }).catch(() => {
    throw new ApiError(0, "network", "Couldn’t reach the server");
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new ApiError(res.status, json?.error?.code ?? "internal", json?.error?.message ?? res.statusText);
  }
  return json as T;
}

const query = (params: Record<string, string | number | boolean | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== "") q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : "";
};

export type CardQuery = { deck_id: string; q?: string; tag?: string; sort: Sort; desc?: boolean; seed?: number };

export const api = (s: Session) => ({
  decks: () => call<{ decks: DeckSummary[] }>(s, "GET", "/v1/decks").then((r) => r.decks),
  createDeck: (name: string, parent_id: string | null) => call<Deck>(s, "POST", "/v1/decks", { name, parent_id }),
  updateDeck: (id: string, update: { name?: string; parent_id?: string | null }) =>
    call<Deck>(s, "PATCH", `/v1/decks/${id}`, update),
  deleteDeck: (id: string) => call<{ ok: true }>(s, "DELETE", `/v1/decks/${id}`),

  cards: (q: CardQuery) =>
    call<{ cards: CardView[] }>(
      s,
      "GET",
      `/v1/cards${query({ ...q, desc: q.desc || undefined, limit: 500 })}`,
    ).then((r) => r.cards),
  card: (id: string) => call<CardView>(s, "GET", `/v1/cards/${id}`),
  createCard: (card: { deck_id: string; content: string; tags: string[] }) =>
    call<CardView>(s, "POST", "/v1/cards", card),
  updateCard: (id: string, update: { content: string; tags: string[]; deck_id: string }) =>
    call<CardView>(s, "PATCH", `/v1/cards/${id}`, update),
  deleteCard: (id: string) => call<{ ok: true }>(s, "DELETE", `/v1/cards/${id}`),
  tags: (deck_id?: string) =>
    call<{ tags: TagSummary[] }>(s, "GET", `/v1/tags${query({ deck_id })}`).then((r) => r.tags),

  next: (deck_id?: string) => call<NextReview>(s, "GET", `/v1/review/next${query({ deck_id })}`),
  answer: (id: string, remembered: boolean, duration_ms: number) =>
    call<CardView>(s, "POST", `/v1/cards/${id}/reviews`, { remembered, duration_ms }),
});

export type Api = ReturnType<typeof api>;

/** Creates an account on `server`: the interim sign-in (docs/contract.md, "Accounts"). */
export async function createAccount(server: string): Promise<Session> {
  const res = await fetch(`${server}/v1/accounts`, { method: "POST" }).catch(() => {
    throw new ApiError(0, "network", "Couldn’t reach the server");
  });
  if (!res.ok) throw new ApiError(res.status, "internal", "Couldn’t create an account");
  const { token } = (await res.json()) as { token: string };
  return { server, token };
}

/** Checks a token by listing decks; throws `unauthorized` for a bad one. */
export async function checkSession(session: Session): Promise<void> {
  await api(session).decks();
}
