import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';

// 2026-08-19 (Day 101 / P7-1.1): Web UI dev server 配置
// - port 3080 (跟 dsh 一致, 跟 tonic 50050 错开)
// - proxy /api/* → http://localhost:50050/* (gRPC-web 桥, 走 tonic-web)
// - 生产 build 输出 ui/dist, 给 ma-harness-server 静态资源用
export default defineConfig({
  plugins: [react()],
  server: {
    port: 3080,
    strictPort: true,
    host: '127.0.0.1',  // dsh 拒绝 0.0.0.0, 我们也一样 (安全设计)
    proxy: {
      '/api': {
        target: 'http://localhost:50050',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
        // gRPC-web 是 HTTP/2 帧, vite proxy 透传即可
      },
    },
  },
  preview: {
    port: 3080,
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    target: 'es2020',
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
});
