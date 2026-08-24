import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['src/__tests__/**/*.test.ts'],
    exclude: ['src/__tests__/e2e/**', 'src/__tests__/*-e2e.test.ts'],
    testTimeout: 15000,
    hookTimeout: 15000,
  },
});
