import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const verificationMeta = [
    ["google-site-verification", env.VITE_GOOGLE_SITE_VERIFICATION],
    ["msvalidate.01", env.VITE_BING_SITE_VERIFICATION],
  ]
    .filter(([, value]) => value)
    .map(([name, value]) => `    <meta name="${name}" content="${value}" />`)
    .join("\n");

  return {
    plugins: [
      react(),
      {
        name: "verification-meta",
        transformIndexHtml(html) {
          return html.replace("<!-- verification-meta -->", verificationMeta);
        },
      },
    ],
  };
});
