import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Dev: Vite on :5173. API calls and webhook ingests are proxied to the
// backend on :3011. Prod: the built dist/ is embedded into the `web` Rust
// binary, which reverse-proxies API paths to the backend itself.
//
// Note: only known API prefixes are proxied here. SPA routes (/providers,
// /hooks, /send, /event/:id) are served by Vite as index.html. To send a
// webhook from the dev UI, the Send page posts to /{source}, which we proxy
// via the wildcard rule below (skipping SPA routes and Vite assets).
const SPA_ROUTES = ["/", "/login", "/providers", "/hooks", "/send", "/event"];

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      "/events": "http://localhost:3011",
      "/rules": "http://localhost:3011",
      "/config": "http://localhost:3011",
      "/health": "http://localhost:3011",
      // OAuth + auth endpoints — same-origin so the session cookie set by the
      // backend's /api/auth/callback/google is sent on subsequent requests.
      "/api": {
        target: "http://localhost:3011",
        changeOrigin: true,
        cookieDomainRewrite: "localhost",
      },
      // Webhook ingest: POST /{source}. Proxy any POST/PUT/DELETE that isn't
      // a known SPA route or a Vite asset path.
      "^/(?!assets/|@vite/|node_modules/|favicon\\.svg|.*\\.).*": {
        target: "http://localhost:3011",
        changeOrigin: true,
        bypass: (req) => {
          // Only intercept write methods; let GET fall through to SPA.
          if (req.method === "GET" || req.method === "HEAD") return req.url;
          // Don't intercept SPA route navigations (defensive).
          const path = req.url?.split("?")[0] ?? "";
          if (SPA_ROUTES.includes(path)) return req.url;
        },
      },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
