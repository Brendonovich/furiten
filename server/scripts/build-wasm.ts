// Builds crates/srs to wasm and copies it to build/recall_srs.wasm, where src/srs.ts imports it.
import { $ } from "bun";
import { mkdirSync } from "node:fs";

const root = new URL("../..", import.meta.url).pathname;
await $`cargo build -p recall-srs --release --target wasm32-unknown-unknown`.cwd(root);
mkdirSync(new URL("../build", import.meta.url), { recursive: true });
await $`cp target/wasm32-unknown-unknown/release/recall_srs.wasm server/build/recall_srs.wasm`.cwd(root);
