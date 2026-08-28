import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/.test-data/**"] },
  },
  build: {
    target: "safari17",
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [{ name: "math", test: /node_modules[\\/]katex[\\/]/ }],
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test-setup.ts"],
  },
});
