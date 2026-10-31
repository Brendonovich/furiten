# Server

- Effect is pinned to 4.0.x (with `overrides`) because Alchemy 2.0.0-beta.84 imports modules Effect 4.1 removed.
  Upgrade Effect and Alchemy together.
- `src/Store.ts` mirrors `crates/core`'s store (queries, sides, tags, `updated_at` bumps). When one changes behaviour,
  change the other and the contract.
- Migrations in `src/Store.ts` are append-only: user data lives in Durable Objects that were created with every
  earlier version.
- Tests are black-box HTTP against a base URL (see `test/harness.ts`); keep them that way so they can move into
  `tests/conformance`.
- Never import the wasm outside the Durable Object: `src/srs.ts` takes a `WebAssembly.Module` so tests can build
  one from `build/recall_srs.wasm`.
