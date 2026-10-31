// The contract's data types (docs/contract.md), as schemas shared by the HTTP API, MCP and sync.
import * as Schema from "effect/Schema";

export const Ulid = Schema.String.check(Schema.isPattern(/^[0-9A-HJKMNP-TV-Z]{26}$/)).annotate({
  description: "A 26-character Crockford base32 ULID.",
});

const Time = Schema.Int.annotate({ description: "Unix milliseconds." });

export const Deck = Schema.Struct({
  id: Ulid,
  name: Schema.String,
  parent_id: Schema.NullOr(Ulid),
  created_at: Time,
  updated_at: Time,
  deleted: Schema.Boolean,
});
export type Deck = typeof Deck.Type;

export const Card = Schema.Struct({
  id: Ulid,
  deck_id: Ulid,
  content: Schema.String,
  tags: Schema.Array(Schema.String),
  created_at: Time,
  updated_at: Time,
  deleted: Schema.Boolean,
});
export type Card = typeof Card.Type;

export const Rating = Schema.Literals([1, 2, 3, 4]).annotate({
  description: "1 again, 2 hard, 3 good, 4 easy.",
});

export const Review = Schema.Struct({
  id: Ulid,
  card_id: Ulid,
  rating: Rating,
  reviewed_at: Time,
  duration_ms: Schema.NullOr(Schema.Int),
});
export type Review = typeof Review.Type;

export const Schedule = Schema.Struct({
  state: Schema.Literals(["new", "learning", "review", "relearning"]),
  due: Time,
  stability: Schema.Number,
  difficulty: Schema.Number,
  reps: Schema.Int,
  lapses: Schema.Int,
  last_review: Schema.NullOr(Time),
});

export const DeckSummary = Schema.Struct({
  ...Deck.fields,
  cards: Schema.Int,
  due: Schema.Int,
  new: Schema.Int,
});
export type DeckSummary = typeof DeckSummary.Type;

export const CardView = Schema.Struct({
  ...Card.fields,
  sides: Schema.Array(Schema.String),
  schedule: Schedule,
});
export type CardView = typeof CardView.Type;

export const Change = Schema.Union([
  Schema.Struct({ kind: Schema.Literal("deck"), value: Deck }),
  Schema.Struct({ kind: Schema.Literal("card"), value: Card }),
  Schema.Struct({ kind: Schema.Literal("review"), value: Review }),
]);
export type Change = typeof Change.Type;

export const SyncRequest = Schema.Struct({
  cursor: Schema.NullOr(Schema.Int),
  changes: Schema.Array(Change),
});
export type SyncRequest = typeof SyncRequest.Type;

export const SyncResponse = Schema.Struct({
  cursor: Schema.Int,
  changes: Schema.Array(Change),
});
export type SyncResponse = typeof SyncResponse.Type;

export const NewDeck = Schema.Struct({
  name: Schema.String,
  parent_id: Schema.optionalKey(Schema.NullOr(Ulid)),
});
export type NewDeck = typeof NewDeck.Type;

export const DeckUpdate = Schema.Struct({
  name: Schema.optionalKey(Schema.String),
  parent_id: Schema.optionalKey(Schema.NullOr(Ulid)),
});
export type DeckUpdate = typeof DeckUpdate.Type;

export const NewCard = Schema.Struct({
  deck_id: Ulid,
  content: Schema.String,
  tags: Schema.optionalKey(Schema.Array(Schema.String)),
});
export type NewCard = typeof NewCard.Type;

export const NewCards = Schema.Struct({ cards: Schema.Array(NewCard) });

export const CardUpdate = Schema.Struct({
  content: Schema.optionalKey(Schema.String),
  tags: Schema.optionalKey(Schema.Array(Schema.String)),
  deck_id: Schema.optionalKey(Ulid),
});
export type CardUpdate = typeof CardUpdate.Type;

export interface CardQuery {
  readonly deck_id?: string | undefined;
  readonly q?: string | undefined;
  readonly tag?: string | undefined;
  readonly due?: boolean | undefined;
  readonly limit?: number | undefined;
  readonly offset?: number | undefined;
}

export const NextReview = Schema.Struct({
  card: Schema.NullOr(CardView),
  remaining: Schema.Struct({ due: Schema.Int, new: Schema.Int }),
});
export type NextReview = typeof NextReview.Type;

export const Answer = Schema.Struct({
  rating: Rating,
  duration_ms: Schema.optionalKey(Schema.NullOr(Schema.Int)),
});

export const Stats = Schema.Struct({
  cards: Schema.Int,
  due: Schema.Int,
  new: Schema.Int,
  reviews_today: Schema.Int,
  retention_30d: Schema.NullOr(Schema.Number),
});
export type Stats = typeof Stats.Type;

export const Ok = Schema.Struct({ ok: Schema.Literal(true) });

/** A domain failure, `{ code, message }` with the HTTP status it maps to. */
export class ApiError extends Error {
  constructor(
    readonly status: 400 | 401 | 404 | 409,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
  static invalid = (message: string) => new ApiError(400, "invalid_params", message);
  static notFound = (what: string, id: string) => new ApiError(404, "not_found", `${what} ${id} not found`);
  static unauthorized = () => new ApiError(401, "unauthorized", "missing or invalid bearer token");

  toJSON() {
    return { error: { code: this.code, message: this.message } };
  }
}
