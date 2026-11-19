// The benefit cards' rotation: each card that isn't held open gets a turn, opening for OPEN and resting for REST
// before the next one's turn, the way a deck's due cards come up one after another.

/** How long a card stays open on its turn. */
export const OPEN = 5_000;
/** How long the cards rest, all closed, between turns. */
export const REST = 1_200;
export const TURN = OPEN + REST;

export type Rotation = {
  /** The card whose turn it is, or null with every card held open. */
  turn: number | null;
  /** How far into the turn, from 0 to TURN. */
  elapsed: number;
};

/** The next card after `from` (wrapping, and possibly `from` itself) that isn't held open. */
export function nextTurn(held: readonly boolean[], from: number): number | null {
  for (let step = 1; step <= held.length; step++) {
    const i = (from + step) % held.length;
    if (!held[i]) return i;
  }
  return null;
}

/** The rotation `ms` later: turns that run out pass to the next card. */
export function advance(r: Rotation, held: readonly boolean[], ms: number): Rotation {
  let { turn, elapsed } = r;
  if (turn === null || held[turn]) {
    turn = nextTurn(held, turn ?? -1);
    elapsed = 0;
  }
  if (turn === null) return { turn, elapsed: 0 };
  elapsed += ms;
  while (elapsed >= TURN) {
    elapsed -= TURN;
    turn = nextTurn(held, turn) ?? turn;
  }
  return { turn, elapsed };
}

/** Whether the card is open on its turn. */
export const showing = (r: Rotation, i: number) => r.turn === i && r.elapsed < OPEN;

/** How long until card `i` opens on its turn: 0 while it's open, null if it's held open or there's no rotation. */
export function dueIn(r: Rotation, held: readonly boolean[], i: number): number | null {
  if (r.turn === null || held[i]) return null;
  if (showing(r, i)) return 0;
  let wait = TURN - r.elapsed;
  for (let at = nextTurn(held, r.turn); at !== null && at !== i; at = nextTurn(held, at)) wait += TURN;
  return wait;
}

/** "Due", "4 sec", "1 min": as the app labels when a card is due, in seconds. */
export function dueLabel(ms: number): string {
  if (ms <= 0) return "Due";
  const s = Math.ceil(ms / 1000);
  return s < 60 ? `${s} sec` : `${Math.round(s / 60)} min`;
}
