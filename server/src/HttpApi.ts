// The authenticated part of the HTTP API in docs/contract.md, served by each user's Durable Object. `/health` and
// `POST /v1/accounts` are answered by the Worker itself (see Api.ts).
import * as Schema from "effect/Schema";
import { HttpApi, HttpApiEndpoint, HttpApiGroup } from "effect/http-api";
import {
  Answer,
  CardSort,
  CardUpdate,
  CardView,
  Deck,
  DeckSummary,
  DeckUpdate,
  NewCard,
  NewCards,
  NewDeck,
  NextReview,
  Ok,
  Stats,
  SyncRequest,
  SyncResponse,
  TagSummary,
  Ulid,
} from "./Model.ts";

const ErrorBody = Schema.Struct({ code: Schema.String, message: Schema.String });

export class BadRequest extends Schema.Error<BadRequest>("BadRequest")(
  { error: ErrorBody },
  { httpApiStatus: 400 },
) {}
export class Unauthorized extends Schema.Error<Unauthorized>("Unauthorized")(
  { error: ErrorBody },
  { httpApiStatus: 401 },
) {}
export class NotFound extends Schema.Error<NotFound>("NotFound")({ error: ErrorBody }, { httpApiStatus: 404 }) {}
export class Conflict extends Schema.Error<Conflict>("Conflict")({ error: ErrorBody }, { httpApiStatus: 409 }) {}

export const errors = [BadRequest, Unauthorized, NotFound, Conflict] as const;

const IdParams = { id: Ulid };
const DeckFilter = { deck_id: Schema.optionalKey(Ulid) };
const Count = Schema.NumberFromString.check(Schema.isInt(), Schema.isGreaterThanOrEqualTo(0));
const Seed = Count.check(Schema.isLessThanOrEqualTo(0xffff_ffff));

export class DecksGroup extends HttpApiGroup.make("decks")
  .add(
    HttpApiEndpoint.get("list", "/v1/decks", {
      success: Schema.Struct({ decks: Schema.Array(DeckSummary) }),
      error: errors,
    }),
    HttpApiEndpoint.post("create", "/v1/decks", { payload: NewDeck, success: Deck, error: errors }),
    HttpApiEndpoint.patch("update", "/v1/decks/:id", {
      params: IdParams,
      payload: DeckUpdate,
      success: Deck,
      error: errors,
    }),
    HttpApiEndpoint.delete("delete", "/v1/decks/:id", { params: IdParams, success: Ok, error: errors }),
  ) {}

export class CardsGroup extends HttpApiGroup.make("cards")
  .add(
    HttpApiEndpoint.get("list", "/v1/cards", {
      query: {
        ...DeckFilter,
        q: Schema.optionalKey(Schema.String),
        tag: Schema.optionalKey(Schema.String),
        due: Schema.optionalKey(Schema.Literals(["true", "false"])),
        limit: Schema.optionalKey(Count),
        offset: Schema.optionalKey(Count),
        sort: Schema.optionalKey(CardSort),
        desc: Schema.optionalKey(Schema.Literals(["true", "false"])),
        seed: Schema.optionalKey(Seed),
      },
      success: Schema.Struct({ cards: Schema.Array(CardView) }),
      error: errors,
    }),
    HttpApiEndpoint.post("create", "/v1/cards", {
      payload: Schema.Union([NewCard, NewCards]),
      success: Schema.Union([CardView, Schema.Struct({ cards: Schema.Array(CardView) })]),
      error: errors,
    }),
    HttpApiEndpoint.get("get", "/v1/cards/:id", { params: IdParams, success: CardView, error: errors }),
    HttpApiEndpoint.patch("update", "/v1/cards/:id", {
      params: IdParams,
      payload: CardUpdate,
      success: CardView,
      error: errors,
    }),
    HttpApiEndpoint.delete("delete", "/v1/cards/:id", { params: IdParams, success: Ok, error: errors }),
    HttpApiEndpoint.get("tags", "/v1/tags", {
      query: DeckFilter,
      success: Schema.Struct({ tags: Schema.Array(TagSummary) }),
      error: errors,
    }),
  ) {}

export class ReviewGroup extends HttpApiGroup.make("review")
  .add(
    HttpApiEndpoint.get("next", "/v1/review/next", { query: DeckFilter, success: NextReview, error: errors }),
    HttpApiEndpoint.post("answer", "/v1/cards/:id/reviews", {
      params: IdParams,
      payload: Answer,
      success: CardView,
      error: errors,
    }),
    HttpApiEndpoint.get("stats", "/v1/stats", { query: DeckFilter, success: Stats, error: errors }),
  ) {}

export class SyncGroup extends HttpApiGroup.make("sync")
  .add(
    HttpApiEndpoint.post("sync", "/v1/sync", { payload: SyncRequest, success: SyncResponse, error: errors }),
  ) {}

export class RecallApi extends HttpApi.make("recall").add(DecksGroup, CardsGroup, ReviewGroup, SyncGroup) {}
