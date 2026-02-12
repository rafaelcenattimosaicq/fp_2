import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  define: {
    'import.meta.env.VITE_DEMO_MODE': JSON.stringify('true'),
    'import.meta.env.VITE_COGNITO_USER_POOL_ID': JSON.stringify('us-east-1_TESTPOOL'),
    'import.meta.env.VITE_COGNITO_CLIENT_ID': JSON.stringify('test-client-id-v2'),
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: [],
    include: ['src/**/*.test.{ts,tsx}', 'src/**/*.spec.{ts,tsx}'],
    exclude: ['src/**/*.integration.test.{ts,tsx}', 'src/features/dashboard/*.test.*', 'node_modules/**'],
    passWithNoTests: false,
  },
});
