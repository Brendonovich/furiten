import * as Cloudflare from "@alchemy.run/cloudflare";
import * as Alchemy from "alchemy";
import * as Effect from "effect/Effect";
import Api from "./src/Api.ts";

// Deploys (CI only) keep state in Cloudflare; `alchemy dev` and the tests keep it in .alchemy/ and run offline.
export default Alchemy.Stack(
  "furiten",
  {
    providers: Cloudflare.providers(),
    state: process.env.FURITEN_STATE === "cloudflare" ? Cloudflare.state() : Alchemy.localState(),
  },
  Effect.gen(function* () {
    const api = yield* Api;
    return { url: api.url.as<string>() };
  }),
);
