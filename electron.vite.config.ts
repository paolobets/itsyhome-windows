import { resolve } from 'path'
import { defineConfig, externalizeDepsPlugin } from 'electron-vite'

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
    resolve: { alias: { '@shared': resolve('src/shared') } },
    build: {
      rollupOptions: {
        input: { index: resolve(__dirname, 'src/main/index.ts') }
      }
    }
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
    build: {
      rollupOptions: {
        input: {
          popup:    resolve(__dirname, 'src/preload/popup.ts'),
          settings: resolve(__dirname, 'src/preload/settings.ts')
        }
      }
    }
  },
  renderer: {
    resolve: { alias: { '@shared': resolve('src/shared') } },
    build: {
      rollupOptions: {
        input: {
          popup:    resolve(__dirname, 'src/renderer/popup/index.html'),
          settings: resolve(__dirname, 'src/renderer/settings/index.html')
        }
      }
    }
  }
})
