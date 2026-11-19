import { beforeAll, describe, expect, test } from "bun:test";
import { type Account, account, baseUrl, request, ulid } from "./harness.ts";

beforeAll(() => baseUrl(), 120_000);

describe("health and accounts", () => {
  test("GET /health needs no auth", async () => {
    const res = await request("GET", "/health");
    expect(res.status).toBe(200);
    expect(res.body).toEqual({ ok: true, version: expect.any(String) });
  });

  test("POST /v1/accounts returns a token for the user", async () => {
    const res = await request("POST", "/v1/accounts");
    expect(res.status).toBe(200);
    expect(res.body.user_id).toMatch(/^[0-9A-HJKMNP-TV-Z]{26}$/);
    expect(res.body.token).toMatch(new RegExp(`^rc_${res.body.user_id}_[0-9A-HJKMNP-TV-Z]{32}$`));
  });

  test("requests without a valid token are 401", async () => {
    const a = await account();
    const wrongSecret = a.token.slice(0, -1) + (a.token.endsWith("A") ? "B" : "A");
    for (const token of [undefined, "nope", wrongSecret, `rc_${a.userId.replace(/.$/, "0")}_${"A".repeat(32)}`]) {
      const res = await request("GET", "/v1/decks", { token });
      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe("unauthorized");
    }
    expect((await request("POST", "/mcp", { token: wrongSecret, body: {} })).status).toBe(401);
  });

  test("unknown routes are 404 with the error shape", async () => {
    const a = await account();
    for (const res of [await a.get("/v1/nope"), await request("GET", "/nope")]) {
      expect(res.status).toBe(404);
      expect(res.body.error).toEqual({ code: "not_found", message: expect.any(String) });
    }
  });
});

describe("decks", () => {
  let a: Account;
  beforeAll(async () => {
    a = await account();
  });

  test("create, nest, rename, list with counts, delete", async () => {
    const parent = await a.post("/v1/decks", { name: "  Languages " });
    expect(parent.status).toBe(200);
    expect(parent.body).toMatchObject({ name: "Languages", parent_id: null, deleted: false });
    const child = await a.post("/v1/decks", { name: "Spanish", parent_id: parent.body.id });
    expect(child.body.parent_id).toBe(parent.body.id);

    await a.post("/v1/cards", { deck_id: child.body.id, content: "hola\n---\nhello" });
    const renamed = await a.patch(`/v1/decks/${parent.body.id}`, { name: "Lang" });
    expect(renamed.body.name).toBe("Lang");
    expect(renamed.body.updated_at).toBeGreaterThan(parent.body.updated_at);

    const list = await a.get("/v1/decks");
    const byId = Object.fromEntries(list.body.decks.map((d: any) => [d.id, d]));
    // Counts include subdecks.
    expect(byId[parent.body.id]).toMatchObject({ name: "Lang", cards: 1, due: 0, new: 1 });
    expect(byId[child.body.id]).toMatchObject({ cards: 1, new: 1 });

    expect((await a.delete(`/v1/decks/${parent.body.id}`)).body).toEqual({ ok: true });
    expect((await a.get("/v1/decks")).body.decks).toEqual([]);
    expect((await a.get("/v1/cards")).body.cards).toEqual([]);
  });

  test("validation and missing decks", async () => {
    expect((await a.post("/v1/decks", { name: " " })).body.error.code).toBe("invalid_params");
    const missing = await a.post("/v1/decks", { name: "x", parent_id: "01M683DAPX00AV4285VSMKDMEC" });
    expect(missing.status).toBe(404);
    expect(missing.body.error.code).toBe("not_found");
    const bad = await a.post("/v1/decks", { nope: 1 });
    expect(bad.status).toBe(400);
    expect(bad.body.error.code).toBe("invalid_params");

    const top = await a.post("/v1/decks", { name: "Top" });
    const sub = await a.post("/v1/decks", { name: "Sub", parent_id: top.body.id });
    const cycle = await a.patch(`/v1/decks/${top.body.id}`, { parent_id: sub.body.id });
    expect(cycle.status).toBe(400);
    const moved = await a.patch(`/v1/decks/${sub.body.id}`, { parent_id: null });
    expect(moved.body.parent_id).toBeNull();
  });
});

describe("cards", () => {
  let a: Account;
  let deck: string;
  let other: string;
  beforeAll(async () => {
    a = await account();
    deck = (await a.post("/v1/decks", { name: "Main" })).body.id;
    other = (await a.post("/v1/decks", { name: "Other" })).body.id;
  });

  test("create one, with sides and normalized tags", async () => {
    const res = await a.post("/v1/cards", {
      deck_id: deck,
      content: "What is the capital of France?\n---\n\nParis\n",
      tags: [" #Geo", "geo", "Europe"],
    });
    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      deck_id: deck,
      tags: ["europe", "geo"],
      sides: ["What is the capital of France?", "Paris"],
      deleted: false,
      schedule: { state: "new", due: res.body.created_at, reps: 0, lapses: 0, last_review: null },
    });
    expect((await a.get(`/v1/cards/${res.body.id}`)).body).toEqual(res.body);
  });

  test("batch create is all or nothing", async () => {
    const res = await a.post("/v1/cards", {
      cards: [
        { deck_id: other, content: "one" },
        { deck_id: other, content: "two\n---\n2" },
      ],
    });
    expect(res.body.cards.map((c: any) => c.sides)).toEqual([["one"], ["two", "2"]]);
    expect(res.body.cards[1].created_at).toBeGreaterThan(res.body.cards[0].created_at);

    const failed = await a.post("/v1/cards", {
      cards: [
        { deck_id: other, content: "three" },
        { deck_id: "01M683DAPX00AV4285VSMKDMEC", content: "four" },
      ],
    });
    expect(failed.status).toBe(404);
    expect((await a.get(`/v1/cards?deck_id=${other}`)).body.cards).toHaveLength(2);
  });

  test("query by deck, text, tag, due, limit and offset", async () => {
    const ids = (res: any) => res.body.cards.map((c: any) => c.content);
    expect(ids(await a.get(`/v1/cards?deck_id=${other}`))).toEqual(["one", "two\n---\n2"]);
    expect(ids(await a.get("/v1/cards?q=CAPITAL"))).toEqual(["What is the capital of France?\n---\n\nParis\n"]);
    expect(ids(await a.get("/v1/cards?tag=%23GEO"))).toHaveLength(1);
    expect(ids(await a.get("/v1/cards?due=true"))).toHaveLength(3);
    expect(ids(await a.get("/v1/cards?limit=1&offset=1"))).toEqual(["one"]);
    expect((await a.get("/v1/cards?limit=-1")).status).toBe(400);
  });

  test("sort, reverse and shuffle", async () => {
    const d = (await a.post("/v1/decks", { name: "Sorted" })).body.id;
    const made = (
      await a.post("/v1/cards", {
        cards: ["Card 10", "card 9", "**Apple**"].map((content) => ({ deck_id: d, content })),
      })
    ).body.cards;
    await a.post(`/v1/cards/${made[1].id}/reviews`, { remembered: true });
    const fronts = async (query: string) =>
      (await a.get(`/v1/cards?deck_id=${d}&${query}`)).body.cards.map((c: any) => c.sides[0]);
    expect(await fronts("desc=true")).toEqual(["**Apple**", "card 9", "Card 10"]);
    expect(await fronts("sort=name")).toEqual(["**Apple**", "card 9", "Card 10"]);
    expect(await fronts("sort=name&limit=1&offset=1")).toEqual(["card 9"]);
    expect(await fronts("sort=reviewed&desc=true")).toEqual(["card 9", "Card 10", "**Apple**"]);
    expect(await fronts("sort=random&seed=3")).toEqual(await fronts("sort=random&seed=3"));
    expect((await a.get(`/v1/cards?sort=shuffled`)).status).toBe(400);
  });

  test("update and delete", async () => {
    const card = (await a.post("/v1/cards", { deck_id: deck, content: "old" })).body;
    const updated = await a.patch(`/v1/cards/${card.id}`, { content: "new\n---\nback", tags: ["X"], deck_id: other });
    expect(updated.body).toMatchObject({ content: "new\n---\nback", sides: ["new", "back"], tags: ["x"], deck_id: other });
    expect(updated.body.updated_at).toBeGreaterThan(card.updated_at);
    expect((await a.delete(`/v1/cards/${card.id}`)).body).toEqual({ ok: true });
    expect((await a.get(`/v1/cards/${card.id}`)).status).toBe(404);
    expect((await a.patch(`/v1/cards/${card.id}`, { content: "x" })).status).toBe(404);
  });

  test("tags, the deck's first and then the most recently used", async () => {
    const b = await account();
    const one = (await b.post("/v1/decks", { name: "One" })).body.id;
    const two = (await b.post("/v1/decks", { name: "Two" })).body.id;
    await b.post("/v1/cards", { deck_id: one, content: "a", tags: ["old", "shared"] });
    await b.post("/v1/cards", { deck_id: two, content: "b", tags: ["new", "shared"] });
    const tags = async (query = "") => (await b.get(`/v1/tags${query}`)).body.tags.map((t: any) => t.tag);
    expect(await tags()).toEqual(["new", "shared", "old"]);
    expect(await tags(`?deck_id=${one}`)).toEqual(["shared", "old", "new"]);
    expect((await b.get("/v1/tags")).body.tags[1]).toEqual({ tag: "shared", cards: 2, last_used: expect.any(Number) });
  });
});

describe("reviewing", () => {
  test("next review, answering, stats", async () => {
    const a = await account();
    const deck = (await a.post("/v1/decks", { name: "D" })).body.id;
    const [first, second] = (
      await a.post("/v1/cards", {
        cards: [
          { deck_id: deck, content: "first" },
          { deck_id: deck, content: "second" },
        ],
      })
    ).body.cards;

    const next = await a.get(`/v1/review/next?deck_id=${deck}`);
    expect(next.body.card.id).toBe(first.id);
    expect(next.body.remaining).toEqual({ due: 0, new: 2 });

    const answered = await a.post(`/v1/cards/${first.id}/reviews`, { remembered: true, duration_ms: 1200 });
    expect(answered.status).toBe(200);
    expect(answered.body.schedule).toMatchObject({ state: "review", reps: 1 });
    expect(answered.body.schedule.due - answered.body.schedule.last_review).toBe(2 * 86_400_000);

    const after = await a.get("/v1/review/next");
    expect(after.body.card.id).toBe(second.id);
    expect(after.body.remaining).toEqual({ due: 0, new: 1 });

    expect((await a.post(`/v1/cards/${first.id}/reviews`, { rating: 3 })).status).toBe(400);
    expect((await a.post(`/v1/cards/${first.id}/reviews`, { remembered: "yes" })).status).toBe(400);
    expect((await a.post(`/v1/cards/${first.id}/reviews`, { remembered: true, duration_ms: -1 })).status).toBe(400);
    expect((await a.post(`/v1/cards/01M683DAPX00AV4285VSMKDMEC/reviews`, { remembered: true })).status).toBe(404);

    const stats = await a.get(`/v1/stats?deck_id=${deck}`);
    expect(stats.body).toEqual({ cards: 2, due: 0, new: 1, reviews_today: 1, retention_30d: null });
  });

  test("a forgotten card comes back at once, after the new cards, until it's remembered", async () => {
    const a = await account();
    const deck = (await a.post("/v1/decks", { name: "D" })).body.id;
    const [first, second] = (
      await a.post("/v1/cards", {
        cards: [
          { deck_id: deck, content: "first" },
          { deck_id: deck, content: "second" },
        ],
      })
    ).body.cards;
    const next = async () => (await a.get(`/v1/review/next?deck_id=${deck}`)).body;

    const forgot = (await a.post(`/v1/cards/${first.id}/reviews`, { remembered: false })).body;
    expect(forgot.schedule).toMatchObject({ state: "learning", due: forgot.schedule.last_review });
    expect(await next()).toMatchObject({ card: { id: second.id }, remaining: { due: 1, new: 1 } });

    await a.post(`/v1/cards/${second.id}/reviews`, { remembered: true });
    expect(await next()).toMatchObject({ card: { id: first.id }, remaining: { due: 1, new: 0 } });
    await a.post(`/v1/cards/${first.id}/reviews`, { remembered: false });
    expect((await next()).card.id).toBe(first.id);
    await a.post(`/v1/cards/${first.id}/reviews`, { remembered: true });
    expect(await next()).toMatchObject({ card: null, remaining: { due: 0, new: 0 } });
  });
});

describe("settings and the review day", () => {
  const HOUR = 3_600_000;
  /** The request's day, in a zone the caller names (UTC unless it does). */
  const inZone = (a: Account, path: string, zone: string) =>
    request("GET", path, { token: a.token, headers: { "x-furiten-timezone": zone } });

  test("settings are read and changed, and hours outside the day are rejected", async () => {
    const a = await account();
    expect((await a.get("/v1/settings")).body).toEqual({ rollover_hour: 4 });
    expect((await a.patch("/v1/settings", { rollover_hour: 7 })).body).toEqual({ rollover_hour: 7 });
    expect((await a.get("/v1/settings")).body).toEqual({ rollover_hour: 7 });
    expect((await a.patch("/v1/settings", {})).body).toEqual({ rollover_hour: 7 });
    for (const bad of [24, -1, 7.5, "seven"]) {
      expect((await a.patch("/v1/settings", { rollover_hour: bad })).status).toBe(400);
    }
  });

  test("a card due later today is due from the start of the day, wherever the rollover and the caller put it", async () => {
    const a = await account();
    const deck = (await a.post("/v1/decks", { name: "D" })).body.id;
    const card = (await a.post("/v1/cards", { deck_id: deck, content: "q" })).body;

    // A review 25 hours ago, pushed the way a device syncs one, makes the card due 23 hours from now: later
    // today, whatever the clock says.
    const now = Date.now();
    const reviewedAt = now - 25 * HOUR;
    await a.post("/v1/sync", {
      cursor: null,
      changes: [
        { kind: "review", value: { id: ulid(reviewedAt), card_id: card.id, rating: 3, reviewed_at: reviewedAt, duration_ms: null } },
      ],
    });
    const check = async (zone: string) => (await inZone(a, `/v1/review/next?deck_id=${deck}`, zone)).body.remaining;
    const remaining = (due: number) => ({ due, new: 0 });

    // In UTC, a rollover at the current hour just started the day: it ends a whole day out, so the card is due
    // now though its due time is hours away.
    const hour = new Date(now).getUTCHours();
    await a.patch("/v1/settings", { rollover_hour: hour });
    expect(await check("UTC")).toEqual(remaining(1));

    // Rolled to the far side of the clock, the day ends sooner than the card's due time: not today's.
    await a.patch("/v1/settings", { rollover_hour: (hour + 12) % 24 });
    expect(await check("UTC")).toEqual(remaining(0));

    // The day follows the caller's zone too: in UTC+14 the same hour is two o'clock in the afternoon, and the
    // day started a minute ago.
    await a.patch("/v1/settings", { rollover_hour: (hour + 14) % 24 });
    expect(await check("Etc/GMT-14")).toEqual(remaining(1));
    expect(await check("UTC")).toEqual(remaining(0));

    // The cards list's due filter and the stats agree with the queue.
    await a.patch("/v1/settings", { rollover_hour: hour });
    expect((await inZone(a, `/v1/cards?due=true&deck_id=${deck}`, "UTC")).body.cards).toHaveLength(1);
    expect((await inZone(a, `/v1/stats?deck_id=${deck}`, "UTC")).body.due).toBe(1);
  });
});
