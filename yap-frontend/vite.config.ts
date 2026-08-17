import fs from "fs"
import path from "path"
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import wasm from "vite-plugin-wasm";
import tailwindcss from "@tailwindcss/vite"
import { VitePWA } from 'vite-plugin-pwa'
import { visualizer } from 'rollup-plugin-visualizer'
import { sentryVitePlugin } from "@sentry/vite-plugin"

// Serve static site pages (dictionary, blog) without SPA fallback intercepting
function staticSitePlugin() {
  return {
    name: 'static-site',
    configureServer(server: any) {
      server.middlewares.use((req: any, _res: any, next: any) => {
        if (req.url?.startsWith('/d/') || req.url === '/d' ||
            req.url?.startsWith('/blog/') || req.url === '/blog') {
          // Rewrite directory requests to their index.html
          if (!req.url.includes('.')) {
            const path = req.url.endsWith('/') ? req.url : req.url + '/';
            req.url = path + 'index.html';
          }
        }
        next();
      });
    },
  };
}

// A WASM built with `--features local-backend` hardcodes http://localhost:21516
// as the AI backend. `pnpm build` bundles whatever sits in yap-frontend-rs/pkg,
// so a stale local-test build can silently ship to production (this happened:
// autograde/TTS were broken on yap.town for three days). Fail the build instead.
function localBackendGuardPlugin() {
  return {
    name: 'local-backend-guard',
    apply: 'build' as const,
    buildStart() {
      if (process.env.ALLOW_LOCAL_BACKEND) return;
      const wasmPath = path.resolve(__dirname, "../yap-frontend-rs/pkg/yap_frontend_rs_bg.wasm");
      if (fs.readFileSync(wasmPath).includes("localhost:21516")) {
        throw new Error(
          "yap-frontend-rs/pkg was built with --features local-backend (AI backend = localhost:21516). " +
          "Rebuild it without the feature (cd yap-frontend-rs && CARGO_PROFILE_RELEASE_LTO=true wasm-pack build --release) " +
          "or set ALLOW_LOCAL_BACKEND=1 to build anyway."
        );
      }
    },
  };
}

// https://vite.dev/config/
export default defineConfig({
  build: {
    sourcemap: true,
  },
  server: {
    // Allow access through the yap-preview.beef.baby reverse proxy
    allowedHosts: ["yap-preview.beef.baby"],
  },
  plugins: [
    localBackendGuardPlugin(),
    staticSitePlugin(),
    VitePWA({ 
      registerType: 'autoUpdate',
      devOptions: {
        enabled: false,
        //enabled: true,
        type: 'module',
      },
      workbox: {
        globPatterns: ['**/*.{js,css,html,ico,png,svg,wasm,wav,mp3}'],
        globIgnores: ['**/d/**', '**/blog/**'],
        importScripts: [],
        maximumFileSizeToCacheInBytes: 4 * 1024 * 1024, // 4 MiB to cover the current WASM bundle
        navigateFallbackDenylist: [/^\/d\//, /^\/blog\//, /^\/sitemap/, /^\/robots\.txt/],
      },
      manifest: {
        name: 'Yap.Town',
        short_name: 'Yap',
        description: 'Language learning made easy',
        theme_color: '#0A0A0A',
        background_color: '#0A0A0A',
        id: "https://yap.town/",
        start_url: "/",
        icons: [
          {
            src: 'pwa-64x64.png',
            sizes: '64x64',
            type: 'image/png'
          },
          {
            src: 'pwa-192x192.png',
            sizes: '192x192',
            type: 'image/png'
          },
          {
            src: 'pwa-512x512.png',
            sizes: '512x512',
            type: 'image/png'
          }
        ],
        screenshots: [
          {
            src: "screenshot-wide.png",
            sizes: "1988x1176",
            type: "image/gif",
            form_factor: "wide",
            label: "Application"
          },
          {
            src: "screenshot-mobile.png",
            sizes: "584x1260",
            type: "image/gif",
            label: "Application"
          }
        ],
        shortcuts: [
          {
            name: "Dictionary",
            url: "/dictionary",
            description: "Look up words and add new words to your deck"
          }
        ]
      }
    }),
    react({
      babel: {
        plugins: [
          ["babel-plugin-react-compiler", {
            compilationMode: "infer", // Only compile components that would benefit
            runtimeModule: "react"
          }]
        ]
      }
    }), 
    wasm(),
    tailwindcss(),
    visualizer({
      open: false,  // Don't auto-open on every build
      filename: 'bundle-analysis.html',
      gzipSize: true,
      brotliSize: true,
    }),
    sentryVitePlugin({
      org: "yaptown",
      project: "javascript-react",
      authToken: process.env.SENTRY_AUTH_TOKEN,
    }),
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
})
