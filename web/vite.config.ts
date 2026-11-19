import { defineConfig } from "vitest/config";
import solid from "@solidjs/vite-plugin";

export default defineConfig({
  plugins: [solid()],
  build: {
    target: "es2024",
  },
  test: {
    environment: "jsdom",
  },
});
