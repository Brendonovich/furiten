import solid from "@solidjs/vite-plugin";
import { defineConfig } from "vite";

// The desktop's web access (`furiten web on`) serves the API on :8787. In dev the app proxies it, so a phone only
// needs this one address (over the LAN or Tailscale) and the app talks to the same origin.
const api = process.env.FURITEN_API ?? "http://localhost:8787";

export default defineConfig({
  plugins: [solid()],
  server: {
    port: 5173,
    allowedHosts: true,
    proxy: { "/v1": api, "/health": api },
  },
  preview: {
    port: 4173,
    allowedHosts: true,
    proxy: { "/v1": api, "/health": api },
  },
});
