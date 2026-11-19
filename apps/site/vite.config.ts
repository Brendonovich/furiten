import solid from "@solidjs/vite-plugin";
import { defineConfig } from "vite";

// The site renders the web app's own components and stylesheet (../web/src), with one copy of Solid between them.
export default defineConfig({
  plugins: [solid()],
  resolve: { dedupe: ["solid-js", "@solidjs/web"] },
  server: { port: 5174, allowedHosts: true, fs: { allow: [".."] } },
  preview: { port: 4174, allowedHosts: true },
});
