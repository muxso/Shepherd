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
    '/functional-case',
    '/test-plan',
    '/perf',
    '/runner',
    '/runner-agent',
    '/mcp',
  ].map((p) => [p, { target, changeOrigin: true }]),
)

export default defineConfig({
  plugins: [react()],
  server: { port: 5173, proxy },
})
