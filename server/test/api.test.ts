import { beforeAll, describe, expect, test } from "bun:test";
import { type Account, account, baseUrl, request } from "./harness.ts";

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

  test("update and delete", async () => {
    const card = (await a.post("/v1/cards", { deck_id: deck, content: "old" })).body;
    const updated = await a.patch(`/v1/cards/${card.id}`, { content: "new\n---\nback", tags: ["X"], deck_id: other });
    expect(updated.body).toMatchObject({ content: "new\n---\nback", sides: ["new", "back"], tags: ["x"], deck_id: other });
    expect(updated.body.updated_at).toBeGreaterThan(card.updated_at);
    expect((await a.delete(`/v1/cards/${card.id}`)).body).toEqual({ ok: true });
    expect((await a.get(`/v1/cards/${card.id}`)).status).toBe(404);
    expect((await a.patch(`/v1/cards/${card.id}`, { content: "x" })).status).toBe(404);
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

    const answered = await a.post(`/v1/cards/${first.id}/reviews`, { rating: 3, duration_ms: 1200 });
    expect(answered.status).toBe(200);
    expect(answered.body.schedule).toMatchObject({ state: "learning", reps: 1 });
    expect(answered.body.schedule.due - answered.body.schedule.last_review).toBe(10 * 60_000);

    const after = await a.get("/v1/review/next");
    expect(after.body.card.id).toBe(second.id);
    expect(after.body.remaining).toEqual({ due: 0, new: 1 });

    expect((await a.post(`/v1/cards/${first.id}/reviews`, { rating: 5 })).status).toBe(400);
    expect((await a.post(`/v1/cards/${first.id}/reviews`, { rating: 3, duration_ms: -1 })).status).toBe(400);
    expect((await a.post(`/v1/cards/01M683DAPX00AV4285VSMKDMEC/reviews`, { rating: 3 })).status).toBe(404);

    const stats = await a.get(`/v1/stats?deck_id=${deck}`);
    expect(stats.body).toEqual({ cards: 2, due: 0, new: 1, reviews_today: 1, retention_30d: null });
  });
});
