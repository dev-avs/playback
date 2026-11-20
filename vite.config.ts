import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  // relative asset paths so Electron can load the build over file://
  base: './',
  plugins: [react()],
  // src-tauri/target holds locked build output that would crash the watcher
  server: { port: 5173, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
})
