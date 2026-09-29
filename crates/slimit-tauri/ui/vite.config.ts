import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 前端：固定 5173，严格端口避免 devUrl 失配；宿主依赖走预构建排除。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    target: "safari17",
    minify: "esbuild",
    sourcemap: false,
  },
});
