# 前端 E2E 冒烟(Playwright)

针对正在运行的 dev 栈跑真实浏览器,覆盖近期核心流:登录、需求→拆分→功能用例覆盖自动生成、
需求列表覆盖率徽标、任务中心加载。专抓 tsc / 组件单测抓不到的**多步集成回归**。

## 前置
1. **后端 server 跑在 `:9180`**(带数据库,admin 可登录)。例:
   ```bash
   DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
   SHEPHERD_BIND=127.0.0.1:9180 SHEPHERD_ADMIN_PASSWORD=s3cret ./target/debug/server
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

## 结构(Page-Object + flow,新用例靠组合)
```
e2e/
  fixtures.ts        # 扩展 test:auth(API 登录+建隔离项目)、注入页面对象、createReq 助手
  flows.ts           # 跨页面/接口的业务流(如 decompose = 打开需求→点自动拆分)
  pages/             # 页面对象(选择器/交互集中,spec 不碰 DOM 细节)
    LoginPage.ts  RequirementsPage.ts  TaskCenterPage.ts
  *.spec.ts          # 用例 = 组合 fixtures + flow + page-object 的断言
```

加新用例示范:
```ts
import { test, expect } from './fixtures'
import { decompose } from './flows'

test('xxx', async ({ requirements, createReq }) => {
  const { title } = await createReq(['标准A', '标准B'])   // API 建需求(隔离项目)
  await decompose(requirements, title)                     // flow:打开 + 自动拆分
  await requirements.openCoverageTab()
  await requirements.expectCoverage(2, 2)                  // page-object 断言
})
```
加新页面/流程:在 `pages/` 加页面对象、`flows.ts` 加业务流,spec 只管组合 + 断言。

## 说明
- 用例经 API 登录 + 注入 localStorage(`shepherd.token` / `shepherd.projectId`)免去重复点登录;
  `login.spec.ts` 例外,专测登录 UI 本身。
- 每次跑建一个全新隔离项目(fixture `auth`),需求只在该项目里,不被历史数据/分页淹没。
- 冒烟会建少量「E2E-」前缀的测试需求/功能用例 + E2E 项目(dogfood 库可接受);需要的话自行清理。
- **不触发真实 Claude 派发**(那会慢且改仓库);派发→执行链路用后端 e2e / 手动验证。
