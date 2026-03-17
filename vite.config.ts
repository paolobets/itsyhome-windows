import { defineConfig } from 'vite'
import { resolve }      from 'path'

export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        popup:    resolve(__dirname, 'src/renderer/popup/index.html'),
        settings: resolve(__dirname, 'src/renderer/settings/index.html'),
      },
      output: { dir: 'dist' },
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
