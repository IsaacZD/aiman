import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "node:path";

export default defineConfig({
  plugins: [vue()],
  root: path.resolve(__dirname, "src/ui"),
  server: {
    proxy: {
      "/api": "http://localhost:4020"
    }
  },
  build: {
    outDir: path.resolve(__dirname, "dist/ui"),
    emptyOutDir: true
  }
});
