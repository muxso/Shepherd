import { test as base, expect, type APIRequestContext } from '@playwright/test'
import { RequirementsPage } from './pages/RequirementsPage'
import { TaskCenterPage } from './pages/TaskCenterPage'

// 与 src/api.ts、src/context.tsx 对齐的 localStorage 键。
const TOKEN_KEY = 'shepherd.token'
const PROJECT_KEY = 'shepherd.projectId'
const PASSWORD = process.env.E2E_PASSWORD || 's3cret'

export async function adminToken(request: APIRequestContext): Promise<string> {
  const r = await request.post('/auth/login', { data: { username: 'admin', password: PASSWORD } })
  expect(r.ok(), 'login 失败:server :9180 起了吗?MS_ADMIN_PASSWORD 对吗?').toBeTruthy()
  return (await r.json()).token
}

// 每次运行建一个全新隔离项目 —— 列表里只有本次 E2E 的需求(不被海量历史数据/分页淹没)。
export async function freshProjectId(request: APIRequestContext, token: string): Promise<string> {
  const headers = { authorization: `Bearer ${token}` }
  const orgs = await (await request.get('/organization?pageSize=100', { headers })).json()
  const orgId = orgs.items?.[0]?.id
  expect(orgId, '没有组织 — 先建一个组织').toBeTruthy()
  const r = await request.post('/project', { headers, data: { organizationId: orgId, name: `E2E-${Date.now()}` } })
  expect(r.ok(), '建 E2E 项目失败').toBeTruthy()
  return (await r.json()).id
}

// 经 API 建需求(不拆分;拆分留给 UI 点「自动拆分」以测真实流程),返回 requirementId。
export async function createRequirement(
  request: APIRequestContext,
  token: string,
  projectId: string,
  title: string,
  criteria: string[],
): Promise<string> {
  const headers = { authorization: `Bearer ${token}` }
  const r = await request.post('/requirement', { headers, data: { projectId, title, acceptanceCriteria: criteria } })
  expect(r.ok(), '建需求失败').toBeTruthy()
  return (await r.json()).id
}

type Fixtures = {
  auth: { token: string; projectId: string }
  requirements: RequirementsPage
  taskCenter: TaskCenterPage
  /** 经 API 建需求(不拆分),返回 {id, title};标题唯一。 */
  createReq: (criteria: string[], prefix?: string) => Promise<{ id: string; title: string }>
}

// authed:API 登录 + 注入 localStorage(token + 选中项目),页面打开即已登录、已选项目。
// 并注入页面对象(requirements/taskCenter)与 createReq 助手,spec 直接拿来组合。
export const test = base.extend<Fixtures>({
  auth: async ({ request }, use) => {
    const token = await adminToken(request)
    const projectId = await freshProjectId(request, token)
    await use({ token, projectId })
  },
  page: async ({ page, auth }, use) => {
    await page.addInitScript(
      ([t, p]) => {
        localStorage.setItem('shepherd.token', t)
        localStorage.setItem('shepherd.projectId', p)
      },
      [auth.token, auth.projectId] as [string, string],
    )
    await use(page)
  },
  requirements: async ({ page }, use) => {
    await use(new RequirementsPage(page))
  },
  taskCenter: async ({ page }, use) => {
    await use(new TaskCenterPage(page))
  },
  createReq: async ({ request, auth }, use) => {
    let n = 0
    await use(async (criteria, prefix = 'E2E') => {
      const title = `${prefix}-${Date.now()}-${n++}`
      const id = await createRequirement(request, auth.token, auth.projectId, title, criteria)
      return { id, title }
    })
  },
})

export { expect, TOKEN_KEY, PROJECT_KEY }
