import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 4801,
    proxy: {
      '/services': 'http://localhost:4800',
      '/health': 'http://localhost:4800',
    },
  },
});
