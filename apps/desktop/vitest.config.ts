import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test-setup.ts'],
    globals: true,
    // userEvent typing in jsdom is slow on loaded machines; the 5s default made form tests flaky.
    testTimeout: 20000,
  },
});
