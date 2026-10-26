import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Built output lands in llm-gateway/static/ and is embedded by rust-embed —
// index.html is served at "/" while every hashed asset goes under /static/*.
// prebuild (package.json) clears ../static/assets + ../static/index.html only;
// ../static/fonts and ../static/sakura-fonts.css are hand-maintained, never wiped.
//
// base is '/' in dev (plain http://127.0.0.1:5173/) and '/static/' for build.
// NOTE: with a non-root base, vite's dev server got confused after a config
// hot-restart (proxy stopped intercepting /api → SPA fallback → empty boards).
export default defineConfig(({ command }) => ({
  base: command === 'build' ? '/static/' : '/',
  plugins: [react()],
  build: {
    outDir: '../static',
    emptyOutDir: false,
    // Android System WebView on older devices: stay conservative.
    target: 'es2019',
  },
  server: {
    // Bind IPv4 loopback explicitly — 'localhost' resolved to ::1 only, which
    // broke browsers that try 127.0.0.1 first.
    host: '127.0.0.1',
    // Dev workflow: `pnpm dev` here + the gateway binary running on :8001.
    proxy: {
      '/api': 'http://127.0.0.1:8001',
      // theme files + self-hosted fonts come from the gateway
      '/themes': 'http://127.0.0.1:8001',
      '/static': 'http://127.0.0.1:8001',
    },
  },
}));
