import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Built output lands in llm-gateway/static/ and is embedded by rust-embed —
// index.html is served at "/" while every hashed asset goes under /static/*.
// prebuild (package.json) clears ../static/assets + ../static/index.html only;
// ../static/fonts and ../static/sakura-fonts.css are hand-maintained, never wiped.
export default defineConfig({
  base: '/static/',
  plugins: [react()],
  build: {
    outDir: '../static',
    emptyOutDir: false,
    // Android System WebView on older devices: stay conservative.
    target: 'es2019',
  },
  server: {
    // Dev workflow: `pnpm dev` here + the gateway binary running on :8001.
    proxy: {
      '/api': 'http://127.0.0.1:8001',
      '/static': 'http://127.0.0.1:8001',
    },
  },
});
