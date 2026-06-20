// Shepherd 后端 REST 客户端 + 类型(镜像 Rust DTO,camelCase)。
// 所有请求带 Bearer token;401 触发登出回调。dev 经 Vite proxy → :9180。

const TOKEN_KEY = 'shepherd.token'

export const tokenStore = {
  get: () => localStorage.getItem(TOKEN_KEY) || '',
  set: (t: string) => localStorage.setItem(TOKEN_KEY, t),
  clear: () => localStorage.removeItem(TOKEN_KEY),
}

let onUnauthorized: (() => void) | null = null
export const setUnauthorizedHandler = (fn: () => void) => {
  onUnauthorized = fn
}

export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {}
  const token = tokenStore.get()
  if (token) headers['authorization'] = `Bearer ${token}`
  if (body !== undefined) headers['content-type'] = 'application/json'

  const res = await fetch(path, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })

  if (res.status === 401) {
    onUnauthorized?.()
    throw new ApiError(401, '登录已失效,请重新登录')
  }

  const text = await res.text()
  if (!res.ok) {
    throw new ApiError(res.status, text || `${res.status} ${res.statusText}`)
  }
  if (!text) return undefined as T
  // 2xx 但返回 HTML(典型:dev 代理未命中 → Vite 回 index.html)。
  // 早期直接抛出清晰错误,避免把 HTML 字符串当数据塞进 state 引发白屏。
  const ct = res.headers.get('content-type') || ''
  if (ct.includes('text/html') || text.trimStart().startsWith('<')) {
    throw new ApiError(
      502,
      '后端未连通:收到 HTML 而非 JSON。请确认 server 跑在 :9180,并重启前端 dev(npm run dev)使代理生效。',
    )
  }
  try {
    return JSON.parse(text) as T
  } catch {
    throw new ApiError(500, `响应不是合法 JSON:${text.slice(0, 120)}`)
  }
}

export const http = {
  get: <T>(p: string) => request<T>('GET', p),
  post: <T>(p: string, b?: unknown) => request<T>('POST', p, b),
  put: <T>(p: string, b?: unknown) => request<T>('PUT', p, b),
  del: <T>(p: string, b?: unknown) => request<T>('DELETE', p, b),
}

// ---------- 类型 ----------

export interface Page<T> {
  total: number
  current: number
  pageSize: number
  totalPages: number
  items: T[]
}

export interface Organization {
  id: string
  name: string
}

export interface Project {
  id: string
  organizationId: string
  name: string
  enable: boolean
}

export interface ApiDefinition {
  id: string
  projectId: string
  name: string
  protocol: string
  method: string
  path: string
  status: string
}

export interface ApiCase {
  id: string
  apiDefinitionId: string
  projectId: string
  name: string
  method: string
  url: string
  body: string | null
  assertions: unknown
  processors: unknown
}

export interface ApiMock {
  id: string
  apiDefinitionId: string
  name: string
  matchRule: unknown
  responseStatus: number
  responseBody: string | null
  enabled: boolean
}

export interface CaseExecution {
  reportId: string
  caseId: string
  outcome: string
  failures: unknown
  executedAt: string
}

export interface Scenario {
  id: string
  projectId: string
  name: string
  status: string
}

export interface ScenarioStep {
  id: string
  order: number
  kind: string
  refMode: string
  caseId?: string | null
  scenarioId?: string | null
  request?: { method: string; url: string; body?: string | null; assertions?: unknown } | null
  control?: unknown
  snapshot?: unknown
}

export interface ResourcePool {
  id: string
  name: string
  enabled: boolean
}

export type RunMode = 'PARALLEL' | 'SERIAL'

// ---------- 端点封装 ----------

export const api = {
  login: (username: string, password: string) =>
    http.post<{ token: string }>('/auth/login', { username, password }),

  organizations: () => http.get<Page<Organization>>('/organization?pageSize=100'),
  createOrganization: (name: string) => http.post<Organization>('/organization', { name }),
  projects: (organizationId: string) =>
    http.get<Page<Project>>(`/project?organizationId=${encodeURIComponent(organizationId)}&pageSize=100`),
  createProject: (organizationId: string, name: string) =>
    http.post<Project>('/project', { organizationId, name }),

  definitions: (projectId: string) =>
    http.get<ApiDefinition[]>(`/api/definition?projectId=${encodeURIComponent(projectId)}`),
  getDefinition: (id: string) => http.get<ApiDefinition>(`/api/definition/${id}`),
  createDefinition: (b: {
    projectId: string
    name: string
    protocol?: string
    method?: string
    path?: string
  }) => http.post<ApiDefinition>('/api/definition', b),
  importDefinitions: (projectId: string, content: unknown) =>
    http.post<{ created: ApiDefinition[]; skipped: number }>('/api/definition/import', {
      projectId,
      content,
    }),

  cases: (definitionId: string) =>
    http.get<ApiCase[]>(`/api/definition/${definitionId}/case`),
  createCase: (
    definitionId: string,
    b: { name: string; method: string; url: string; body?: string; assertions?: unknown; processors?: unknown },
  ) => http.post<ApiCase>(`/api/definition/${definitionId}/case`, b),
  runCase: (caseId: string, projectId: string, runMode: RunMode, poolId?: string) =>
    http.post<{ reportId: string; status: string }>(`/api/case/${caseId}/run`, {
      projectId,
      runMode,
      poolId,
    }),

  resourcePools: () => http.get<ResourcePool[]>('/api/resource-pool'),
  createResourcePool: (name: string, enabled = true) =>
    http.post<ResourcePool>('/api/resource-pool', { name, enabled }),
  caseExecutions: (caseId: string) =>
    http.get<Page<CaseExecution>>(`/api/case/${caseId}/executions?pageSize=50`),

  mocks: (definitionId: string) => http.get<ApiMock[]>(`/api/definition/${definitionId}/mock`),
  createMock: (
    definitionId: string,
    b: { name: string; matchRule?: unknown; responseStatus?: number; responseBody?: string; enabled?: boolean },
  ) => http.post<ApiMock>(`/api/definition/${definitionId}/mock`, b),

  scenarios: (projectId: string) =>
    http.get<Scenario[]>(`/api/scenario?projectId=${encodeURIComponent(projectId)}`),
  getScenario: (id: string) => http.get<Scenario & { steps: ScenarioStep[] }>(`/api/scenario/${id}`),
  createScenario: (projectId: string, name: string) =>
    http.post<Scenario>('/api/scenario', { projectId, name }),
  addStep: (
    scenarioId: string,
    b: { kind: string; order: number; refId?: string; request?: unknown; control?: unknown },
  ) => http.post<ScenarioStep>(`/api/scenario/${scenarioId}/step`, b),
  runScenario: (scenarioId: string, projectId: string) =>
    http.post<unknown>(`/api/scenario/${scenarioId}/run`, { projectId }),
}
