# 前端 E2E 冒烟(Playwright)

针对正在运行的 dev 栈跑真实浏览器,覆盖近期核心流:登录、需求→拆分→功能用例覆盖自动生成、
需求列表覆盖率徽标、任务中心加载。专抓 tsc / 组件单测抓不到的**多步集成回归**。

## 前置
1. **后端 server 跑在 `:9180`**(带数据库,admin 可登录)。例:
   ```bash
   DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
   MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret ./target/debug/server
   ```
2. 至少有 1 个组织 + 1 个项目(冒烟用例需要选中项目)。
3. vite 会由 Playwright 的 `webServer` 自动起(已在 :5173 跑则复用)。

## 首次安装
```bash
cd web
npm install                 # 装 @playwright/test
npx playwright install chromium
```

## 运行
```bash
npm run e2e                 # 无头跑全部
npm run e2e:ui              # 带 UI 调试器
npx playwright test e2e/requirements.spec.ts   # 单文件
```

可用环境变量覆盖:`E2E_BASE_URL`(默认 http://localhost:5173)、`E2E_PASSWORD`(默认 s3cret)。

## 说明
- 用例经 API 登录 + 注入 localStorage(`shepherd.token` / `shepherd.projectId`)免去重复点登录;
  `login.spec.ts` 例外,专测登录 UI 本身。
- 冒烟会建少量「E2E-」前缀的测试需求/功能用例(dogfood 库可接受);需要的话自行清理。
- **不触发真实 Claude 派发**(那会慢且改仓库);派发→执行链路用后端 e2e / 手动验证。
