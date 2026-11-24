import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
  },
  build: {
    target: "esnext",
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("node_modules/react/") || id.includes("node_modules/react-dom/")) {
            return "react-vendor";
          }
          if (id.includes("node_modules/motion") || id.includes("node_modules/framer-motion")) {
            return "motion";
          }
          if (id.includes("node_modules/@radix-ui/")) {
            return "radix";
          }
          if (id.includes("node_modules/lucide-react")) {
            return "icons";
          }
          if (
            id.includes("node_modules/@tanstack/") ||
            id.includes("node_modules/cmdk") ||
            id.includes("node_modules/sonner") ||
            id.includes("node_modules/zustand")
          ) {
            return "ui-libs";
          }
          return undefined;
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
  },
});
