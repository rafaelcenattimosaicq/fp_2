import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['src/**/*.integration.test.{ts,tsx}'],
    testTimeout: 30_000,
    hookTimeout: 60_000,
    sequence: { concurrent: false },
  },
});
