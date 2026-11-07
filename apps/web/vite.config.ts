import solid from "@solidjs/vite-plugin";
import { defineConfig } from "vite";

// The Worker (`cd server && bun run dev`) serves the API on :1337. In dev the app proxies it, so a phone only needs
// this one address (over the LAN or Tailscale) and the app talks to the same origin.
const api = process.env.RECALL_API ?? "http://localhost:1337";

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
