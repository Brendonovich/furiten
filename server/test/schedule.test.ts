import { beforeAll, expect, test } from "bun:test";
import { makeScheduler } from "../src/srs.ts";
import { account, baseUrl, ulid } from "./harness.ts";

beforeAll(() => baseUrl(), 120_000);

const MINUTE = 60_000;
const DAY = 86_400_000;
// FSRS-6 default w[2], w[4] and w[5]: the initial stability after Good, and the initial difficulty's terms.
const [W2, W4, W5] = [2.3065, 6.4133, 0.8334];

test("Remembered on a new card through the API schedules exactly like crates/srs", async () => {
  const a = await account();
  const deck = (await a.post("/v1/decks", { name: "D" })).body.id;
  const card = (await a.post("/v1/cards", { deck_id: deck, content: "q\n---\na" })).body;
  const reviewed = (await a.post(`/v1/cards/${card.id}/reviews`, { remembered: true })).body;
  const t0 = reviewed.schedule.last_review;
  // Good on a new card: learning, back in 10 minutes.
  expect(reviewed.schedule).toMatchObject({ state: "learning", due: t0 + 10 * MINUTE, reps: 1, lapses: 0 });
  expect(reviewed.schedule.stability).toBe(W2);
  expect(reviewed.schedule.difficulty).toBeCloseTo(W4 - Math.exp(W5 * 2) + 1, 12);
});

test("synced reviews fold in (reviewed_at, id) order, matching the wasm scheduler run locally", async () => {
  const scheduler = makeScheduler(
    new WebAssembly.Module(await Bun.file(new URL("../build/furiten_srs.wasm", import.meta.url)).arrayBuffer()),
  );
  const a = await account();
  const t0 = 1_800_000_000_000;
  const deck = { id: ulid(t0), name: "D", parent_id: null, created_at: t0, updated_at: t0, deleted: false };
  const card = { id: ulid(t0), deck_id: deck.id, content: "q", tags: [], created_at: t0, updated_at: t0, deleted: false };
  const reviews = [
    { at: t0, rating: 3 },
    { at: t0 + 10 * MINUTE, rating: 3 },
    { at: t0 + 3 * DAY, rating: 2 },
    { at: t0 + 20 * DAY, rating: 1 },
  ].map((r) => ({ id: ulid(r.at), card_id: card.id, rating: r.rating, reviewed_at: r.at, duration_ms: null }));

  // Sent out of order, and the card after its reviews: the server must still fold in time order.
  await a.post("/v1/sync", {
    cursor: null,
    changes: [
      { kind: "review", value: reviews[2] },
      { kind: "review", value: reviews[0] },
      { kind: "deck", value: deck },
    ],
  });
  await a.post("/v1/sync", {
    cursor: null,
    changes: [
      { kind: "card", value: card },
      { kind: "review", value: reviews[3] },
      { kind: "review", value: reviews[1] },
    ],
  });

  const expected = scheduler.fold(
    t0,
    reviews.map((r) => ({ reviewed_at: r.reviewed_at, rating: r.rating })),
  );
  const view = (await a.get(`/v1/cards/${card.id}`)).body;
  expect(view.schedule).toEqual(expected);
  expect(view.schedule).toMatchObject({ state: "relearning", reps: 4, lapses: 1, last_review: t0 + 20 * DAY });
  expect(view.schedule.due).toBe(t0 + 20 * DAY + 10 * MINUTE);
});
