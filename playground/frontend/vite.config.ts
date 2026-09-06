import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Playground is a standalone dev server for iteration speed. In production,
// vakd-rest serves the built static files.
export default defineConfig({
  plugins: [react()],
  // Use relative base so assets load under /playground/, root, or proxy cleanly.
  base: './',
  server: {
    port: 3000,
    proxy: {
      '/health': 'http://localhost:7788',
      '/sessions': 'http://localhost:7788',
      '/playground/rpc': 'http://localhost:7788',
      '/ws': 'http://localhost:7788',
    },
  },
  build: {
    outDir: '../static',
    emptyOutDir: true,
  },
});
