import { beforeAll, expect, test } from "bun:test";
import { type Account, account, baseUrl, ulid } from "./harness.ts";

beforeAll(() => baseUrl(), 120_000);

type Change = { kind: "deck" | "card" | "review"; value: any };

/** A simulated device: its own store, an outbox and a cursor, merging with the contract's rules. */
class Device {
  readonly entities = new Map<string, Change>();
  outbox: Change[] = [];
  cursor: number | null = null;

  constructor(private readonly api: Account) {}

  write(kind: Change["kind"], value: any) {
    const change = { kind, value };
    this.entities.set(`${kind}:${value.id}`, change);
    this.outbox.push(change);
  }

  get(kind: Change["kind"], id: string) {
    return this.entities.get(`${kind}:${id}`)?.value;
  }

  private apply(change: Change) {
    const key = `${change.kind}:${change.value.id}`;
    const local = this.entities.get(key);
    if (change.kind === "review") {
      if (local === undefined) this.entities.set(key, change);
    } else if (local === undefined || change.value.updated_at > local.value.updated_at) {
      this.entities.set(key, change);
    }
  }

  async sync() {
    const res = await this.api.post<{ cursor: number; changes: Change[] }>("/v1/sync", {
      cursor: this.cursor,
      changes: this.outbox,
    });
    expect(res.status).toBe(200);
    res.body.changes.forEach((c) => this.apply(c));
    this.outbox = [];
    this.cursor = res.body.cursor;
    return res.body;
  }

  snapshot() {
    return Object.fromEntries([...this.entities].sort(([a], [b]) => a.localeCompare(b)));
  }
}

test("two devices with conflicting offline edits converge", async () => {
  const api = await account();
  const a = new Device(api);
  const b = new Device(api);
  const t = Date.now();

  const deck = { id: ulid(t), name: "Spanish", parent_id: null, created_at: t, updated_at: t, deleted: false };
  const card = { id: ulid(t), deck_id: deck.id, content: "hola\n---\nhello", tags: [], created_at: t, updated_at: t, deleted: false };
  a.write("deck", deck);
  a.write("card", card);
  expect((await a.sync()).changes).toEqual([]);
  const pulled = await b.sync();
  expect(pulled.changes).toEqual([
    { kind: "deck", value: deck },
    { kind: "card", value: card },
  ]);
  expect(pulled.cursor).toBe(a.cursor!);

  // Offline: both edit the card (B later), both rename the deck (A later), both review the card.
  a.write("card", { ...card, content: "hola\n---\nhi", updated_at: t + 100 });
  b.write("card", { ...card, content: "hola\n---\nhello (greeting)", updated_at: t + 200 });
  a.write("deck", { ...deck, name: "Español", updated_at: t + 150 });
  b.write("deck", { ...deck, name: "Spanish 1", updated_at: t + 50 });
  const reviewA = { id: ulid(t + 1000), card_id: card.id, rating: 3, reviewed_at: t + 1000, duration_ms: 900 };
  const reviewB = { id: ulid(t + 2000), card_id: card.id, rating: 4, reviewed_at: t + 2000, duration_ms: null };
  a.write("review", reviewA);
  b.write("review", reviewB);

  // A syncs first: everything it sent is accepted and nothing else is new.
  expect((await a.sync()).changes).toEqual([]);

  // B's card edit wins; its deck rename loses, so it gets A's deck back, plus A's review. Its own writes aren't echoed.
  const fromB = await b.sync();
  expect(fromB.changes).toEqual([
    { kind: "deck", value: { ...deck, name: "Español", updated_at: t + 150 } },
    { kind: "review", value: reviewA },
  ]);

  // A pulls B's card edit and review, not its own deck rename.
  const fromA = await a.sync();
  expect(fromA.changes).toEqual([
    { kind: "card", value: { ...card, content: "hola\n---\nhello (greeting)", updated_at: t + 200 } },
    { kind: "review", value: reviewB },
  ]);

  expect(a.snapshot()).toEqual(b.snapshot());
  expect(a.cursor).toBe(fromA.cursor);
  expect((await b.sync()).changes).toEqual([]);
  expect(b.cursor).toBe(a.cursor!);

  // The server agrees, and its schedule folds both devices' reviews.
  const view = (await api.get(`/v1/cards/${card.id}`)).body;
  expect(view.content).toBe("hola\n---\nhello (greeting)");
  expect(view.schedule).toMatchObject({ reps: 2, last_review: t + 2000 });
  expect((await api.get("/v1/decks")).body.decks[0].name).toBe("Español");
});

test("resending is harmless and a lost write is answered with the stored value", async () => {
  const api = await account();
  const t = Date.now();
  const deck = { id: ulid(t), name: "New", parent_id: null, created_at: t, updated_at: t + 10, deleted: false };
  const review = { id: ulid(t), card_id: ulid(t), rating: 2, reviewed_at: t, duration_ms: null };
  const request = { cursor: null, changes: [{ kind: "deck", value: deck }, { kind: "review", value: review }] };

  const first = (await api.post("/v1/sync", request)).body;
  expect(first.changes).toEqual([]);
  const again = (await api.post("/v1/sync", { ...request, cursor: first.cursor })).body;
  expect(again).toEqual({ cursor: first.cursor, changes: [] });

  // A stale edit from a device that has already seen the newer value still gets the newer value back.
  const stale = { ...deck, name: "Old", updated_at: t };
  const lost = (await api.post("/v1/sync", { cursor: first.cursor, changes: [{ kind: "deck", value: stale }] })).body;
  expect(lost).toEqual({ cursor: first.cursor, changes: [{ kind: "deck", value: deck }] });
});

test("API writes and deletes reach devices through sync", async () => {
  const api = await account();
  const device = new Device(api);
  await device.sync();
  const deck = (await api.post("/v1/decks", { name: "From the API" })).body;
  const card = (await api.post("/v1/cards", { deck_id: deck.id, content: "q\n---\na", tags: ["#T"] })).body;
  await api.post(`/v1/cards/${card.id}/reviews`, { remembered: true });
  await api.delete(`/v1/decks/${deck.id}`);

  const pulled = await device.sync();
  expect(pulled.changes.map((c) => c.kind)).toEqual(["review", "deck", "card"]);
  expect(device.get("deck", deck.id)).toMatchObject({ name: "From the API", deleted: true });
  expect(device.get("card", card.id)).toMatchObject({ tags: ["t"], deleted: true });
  expect((await api.post("/v1/sync", { cursor: pulled.cursor, changes: [] })).body.changes).toEqual([]);
});

test("invalid changes are rejected with 400", async () => {
  const api = await account();
  const res = await api.post("/v1/sync", { cursor: null, changes: [{ kind: "deck", value: { id: "nope" } }] });
  expect(res.status).toBe(400);
  expect(res.body.error.code).toBe("invalid_params");
});
