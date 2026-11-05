import { test as base, expect, type APIRequestContext } from '@playwright/test'
import { RequirementsPage } from './pages/RequirementsPage'
import { TaskCenterPage } from './pages/TaskCenterPage'

// Match the storage keys in api.ts and context.tsx.
const TOKEN_KEY = 'shepherd.token'
const PROJECT_KEY = 'shepherd.projectId'
const PASSWORD = process.env.E2E_PASSWORD || 's3cret'

export async function adminToken(request: APIRequestContext): Promise<string> {
  const r = await request.post('/auth/login', { data: { username: 'admin', password: PASSWORD } })
  expect(r.ok(), 'login 失败:server :9180 起了吗?SHEPHERD_ADMIN_PASSWORD 对吗?').toBeTruthy()
  return (await r.json()).token
}

// Isolate each run in a fresh project to avoid pagination and stale data.
export async function freshProjectId(request: APIRequestContext, token: string): Promise<string> {
  const headers = { authorization: `Bearer ${token}` }
  const orgs = await (await request.get('/organization?pageSize=100', { headers })).json()
  const orgId = orgs.items?.[0]?.id
  expect(orgId, '没有组织 — 先建一个组织').toBeTruthy()
  const r = await request.post('/project', { headers, data: { organizationId: orgId, name: `E2E-${Date.now()}` } })
  expect(r.ok(), '建 E2E 项目失败').toBeTruthy()
  return (await r.json()).id
}

// Create through the API; leave decomposition to the UI.
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
  /** Create a uniquely titled requirement without decomposing it. */
  createReq: (criteria: string[], prefix?: string) => Promise<{ id: string; title: string }>
}

// Inject the API session and selected project before opening pages.
// Expose page objects and a requirement creation helper.
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
