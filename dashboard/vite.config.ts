import { defineConfig, loadEnv } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "node:path";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const port = Number(env.AIMAN_DASHBOARD_PORT ?? "4020");
  const bind = env.AIMAN_DASHBOARD_BIND ?? "0.0.0.0";
  const host = bind === "0.0.0.0" ? "127.0.0.1" : bind;
  const target = env.VITE_DASHBOARD_URL ?? `http://${host}:${port}`;

  return {
    plugins: [vue()],
    root: path.resolve(__dirname, "src/ui"),
    server: {
      proxy: {
        "/api": target
      }
    },
    build: {
      outDir: path.resolve(__dirname, "dist/ui"),
      emptyOutDir: true
    }
  };
});
