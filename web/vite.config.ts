import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// 后端 Shepherd server 默认 127.0.0.1:9180(见 scripts/api-coverage.sh)。
// dev 时把所有后端面 proxy 过去,前端零 CORS。可用 SHEPHERD_API 覆盖。
const target = process.env.SHEPHERD_API || 'http://127.0.0.1:9180'
// 所有后端面前缀都要在此 proxy,否则 dev 下命中不到的路径会被 Vite 当 SPA 返回 index.html。
const proxy = Object.fromEntries(
  [
    '/api',
    '/auth',
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
    '/functional-case',
    '/test-plan',
    '/perf',
    '/runner',
    '/runner-agent',
    '/case-review',
    '/mcp',
  ].map((p) => [
    p,
    {
      target,
      changeOrigin: true,
      // 实时执行事件 / pool-runner 走同前缀的 WebSocket。
      ws: true,
      // 关键:这些前缀同时是前端路由(/api/definition、/test-plan…)。
      // 浏览器整页导航/刷新带 Accept: text/html → 返回 SPA;只有真正的 API fetch 才转发后端。
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
        // 把体量大的第三方库拆成独立 vendor chunk:入口更小、首屏更快、依赖不变时可长期缓存。
        manualChunks: {
          'vendor-react': ['react', 'react-dom', 'react-router-dom'],
          'vendor-antd': ['antd', '@ant-design/icons'],
        },
      },
    },
  },
})
