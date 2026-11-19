import { describe, expect, test } from "bun:test";
import { advance, dueIn, dueLabel, OPEN, showing, TURN, type Rotation } from "../src/schedule.ts";

const none = [false, false, false];

describe("rotation", () => {
  test("opens each card in turn, resting between them", () => {
    let r: Rotation = { turn: 0, elapsed: 0 };
    expect(showing(r, 0)).toBe(true);
    r = advance(r, none, OPEN);
    expect(showing(r, 0)).toBe(false);
    r = advance(r, none, TURN - OPEN);
    expect(r).toEqual({ turn: 1, elapsed: 0 });
    r = advance(r, none, 2 * TURN);
    expect(r.turn).toBe(0);
  });

  test("skips cards held open", () => {
    const held = [false, true, false];
    expect(advance({ turn: 0, elapsed: 0 }, held, TURN).turn).toBe(2);
    expect(advance({ turn: 1, elapsed: 0 }, held, 0)).toEqual({ turn: 2, elapsed: 0 });
  });

  test("stops with every card held open", () => {
    const held = [true, true, true];
    expect(advance({ turn: 0, elapsed: 500 }, held, TURN).turn).toBeNull();
    expect(dueIn({ turn: null, elapsed: 0 }, held, 0)).toBeNull();
  });

  test("counts down to each card's turn", () => {
    const r = { turn: 0, elapsed: 1_000 };
    expect(dueIn(r, none, 0)).toBe(0);
    expect(dueIn(r, none, 1)).toBe(TURN - 1_000);
    expect(dueIn(r, none, 2)).toBe(2 * TURN - 1_000);
    expect(dueIn(r, [false, true, false], 2)).toBe(TURN - 1_000);
    expect(dueIn(r, [false, true, false], 1)).toBeNull();
    // Resting after its turn, a card is last in line.
    expect(dueIn({ turn: 0, elapsed: OPEN }, none, 0)).toBe(3 * TURN - OPEN);
  });

  test("labels", () => {
    expect(dueLabel(0)).toBe("Due");
    expect(dueLabel(4_100)).toBe("5 sec");
    expect(dueLabel(90_000)).toBe("2 min");
  });
});
