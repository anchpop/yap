import path from "path"
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import tailwindcss from "@tailwindcss/vite"
import { VitePWA } from 'vite-plugin-pwa'
import { visualizer } from 'rollup-plugin-visualizer'

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

// https://vite.dev/config/
export default defineConfig({
  plugins: [
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
        navigateFallbackDenylist: [/^\/d\//, /^\/blog\//],
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
    topLevelAwait(), 
    tailwindcss(),
    visualizer({
      open: false,  // Don't auto-open on every build
      filename: 'bundle-analysis.html',
      gzipSize: true,
      brotliSize: true,
    })
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
})
