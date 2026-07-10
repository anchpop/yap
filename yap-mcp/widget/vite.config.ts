import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { viteSingleFile } from "vite-plugin-singlefile";
import path from "node:path";

const frontendSrc = path.resolve(__dirname, "../../yap-frontend/src");

// The widget reuses yap-frontend components verbatim via the `@` alias. Two
// shims keep the WASM module out of the bundle: `@/lib/utils` (imports wasm
// at module level for playAudio) and `yap-frontend-rs/pkg` for anything that
// still resolves it — type-only imports erase, but a stray value import
// should fail loudly rather than pull 4 MiB into the iframe.
export default defineConfig({
  plugins: [react(), tailwindcss(), viteSingleFile()],
  resolve: {
    // Reused app components resolve react from yap-frontend/node_modules;
    // dedupe so exactly one React instance ends up in the bundle.
    dedupe: ["react", "react-dom"],
    alias: [
      { find: "@/lib/utils", replacement: path.resolve(__dirname, "src/shims/utils.ts") },
      {
        find: "@/lib/sound-effects",
        replacement: path.resolve(__dirname, "src/shims/sound-effects.ts"),
      },
      { find: "@", replacement: frontendSrc },
      {
        // Bare or relative, however deep: anything resolving to the pkg.
        // Must consume the WHOLE specifier — alias does a string replace, so
        // a partial match would leave the relative prefix glued on.
        find: /^(?:.*\/)?yap-frontend-rs\/pkg$/,
        replacement: path.resolve(__dirname, "src/shims/wasm-guard.ts"),
      },
    ],
  },
  build: {
    target: "es2022",
    // One self-contained HTML file, served as the ui:// MCP resource.
    assetsInlineLimit: 100_000_000,
    chunkSizeWarningLimit: 4_000,
  },
});
