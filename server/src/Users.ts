// One Durable Object per user (named by the token's user id), holding their library in SQLite. It checks the token
// itself and serves the authenticated HTTP API and MCP.
import * as Cloudflare from "@alchemy.run/cloudflare";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import { McpProtocol, McpServer } from "effect/ai";
import { Etag, HttpPlatform, HttpRouter, HttpServerRequest, HttpServerResponse } from "effect/http";
import { HttpApiBuilder } from "effect/http-api";
import { HttpApiSchemaError } from "effect/http-api/HttpApiError";
import srsWasm from "../build/recall_srs.wasm";
import { parseToken, verifySecret } from "./Auth.ts";
import { BadRequest, Conflict, NotFound, RecallApi, Unauthorized } from "./HttpApi.ts";
import * as Mcp from "./Mcp.ts";
import { ApiError } from "./Model.ts";
import { makeScheduler } from "./srs.ts";
import { Store } from "./Store.ts";
import { VERSION } from "./Version.ts";

const HttpPlatformStub = Layer.succeed(HttpPlatform.HttpPlatform, {
  platform: "web",
  compression: {
    algorithms: new Set<HttpPlatform.CompressionAlgorithm>(),
    compressResponse: () => Effect.die("compression is unavailable in a Durable Object"),
  },
  fileResponse: () => Effect.die("fileResponse is unavailable in a Durable Object"),
  fileWebResponse: () => Effect.die("fileWebResponse is unavailable in a Durable Object"),
});

const httpError = (e: ApiError) => {
  const body = { error: { code: e.code, message: e.message } };
  switch (e.status) {
    case 400:
      return new BadRequest(body);
    case 401:
      return new Unauthorized(body);
    case 404:
      return new NotFound(body);
    case 409:
      return new Conflict(body);
  }
};

/** Runs a store call, turning its `ApiError`s into typed failures and anything else into a defect. */
const attempt = <A>(f: () => A): Effect.Effect<A, ApiError> =>
  Effect.try({ try: f, catch: (e) => e }).pipe(
    Effect.catch((e) => (e instanceof ApiError ? Effect.fail(e) : Effect.die(e))),
  );

const errorResponse = (error: ApiError) =>
  Effect.succeed(HttpServerResponse.jsonUnsafe(error.toJSON(), { status: error.status }));

const makeApp = (store: Store) => {
  const http = <A>(f: () => A) => attempt(f).pipe(Effect.mapError(httpError));
  const tool = <A>(f: () => A) =>
    attempt(f).pipe(Effect.mapError((e) => ({ code: e.code, message: e.message })));

  const decks = HttpApiBuilder.group(RecallApi, "decks", (h) =>
    h
      .handle("list", () => http(() => ({ decks: store.decks() })))
      .handle("create", ({ payload }) => http(() => store.createDeck(payload)))
      .handle("update", ({ params, payload }) => http(() => store.updateDeck(params.id, payload)))
      .handle("delete", ({ params }) => http(() => (store.deleteDeck(params.id), { ok: true as const }))),
  );

  const cards = HttpApiBuilder.group(RecallApi, "cards", (h) =>
    h
      .handle("list", ({ query }) =>
        http(() => {
          const flag = (v: "true" | "false" | undefined) => (v === undefined ? undefined : v === "true");
          return { cards: store.cards({ ...query, due: flag(query.due), desc: flag(query.desc) }) };
        }),
      )
      .handle("create", ({ payload }) =>
        http(() => ("cards" in payload ? { cards: store.createCards(payload.cards) } : store.createCards([payload])[0]!)),
      )
      .handle("get", ({ params }) => http(() => store.card(params.id)))
      .handle("update", ({ params, payload }) => http(() => store.updateCard(params.id, payload)))
      .handle("delete", ({ params }) => http(() => (store.deleteCard(params.id), { ok: true as const })))
      .handle("tags", ({ query }) => http(() => ({ tags: store.tags(query.deck_id) }))),
  );

  const review = HttpApiBuilder.group(RecallApi, "review", (h) =>
    h
      .handle("next", ({ query }) => http(() => store.nextReview(query.deck_id)))
      .handle("answer", ({ params, payload }) =>
        http(() => store.answer(params.id, payload.remembered, payload.duration_ms)),
      )
      .handle("stats", ({ query }) => http(() => store.stats(query.deck_id))),
  );

  const sync = HttpApiBuilder.group(RecallApi, "sync", (h) =>
    h.handle("sync", ({ payload }) => http(() => store.sync(payload))),
  );

  const api = HttpApiBuilder.layer(RecallApi).pipe(
    Layer.provide([decks, cards, review, sync]),
    Layer.provide([Etag.layer, HttpPlatformStub, Path.layer, FileSystem.layerNoop({})]),
  );

  const tools = Mcp.toolkit.toLayer({
    list_decks: () => tool(() => ({ decks: store.decks() })),
    create_deck: (p) => tool(() => store.createDeck(p)),
    search_cards: (p) => tool(() => ({ cards: store.cards(p) })),
    create_cards: (p) => tool(() => ({ cards: store.createCards(p.cards) })),
    update_card: ({ id, ...update }) => tool(() => store.updateCard(id, update)),
    delete_card: ({ id }) => tool(() => (store.deleteCard(id), { ok: true as const })),
    next_review: (p) => tool(() => store.nextReview(p.deck_id)),
    review_card: (p) => tool(() => store.answer(p.id, p.remembered, p.duration_ms)),
    stats: (p) => tool(() => store.stats(p.deck_id)),
  });

  const mcp = McpServer.toolkit(Mcp.toolkit).pipe(
    Layer.provide(tools),
    Layer.provideMerge(
      McpServer.layerHttp({
        name: "Recall",
        version: VERSION,
        instructions:
          "Recall is a spaced-repetition flashcard app. Cards are Markdown; a line containing only `---` splits " +
          "a card into its front and back.",
        path: "/mcp",
        protocols: [McpProtocol.v2026_07_28, McpProtocol.v2025_11_25, McpProtocol.v2025_06_18, McpProtocol.v2025_03_26],
      }),
    ),
  );

  return HttpRouter.toHttpEffect(Layer.mergeAll(api, mcp));
};

export default class Users extends Cloudflare.DurableObject<Users>()(
  "Users",
  Effect.gen(function* () {
    const state = yield* Cloudflare.DurableObjectState;

    return Effect.gen(function* () {
      const storage = state.raw.storage;
      const store = new Store(
        { sql: storage.sql, transaction: (f) => storage.transactionSync(f) },
        makeScheduler(srsWasm),
      );
      const app = yield* makeApp(store).pipe(Effect.orDie);

      return {
        /** Creates the account with the SHA-256 (hex) of its secret. */
        createAccount: (secretHash: string) =>
          attempt(() => store.createAccount(secretHash)).pipe(
            Effect.map(() => ({ ok: true as const })),
            Effect.mapError((e) => e.toJSON()),
          ),

        fetch: Effect.gen(function* () {
          const request = yield* HttpServerRequest.HttpServerRequest;
          const stored = store.secretHash();
          const token = parseToken(request.headers.authorization);
          if (stored === undefined || token === undefined || !(yield* verifySecret(token.secret, stored))) {
            return yield* errorResponse(ApiError.unauthorized());
          }
          return yield* app.pipe(
            Effect.catchCause((cause) => {
              const error = cause.reasons.find((r) => r._tag === "Die" || r._tag === "Fail");
              const value = error?._tag === "Die" ? error.defect : error?._tag === "Fail" ? error.error : undefined;
              if (HttpApiSchemaError.is(value)) {
                return errorResponse(ApiError.invalid(String(value.cause.message ?? value.cause)));
              }
              const reason = (value as { reason?: { _tag?: string } } | undefined)?.reason?._tag;
              if (reason === "RouteNotFound") {
                return errorResponse(new ApiError(404, "not_found", `no route for ${request.method} ${request.url}`));
              }
              if (reason === "RequestParseError") {
                return errorResponse(ApiError.invalid("the request body isn't valid JSON"));
              }
              return Effect.logError("unhandled", cause).pipe(
                Effect.as(
                  HttpServerResponse.jsonUnsafe(
                    { error: { code: "internal", message: "internal error" } },
                    { status: 500 },
                  ),
                ),
              );
            }),
          );
        }),
      };
    });
  }),
) {}
