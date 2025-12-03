import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async ({ mode }) => {
  // @ts-expect-error process.cwd is a nodejs global
  const env = loadEnv(mode, process.cwd(), "");
  const nesProxyTarget = env.NES_PROXY_TARGET || env.VITE_NES_API_URL || "http://localhost:8081";

  return {
    plugins: [react()],

    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      host: host || false,
      hmr: host
        ? {
            protocol: "ws",
            host,
            port: 1421,
          }
        : undefined,
      watch: {
        ignored: ["**/src-tauri/**"],
      },
      proxy: {
        "/v1/nes": {
          target: nesProxyTarget,
          changeOrigin: true,
        },
        "/mqtt": {
          target: nesProxyTarget,
          ws: true,
          changeOrigin: true,
        },
      },
    },
  };
});
