import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const backend = process.env.HYCLI_DASHBOARD_URL || 'http://127.0.0.1:4318';
const localProxy = () => ({
  target: backend,
  changeOrigin: true,
  configure(proxy: { on: (event: string, handler: (request: { setHeader: (key: string, value: string) => void }) => void) => void }) {
    proxy.on('proxyReq', request => {
      request.setHeader('origin', backend);
      request.setHeader('sec-fetch-site', 'same-origin');
    });
  },
});
export default defineConfig({
  plugins: [react()],
  build: { outDir: '../web', emptyOutDir: true },
  server: { proxy: { '/api': localProxy(), '/guide': localProxy() } },
});
