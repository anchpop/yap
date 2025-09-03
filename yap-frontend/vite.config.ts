import path from "path"
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import tailwindcss from "@tailwindcss/vite"
import { VitePWA } from 'vite-plugin-pwa'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    VitePWA({ 
      registerType: 'autoUpdate',
      devOptions: {
        enabled: false,
        //enabled: true,
        type: 'module',
      },
      workbox: {
        globPatterns: ['**/*.{js,css,html,ico,png,svg,wasm,wav,mp3}'],
        importScripts: [],
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
        ]
      }
    }),
    react(), wasm(), topLevelAwait(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
})
