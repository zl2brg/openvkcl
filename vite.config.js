import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [sveltekit()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // Without an explicit target, Vite 8 defaults to Safari 16.4+. That leaves a
    // black window on older macOS WKWebViews: Big Sur ships Safari 14.1, Monterey
    // Safari 15.x — both below 16.4 — so the JS fails to parse before anything
    // renders. Prefer an ES year over safariNN: esbuild cannot downlevel some
    // SvelteKit destructuring patterns to safari13/14, but es2020 strips
    // ES2021+ syntax (e.g. ??=) that Big Sur cannot parse, while keeping
    // features Safari 14 already has (optional chaining, nullish coalescing).
    // https://v2.tauri.app/start/frontend/vite/
    // https://v2.tauri.app/reference/webview-versions/
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "es2020",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    cssTarget: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari14",
  },
}));
