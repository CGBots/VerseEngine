import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// En développement, l'API Rust (ACTIVITY_BIND, défaut 127.0.0.1:8787) est proxifiée,
// comme le fera le proxy Discord en production (même origine).
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    // Nécessaire derrière un tunnel (cloudflared) pendant le développement.
    allowedHosts: true,
    proxy: {
      '/api': { target: 'http://127.0.0.1:8787', changeOrigin: true, ws: true },
    },
  },
  build: { target: 'es2022', sourcemap: false, chunkSizeWarningLimit: 900 },
});
