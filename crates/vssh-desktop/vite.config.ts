import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import { svelteTesting } from '@testing-library/svelte/vite'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [tailwindcss(), svelte(), svelteTesting()],
  // Tauri prints its own logs to the same terminal and expects this port.
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test-setup.ts'],
    // Any other stylesheet a test imports comes out empty; theme.test.ts reads this one's colours.
    css: { include: /app\.css/ },
    // jsdom cannot draw a terminal: the tests get one that only remembers.
    alias: [{ find: /^@xterm\/(xterm|addon-fit|addon-unicode11)$/, replacement: '/src/lib/fixtures/xterm.ts' }],
  },
})
