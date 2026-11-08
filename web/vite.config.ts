import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Default backend; override with SHEPHERD_API.
const target = process.env.SHEPHERD_API || 'http://127.0.0.1:9180'
// List every API prefix to prevent Vite from returning the SPA for API calls.
const proxy = Object.fromEntries(
  [
    '/api',
    '/auth',
    '/public',
    '/rag',
    '/project',
    '/organization',
    '/system',
    '/role',
    '/user-role',
    '/requirement',
    '/decomposition',
    '/delivery',
    '/verification',
    '/skill',
    '/bug',
    '/comment',
    '/follow',
    '/notice',
    '/functional-case',
    '/test-assets',
    '/test-plan',
    '/perf',
    '/runner',
    '/runner-agent',
    '/case-review',
    '/mcp',
    '/template',
    '/me',
    '/prd-draft',
  ].map((p) => [
    p,
    {
      target,
      changeOrigin: true,
      // Execution events and pool runners share WebSocket routes.
      ws: true,
      // API prefixes also serve frontend routes.
      // Serve the SPA for HTML navigation and proxy API fetches.
      bypass: (req: { headers: Record<string, string | string[] | undefined> }) =>
        (req.headers.accept as string | undefined)?.includes('text/html') ? '/index.html' : undefined,
    },
  ]),
)

export default defineConfig({
  plugins: [react()],
  server: { port: 5173, proxy },
  build: {
    rollupOptions: {
      output: {
        // Cache large vendor chunks separately from application code.
        manualChunks: {
          'vendor-react': ['react', 'react-dom', 'react-router-dom'],
          'vendor-antd': ['antd', '@ant-design/icons'],
        },
      },
    },
  },
})
