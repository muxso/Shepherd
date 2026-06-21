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

// 原始文本 GET(用于 Markdown 报告等非 JSON 响应)。
async function requestText(path: string): Promise<string> {
  const headers: Record<string, string> = {}
  const token = tokenStore.get()
  if (token) headers['authorization'] = `Bearer ${token}`
  const res = await fetch(path, { headers })
  if (res.status === 401) {
    onUnauthorized?.()
    throw new ApiError(401, '登录已失效,请重新登录')
  }
  const text = await res.text()
  if (!res.ok) throw new ApiError(res.status, text || `${res.status}`)
  return text
}

export const http = {
  get: <T>(p: string) => request<T>('GET', p),
  getText: (p: string) => requestText(p),
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
  moduleId?: string | null
}

export interface ApiModule {
  id: string
  projectId: string
  parentId?: string | null
  name: string
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

export interface ScenarioRunResult {
  reportId: string
  status: string
  caseCount: number
}

export interface ScenarioExecution {
  id: string
  scenarioId: string
  projectId: string
  status: string
  caseCount: number
  reportId: string
  createdAt: string
}

export interface ResourcePool {
  id: string
  name: string
  enabled: boolean
}

export interface Role {
  id: string
  name: string
  permissions?: string[]
}

export interface User {
  id: string
  name: string
  email: string
  enable?: boolean
}

export interface CaseStep {
  step: string
  expected: string
}

export interface FunctionalCase {
  id: string
  projectId: string
  name: string
  module?: string
  priority?: string
  status?: string
  steps?: CaseStep[]
  customFields?: Record<string, string>
}

export interface EnvHeader {
  name: string
  value: string
}

export interface Environment {
  id: string
  projectId?: string
  name: string
  baseUrl: string
  headers?: EnvHeader[]
  variables?: Record<string, string>
  enabled?: boolean
  protocols?: string[]
}

export interface EnvironmentBody {
  projectId: string
  name: string
  baseUrl: string
  headers?: EnvHeader[]
  variables?: Record<string, string>
  enabled?: boolean
}

export interface TestPlan {
  id: string
  projectId: string
  name: string
  planType?: string
}

export interface PlanStats {
  status: string
  total: number
  passRate: number
  executeRate: number
  isPass: boolean
}

export interface PlanCase {
  caseId: string
  name: string
  status: string
  latencyMs?: number | null
  statusCode?: number | null
}

export interface PerfLatency {
  min: number
  max: number
  mean: number
  p50: number
  p90: number
  p95: number
  p99: number
}

export interface PerfReport {
  id: string
  projectId: string
  method: string
  url: string
  concurrency: number
  iterations: number
  total: number
  success: number
  failed: number
  errorRate: number
  throughputRps: number
  elapsedMs: number
  status: string
  latency: PerfLatency
}

export interface RequirementVersion {
  version: number
  description?: string
  acceptanceCriteria?: string[]
}

export interface Requirement {
  id: string
  projectId?: string
  title: string
  baselineVersion: number
  latestVersion?: number
  status: string
  // 兼容两种返回:顶层 acceptanceCriteria(旧)或 versions[].acceptanceCriteria(现)。
  acceptanceCriteria?: string[]
  versions?: RequirementVersion[]
}

export interface Task {
  id: string
  title: string
  status: string
  acceptanceCriteria?: string[]
  dependencies?: string[]
}

export interface Decomposition {
  id: string
  tasks: Task[]
  verificationId?: string
}

export interface DeliveryAttempt {
  id?: string
  attemptId?: string
  status: string
  taskId?: string
  title?: string
  executor?: string
}

export interface DeliveryEvent {
  seq?: number
  kind: string
  message?: string
  detail?: unknown
}

export interface VerificationReport {
  satisfied?: number
  complete?: boolean
  [k: string]: unknown
}

export interface Bug {
  id: string
  title?: string
  status: string
}

export interface McpTool {
  name: string
  description?: string
}

export interface DebugResponse {
  status: number
  latencyMs: number
  headers: [string, string][]
  body: string
}

export type RunMode = 'PARALLEL' | 'SERIAL'

// ---------- 端点封装 ----------

const emptyPage = <T>(): Page<T> => ({ total: 0, current: 1, pageSize: 0, totalPages: 0, items: [] })

export const api = {
  login: (username: string, password: string) =>
    http.post<{ token: string }>('/auth/login', { username, password }),

  organizations: () => http.get<Page<Organization>>('/organization?pageSize=100'),
  createOrganization: (name: string) => http.post<Organization>('/organization', { name }),
  projects: (organizationId: string) =>
    organizationId
      ? http.get<Page<Project>>(`/project?organizationId=${encodeURIComponent(organizationId)}&pageSize=100`)
      : Promise.resolve(emptyPage<Project>()),
  createProject: (organizationId: string, name: string) =>
    http.post<Project>('/project', { organizationId, name }),

  definitions: (projectId: string) =>
    projectId
      ? http.get<ApiDefinition[]>(`/api/definition?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as ApiDefinition[]),
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

  // 接口模块(文件夹)
  modules: (projectId: string) =>
    projectId
      ? http.get<ApiModule[]>(`/api/module?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as ApiModule[]),
  createModule: (b: { projectId: string; parentId?: string | null; name: string }) =>
    http.post<ApiModule>('/api/module', b),
  renameModule: (id: string, name: string) => http.put(`/api/module/${id}`, { name }),
  deleteModule: (id: string) => http.del(`/api/module/${id}`),
  moveDefinition: (definitionId: string, moduleId: string | null) =>
    http.put(`/api/definition/${definitionId}/module`, { moduleId }),

  // 任务 ↔ 用例 关联(链路:任务→用例)
  taskCases: (decompositionId: string, taskId: string) =>
    http.get<ApiCase[]>(`/api/task-case?decompositionId=${encodeURIComponent(decompositionId)}&taskId=${encodeURIComponent(taskId)}`),
  linkTaskCase: (decompositionId: string, taskId: string, caseId: string) =>
    http.post('/api/task-case', { decompositionId, taskId, caseId }),
  unlinkTaskCase: (decompositionId: string, taskId: string, caseId: string) =>
    http.post('/api/task-case/unlink', { decompositionId, taskId, caseId }),
  // 用例 → 计划 反查(链路:用例→计划)
  plansByCase: (caseId: string) => http.get<{ planId: string; name: string }[]>(`/test-plan/by-case/${caseId}`),

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

  // 角色 / 用户(平台级)
  roles: () => http.get<Page<Role>>('/role?pageSize=100'),
  createRole: (b: { name: string; permissions?: string[] }) => http.post<Role>('/role', b),
  users: () => http.get<Page<User>>('/system/user?pageSize=100'),
  createUser: (b: { name: string; email: string }) => http.post<User>('/system/user', b),

  // 功能用例(项目级)
  functionalCases: (projectId: string) =>
    projectId
      ? http.get<FunctionalCase[]>(`/functional-case?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as FunctionalCase[]),
  createFunctionalCase: (b: {
    projectId: string
    name: string
    priority?: string
    module?: string
    steps?: CaseStep[]
    customFields?: Record<string, string>
  }) => http.post<FunctionalCase>('/functional-case', b),

  // 项目接口用例(供测试计划挂载选择)
  projectCases: (projectId: string) =>
    projectId
      ? http.get<Page<ApiCase>>(`/api/case?projectId=${encodeURIComponent(projectId)}&pageSize=100`)
      : Promise.resolve(emptyPage<ApiCase>()),

  // 测试计划(无 list 端点 → 列表由前端注册表维护)
  createPlan: (b: { projectId: string; name: string; type?: string }) =>
    http.post<TestPlan>('/test-plan', { type: 'TEST_PLAN', ...b }),
  planStats: (id: string) => http.get<PlanStats>(`/test-plan/${id}/statistics`),
  planCases: (id: string) => http.get<PlanCase[] | Page<PlanCase>>(`/test-plan/${id}/cases`),
  linkPlanCase: (id: string, caseId: string, name: string) =>
    http.post(`/test-plan/${id}/cases`, { caseId, name }),
  runPlan: (id: string, environmentId?: string) =>
    http.post<{ status?: string; total: number; executed: number }>(`/test-plan/${id}/run`, { environmentId }),
  planSchedule: (id: string, cron: string) => http.post(`/test-plan/${id}/schedule`, { cron }),
  planRuns: (id: string) => http.get<unknown[]>(`/test-plan/${id}/runs`),
  planReportMd: (id: string) => http.getText(`/test-plan/${id}/report.md`),

  // 性能压测(无 list 端点 → 报告列表由前端注册表维护)
  runPerf: (b: {
    projectId: string
    method: string
    url: string
    concurrency: number
    iterations: number
  }) => http.post<{ reportId: string; status: string }>('/perf/run', b),
  perfReport: (id: string) => http.get<PerfReport>(`/perf/report/${id}`),

  // 需求(版本 / 基线 / 拆分)— 无 list 端点,列表用前端注册表
  createRequirement: (b: { projectId: string; title: string; acceptanceCriteria: string[] }) =>
    http.post<Requirement>('/requirement', b),
  getRequirement: (id: string) => http.get<Requirement>(`/requirement/${id}`),
  // 列表走后端(分页),pageSize 取大值一次拉全 —— 让 CLI/API 建的需求也在 UI 可见。
  requirements: (projectId: string) =>
    http.get<Page<Requirement>>(`/requirement?projectId=${encodeURIComponent(projectId)}&current=1&pageSize=200`),
  addRequirementVersion: (id: string, b: { description: string; acceptanceCriteria: string[] }) =>
    http.post<{ version: number }>(`/requirement/${id}/version`, b),
  setBaseline: (id: string, version: number) =>
    http.put<Requirement>(`/requirement/${id}/baseline`, { version }),
  breakdown: (id: string) =>
    http.post<{ id: string; verificationId: string; tasks: Task[] }>(`/requirement/${id}/breakdown`, {}),

  // 拆分图 / 任务
  decomposition: (id: string) => http.get<Decomposition>(`/decomposition/${id}`),
  decompositionReady: (id: string) => http.get<Task[]>(`/decomposition/${id}/ready`),
  addTask: (id: string, b: { title: string; acceptanceCriteria: string[]; dependencies: string[] }) =>
    http.post<{ taskId: string }>(`/decomposition/${id}/task`, b),
  runDecomposition: (id: string) =>
    http.post<{ total: number; verified: number; failed: number; blocked: number; rounds: number }>(
      `/decomposition/${id}/run`,
      {},
    ),

  // 交付
  createDelivery: (b: { decompositionId: string; taskId: string; title: string; executor: string }) =>
    http.post<DeliveryAttempt>('/delivery', b),
  deliveries: (decompositionId: string, taskId: string) =>
    http.get<DeliveryAttempt[]>(
      `/delivery?decompositionId=${encodeURIComponent(decompositionId)}&taskId=${encodeURIComponent(taskId)}`,
    ),
  deliveryEvents: (attemptId: string) => http.get<DeliveryEvent[]>(`/delivery/${attemptId}/events`),

  // 验证(覆盖链 / 报告)
  verificationReport: (id: string) => http.get<VerificationReport>(`/verification/${id}/report`),
  verificationLink: (id: string, b: { criterionIndex: number; decompositionId: string; taskId: string }) =>
    http.post(`/verification/${id}/link`, b),
  verificationSync: (id: string, b: { decompositionId: string; taskId: string; satisfied: boolean }) =>
    http.post(`/verification/${id}/sync`, b),

  // 缺陷 — 无 list 端点,列表用前端注册表
  createBug: (b: { projectId: string; title: string; initialStatus: string }) => http.post<Bug>('/bug', b),
  setBugStatus: (id: string, status: string) => http.post<Bug>(`/bug/${id}/status`, { status }),

  // 技能 — 无 list 端点,列表用前端注册表
  createSkill: (b: { projectId: string; name: string; instructions: string }) =>
    http.post<{ id: string }>('/skill', b),
  composeSkills: (projectId: string, skillIds: string[]) =>
    http.post<{ instructions: string }>('/skill/compose', { projectId, skillIds }),

  // 接口调试台:进程内即时发起请求(POST /api/debug/send)
  debugSend: (b: { protocol?: string; method: string; url: string; headers?: { key: string; value: string }[]; body?: string; meta?: Record<string, string> }) =>
    http.post<DebugResponse>('/api/debug/send', b),
  // 后端启用的协议插件(即插即用:开了哪个 feature 就返回哪个),供调试台动态渲染。
  debugProtocols: () => http.get<{ protocols: string[] }>('/api/debug/protocols'),

  // MCP 工具
  mcpTools: () =>
    http.post<{ result: { tools: McpTool[] } }>('/mcp', { jsonrpc: '2.0', id: 1, method: 'tools/list' }),

  // 环境(项目级)
  environments: (projectId: string) =>
    projectId
      ? http.get<Environment[]>(`/api/environment?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as Environment[]),
  createEnvironment: (b: EnvironmentBody) => http.post<Environment>('/api/environment', b),
  updateEnvironment: (id: string, b: EnvironmentBody) => http.put<Environment>(`/api/environment/${id}`, b),
  caseExecutions: (caseId: string) =>
    http.get<Page<CaseExecution>>(`/api/case/${caseId}/executions?pageSize=50`),

  mocks: (definitionId: string) => http.get<ApiMock[]>(`/api/definition/${definitionId}/mock`),
  createMock: (
    definitionId: string,
    b: { name: string; matchRule?: unknown; responseStatus?: number; responseBody?: string; enabled?: boolean },
  ) => http.post<ApiMock>(`/api/definition/${definitionId}/mock`, b),

  scenarios: (projectId: string) =>
    projectId
      ? http.get<Scenario[]>(`/api/scenario?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as Scenario[]),
  getScenario: (id: string) => http.get<Scenario & { steps: ScenarioStep[] }>(`/api/scenario/${id}`),
  createScenario: (projectId: string, name: string) =>
    http.post<Scenario>('/api/scenario', { projectId, name }),
  addStep: (
    scenarioId: string,
    b: { kind: string; order: number; refId?: string; request?: unknown; control?: unknown },
  ) => http.post<ScenarioStep>(`/api/scenario/${scenarioId}/step`, b),
  runScenario: (scenarioId: string, projectId: string) =>
    http.post<ScenarioRunResult>(`/api/scenario/${scenarioId}/run`, { projectId }),
  scenarioExecutions: (scenarioId: string) =>
    http.get<Page<ScenarioExecution>>(`/api/scenario/${scenarioId}/executions`),
}
