# Furiten Cloud server

The Cloudflare Worker (`furiten-<stage>-api`) and the per-user Durable Object behind the HTTP API, MCP and sync in
[docs/contract.md](../docs/contract.md). TypeScript, Effect, deployed with Alchemy. Scheduling runs `crates/srs`
compiled to wasm, so it matches every client exactly.

- `src/Api.ts`: the Worker. Answers `GET /health` and `POST /v1/accounts`, and sends everything else under `/v1` and
  `/mcp` to the Durable Object named by the token's user id.
- `src/Users.ts`: the Durable Object. Checks the token against its stored SHA-256, then serves the HTTP API
  (`src/HttpApi.ts`) and MCP (`src/Mcp.ts`).
- `src/Store.ts`: the library in the object's SQLite: decks, cards, reviews, cached schedules, and sync.
- `src/srs.ts`: the wrapper over `build/furiten_srs.wasm`.

## Run locally

Needs Bun 1.4 and Rust with the `wasm32-unknown-unknown` target.

```sh
cd server
bun install
bun run dev    # builds the wasm, then serves http://localhost:1337 in a local workerd (offline)
```

```sh
curl localhost:1337/health
TOKEN=$(curl -s -X POST localhost:1337/v1/accounts | jq -r .token)
curl -H "Authorization: Bearer $TOKEN" localhost:1337/v1/decks
```

`alchemy dev` uses the stage `dev_$USER`, keeps state and data in `.alchemy/`, and reloads on change.

## Test

```sh
bun run check   # typecheck, then the tests
bun run test    # just the tests
```

The tests start `alchemy dev` on the `test` stage and drive the real Worker and Durable Object over HTTP, offline.
They're black-box: `FURITEN_BASE_URL=http://localhost:1337 bun test` runs them against a server that's already up.

## Deploy

Only CI deploys (`.github/workflows/deploy.yml`), with the `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
secrets:

| Branch | Stage | Worker |
| --- | --- | --- |
| `dev` (when `server/` or `crates/srs/` change) | `dev` | `furiten-dev-api` |
| `production` | `production` | `furiten-production-api` |

CI runs `bun run deploy:dev` or `bun run deploy:production`, which keep Alchemy state in Cloudflare
(`FURITEN_STATE=cloudflare`). Both are served at their `workers.dev` URL; there's no custom domain yet.
