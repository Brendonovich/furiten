// The Worker: answers `/health` and account creation itself and routes everything else under `/v1` and `/mcp` to
// the Durable Object named by the bearer token's user id.
import * as Cloudflare from "@alchemy.run/cloudflare";
import * as Alchemy from "alchemy";
import * as Effect from "effect/Effect";
import { HttpMiddleware, HttpServerRequest, HttpServerResponse } from "effect/http";
import { newAccount, parseToken, sha256Hex } from "./Auth.ts";
import { ApiError } from "./Model.ts";
import Users from "./Users.ts";
import { VERSION } from "./Version.ts";

/** `recall-<stage>-<name>`, made safe for Cloudflare names (local stages such as `dev_<user>` contain `_`). */
export const resourceName = (stage: string, name: string) =>
  `recall-${stage}-${name}`.toLowerCase().replace(/[^a-z0-9-]/g, "-");

const errorResponse = (error: ApiError) => HttpServerResponse.jsonUnsafe(error.toJSON(), { status: error.status });

export default class Api extends Cloudflare.Worker<Api>()(
  "Api",
  Effect.gen(function* () {
    const stage = yield* Alchemy.Stage;
    return {
      name: resourceName(stage, "api"),
      main: import.meta.url,
      compatibility: { date: "2026-10-01" },
      observability: { enabled: true },
    };
  }),
  Effect.gen(function* () {
    const users = yield* Users;

    return {
      fetch: Effect.gen(function* () {
        const request = yield* HttpServerRequest.HttpServerRequest;
        const path = new URL(request.url, "http://recall").pathname;

        if (path === "/health" && request.method === "GET") {
          return HttpServerResponse.jsonUnsafe({ ok: true, version: VERSION });
        }

        if (path === "/v1/accounts" && request.method === "POST") {
          const account = newAccount();
          const hash = yield* sha256Hex(account.secret);
          yield* users.getByName(account.userId).createAccount(hash).pipe(Effect.orDie);
          return HttpServerResponse.jsonUnsafe({ user_id: account.userId, token: account.token });
        }

        if (path.startsWith("/v1/") || path === "/mcp") {
          const token = parseToken(request.headers.authorization);
          if (token === undefined) return errorResponse(ApiError.unauthorized());
          return yield* users.getByName(token.userId).fetch(request);
        }

        return errorResponse(new ApiError(404, "not_found", `no route for ${request.method} ${path}`));
      }).pipe(
        Effect.catchCause((cause) =>
          Effect.logError("unhandled", cause).pipe(
            Effect.as(
              HttpServerResponse.jsonUnsafe({ error: { code: "internal", message: "internal error" } }, { status: 500 }),
            ),
          ),
        ),
        HttpMiddleware.cors({ exposedHeaders: ["mcp-session-id"] }),
      ),
    };
  }),
) {}
