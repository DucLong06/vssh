import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import { svelteTesting } from '@testing-library/svelte/vite'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [tailwindcss(), svelte(), svelteTesting()],
  // Tauri prints its own logs to the same terminal and expects this port.
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: { environment: 'jsdom', setupFiles: ['./src/test-setup.ts'] },
})
