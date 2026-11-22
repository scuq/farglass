import { defineConfig } from "vitest/config";
import solid from "@solidjs/vite-plugin";
import pkg from "./package.json" with { type: "json" };

export default defineConfig({
  plugins: [solid()],
  // The release sets the version of package.json to the version of the tag.
  define: {
    __FARGLASS_VERSION__: JSON.stringify(pkg.version),
  },
  build: {
    target: "es2024",
  },
  server: {
    // `npm run dev` sends the WebSocket of the shell to a farglassd on this host.
    // FARGLASSD_URL selects a different farglassd.
    // farglassd must accept the origin of the Vite server: allowed_origins = ["http://localhost:5173"].
    proxy: {
      "/ws": {
        target: process.env.FARGLASSD_URL ?? "https://localhost:8443",
        ws: true,
        // farglassd has a self-signed certificate by default.
        secure: false,
      },
      "/session": {
        target: process.env.FARGLASSD_URL ?? "https://localhost:8443",
        secure: false,
      },
    },
  },
  test: {
    environment: "jsdom",
  },
});
