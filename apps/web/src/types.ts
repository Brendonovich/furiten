// The contract's types (docs/contract.md), as the HTTP API sends them. Times are Unix milliseconds.

export type Deck = {
  id: string;
  name: string;
  parent_id: string | null;
  created_at: number;
  updated_at: number;
  deleted: boolean;
};

export type DeckSummary = Deck & { cards: number; due: number; new: number };

export type State = "new" | "learning" | "review" | "relearning";

export type Schedule = {
  state: State;
  due: number;
  stability: number;
  difficulty: number;
  reps: number;
  lapses: number;
  last_review: number | null;
};

export type CardView = {
  id: string;
  deck_id: string;
  content: string;
  tags: string[];
  created_at: number;
  updated_at: number;
  deleted: boolean;
  sides: string[];
  schedule: Schedule;
};

export type Remaining = { due: number; new: number };
export type NextReview = { card: CardView | null; remaining: Remaining };
export type TagSummary = { tag: string; cards: number; last_used: number };

export type Sort = "created" | "updated" | "name" | "due" | "reviewed" | "interval" | "retention" | "random";
export type View = "grid" | "list" | "column";
