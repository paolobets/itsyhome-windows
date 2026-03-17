import { defineConfig } from 'vite'
import { resolve }      from 'path'

export default defineConfig({
  // Setting root to src/renderer makes Vite serve/build
  // popup/index.html and settings/index.html at the URL root,
  // matching the paths Tauri expects: popup/index.html, settings/index.html.
  root: resolve(__dirname, 'src/renderer'),
  build: {
    outDir:     resolve(__dirname, 'dist'),
    emptyOutDir: true,
    rollupOptions: {
      input: {
        popup:    resolve(__dirname, 'src/renderer/popup/index.html'),
        settings: resolve(__dirname, 'src/renderer/settings/index.html'),
      },
    },
  },
  resolve: {
    alias: {
      '@shared': resolve(__dirname, 'src/shared'),
      '@lib':    resolve(__dirname, 'src/lib'),
    },
  },
  server: { port: 1420, strictPort: true },
})
