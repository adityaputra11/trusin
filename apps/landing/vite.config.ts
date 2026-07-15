import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, ".", "");
  const verificationMeta = [
    ["google-site-verification", env.VITE_GOOGLE_SITE_VERIFICATION],
    ["msvalidate.01", env.VITE_BING_SITE_VERIFICATION],
  ]
    .filter(([, value]) => value)
    .map(([name, value]) => `    <meta name="${name}" content="${value}" />`)
    .join("\n");
  const analyticsMeta = env.VITE_GA_MEASUREMENT_ID
    ? `    <script async src="https://www.googletagmanager.com/gtag/js?id=${env.VITE_GA_MEASUREMENT_ID}"></script>\n    <script>window.dataLayer=window.dataLayer||[];function gtag(){dataLayer.push(arguments)}gtag('js',new Date());gtag('config','${env.VITE_GA_MEASUREMENT_ID}');</script>`
    : "";

  return {
    plugins: [
      react(),
      {
        name: "verification-meta",
        transformIndexHtml(html) {
          return html.replace("<!-- verification-meta -->", verificationMeta);
        },
      },
      {
        name: "analytics-meta",
        transformIndexHtml(html) {
          return html.replace("<!-- analytics-meta -->", analyticsMeta);
        },
      },
      {
        name: "defer-noncritical-stylesheet",
        apply: "build",
        closeBundle() {
          const indexPath = resolve(process.cwd(), "dist/index.html");
          const index = readFileSync(indexPath, "utf8").replace(
            /<link rel="stylesheet" crossorigin href="([^"]+)">/g,
            '<link rel="preload" as="style" href="$1" onload="this.onload=null;this.rel=\'stylesheet\'"> <noscript><link rel="stylesheet" href="$1"></noscript>',
          );
          writeFileSync(indexPath, index);
        },
      },
    ],
  };
});
