// Builds crates/srs to wasm and copies it to build/furiten_srs.wasm, where src/srs.ts imports it.
import { $ } from "bun";
import { mkdirSync } from "node:fs";

const root = new URL("../..", import.meta.url).pathname;
await $`cargo build -p furiten-srs --release --target wasm32-unknown-unknown`.cwd(root);
mkdirSync(new URL("../build", import.meta.url), { recursive: true });
await $`cp target/wasm32-unknown-unknown/release/furiten_srs.wasm server/build/furiten_srs.wasm`.cwd(root);
