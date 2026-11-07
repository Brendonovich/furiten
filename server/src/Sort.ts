// The orders `GET /v1/cards` can return cards in (see Card order in docs/contract.md), as crates/core/src/sort.rs
// sorts them. Every key comes from the CardView alone.

import type { CardSort, CardView } from "./Model.ts";

type Key = number | string | null;

export function sortCards(cards: ReadonlyArray<CardView>, by: CardSort, desc: boolean, seed: number): CardView[] {
  const keyed = cards.map((card) => ({ card, key: key(card, by, seed) }));
  keyed.sort((a, b) => {
    const oldest = a.card.created_at - b.card.created_at || compare(a.card.id, b.card.id);
    if (a.key === null || b.key === null) {
      return a.key === b.key ? oldest : a.key === null ? 1 : -1;
    }
    const o =
      (typeof a.key === "string" ? naturalCompare(a.key, b.key as string) : compare(a.key, b.key as number)) || oldest;
    return desc ? -o : o;
  });
  return keyed.map((k) => k.card);
}

function key(c: CardView, by: CardSort, seed: number): Key {
  const s = c.schedule;
  switch (by) {
    case "created":
      return c.created_at;
    case "updated":
      return c.updated_at;
    case "name":
      return titleKey(c.sides[0] ?? "");
    case "due":
      return s.due;
    case "reviewed":
      return s.last_review;
    case "interval":
      return s.last_review === null ? null : s.due - s.last_review;
    case "retention":
      return s.reps === 0 ? null : Math.max(0, s.reps - s.lapses) / s.reps;
    case "random":
      return fnv1a(`${seed}:${c.id}`);
  }
}

/** What `name` sorts a card by: its front's first non-blank line without Markdown's marks, lowercased. */
export function titleKey(front: string): string {
  const line = front.split("\n").find((l) => l.trim() !== "") ?? "";
  return line
    .replace(/[#*_`~>]/g, "")
    .trim()
    .replace(/^-+/, "")
    .trim()
    .toLowerCase();
}

/** Text in code point order with runs of ASCII digits compared by their value. */
export function naturalCompare(a: string, b: string): number {
  const x = Array.from(a);
  const y = Array.from(b);
  let i = 0;
  let j = 0;
  while (i < x.length && j < y.length) {
    if (isDigit(x[i]!) && isDigit(y[j]!)) {
      const [ra, ni] = digits(x, i);
      const [rb, nj] = digits(y, j);
      const o = ra.length - rb.length || compare(ra, rb);
      if (o !== 0) return o;
      i = ni;
      j = nj;
      continue;
    }
    const o = x[i]!.codePointAt(0)! - y[j]!.codePointAt(0)!;
    if (o !== 0) return o;
    i++;
    j++;
  }
  return x.length - i - (y.length - j);
}

const isDigit = (c: string) => c >= "0" && c <= "9";

/** The digit run starting at `at` without its leading zeros, and where it ends. */
function digits(s: string[], at: number): [string, number] {
  let end = at;
  while (end < s.length && isDigit(s[end]!)) end++;
  const run = s.slice(at, end).join("");
  return [run.replace(/^0+(?=.)/, ""), end];
}

function compare<T extends string | number>(a: T, b: T): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** 32-bit FNV-1a over UTF-8, the same hash as the Rust core's. */
export function fnv1a(text: string): number {
  let h = 0x811c9dc5;
  for (const b of new TextEncoder().encode(text)) h = Math.imul(h ^ b, 0x01000193) >>> 0;
  return h;
}
