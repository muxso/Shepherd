// Shepherd 后端 REST 客户端 + 类型(镜像 Rust DTO,camelCase)。
// 所有请求带 Bearer token;401 触发登出回调。dev 经 Vite proxy → :9180。

const TOKEN_KEY = 'shepherd.token'
const USER_KEY = 'shepherd.user'

export const tokenStore = {
  get: () => localStorage.getItem(TOKEN_KEY) || '',
  set: (t: string) => localStorage.setItem(TOKEN_KEY, t),
  clear: () => localStorage.removeItem(TOKEN_KEY),
}

// 当前登录用户名:登录时写入,供个人中心 / 创建人列等展示(后端暂无 /me)。
// 单一来源,避免各页对 localStorage key 与兜底值各写一份导致漂移。
export const userStore = {
  get: () => localStorage.getItem(USER_KEY) || 'admin',
  set: (u: string) => localStorage.setItem(USER_KEY, u),
  clear: () => localStorage.removeItem(USER_KEY),
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

// 原始二进制 GET(用于 xlsx 导出等文件下载)。
async function requestBlob(path: string): Promise<Blob> {
  const headers: Record<string, string> = {}
  const token = tokenStore.get()
  if (token) headers['authorization'] = `Bearer ${token}`
  const res = await fetch(path, { headers })
  if (res.status === 401) {
    onUnauthorized?.()
    throw new ApiError(401, '登录已失效,请重新登录')
  }
  if (!res.ok) throw new ApiError(res.status, (await res.text()) || `${res.status}`)
  return res.blob()
}

// 原始字节 POST(用于 xlsx 导入上传,body 直接是文件)。
async function requestUpload<T>(path: string, body: Blob): Promise<T> {
  const headers: Record<string, string> = { 'content-type': 'application/octet-stream' }
  const token = tokenStore.get()
  if (token) headers['authorization'] = `Bearer ${token}`
  const res = await fetch(path, { method: 'POST', headers, body })
  if (res.status === 401) {
    onUnauthorized?.()
    throw new ApiError(401, '登录已失效,请重新登录')
  }
  const text = await res.text()
  if (!res.ok) throw new ApiError(res.status, text || `${res.status}`)
  return (text ? JSON.parse(text) : undefined) as T
}

export const http = {
  get: <T>(p: string) => request<T>('GET', p),
  getText: (p: string) => requestText(p),
  getBlob: (p: string) => requestBlob(p),
  upload: <T>(p: string, body: Blob) => requestUpload<T>(p, body),
  post: <T>(p: string, b?: unknown) => request<T>('POST', p, b),
  put: <T>(p: string, b?: unknown) => request<T>('PUT', p, b),
  patch: <T>(p: string, b?: unknown) => request<T>('PATCH', p, b),
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
  enable?: boolean
}

export interface Project {
  id: string
  organizationId: string
  name: string
  enable: boolean
}

/** 请求/响应规格(「定义」标签):请求头/Query/Body + 响应样例。 */
export interface ApiSpecKV {
  name: string
  value?: string
  desc?: string
}
export interface ApiSpecResponse {
  status?: number
  body?: string
  /** 示例响应头(「响应内容」里「响应头」标签)。 */
  headers?: ApiSpecKV[]
}
/** 请求体 content-type。 */
export type ApiBodyType = 'none' | 'form-data' | 'x-www-form-urlencoded' | 'json' | 'xml' | 'raw' | 'binary'

/** body 类型 → 默认 Content-Type;none 无体、raw 不强加(由用户/请求头决定)→ undefined。 */
export function contentTypeForBodyType(t?: ApiBodyType): string | undefined {
  switch (t) {
    case 'json':
      return 'application/json'
    case 'xml':
      return 'application/xml'
    case 'form-data':
      // multipart 需 boundary 参数,而编辑器只提供原始文本体;不自动附加(无 boundary 的头反而让服务端解析失败),交由用户在请求头自定义。
      return undefined
    case 'x-www-form-urlencoded':
      return 'application/x-www-form-urlencoded'
    case 'binary':
      return 'application/octet-stream'
    default:
      // none / raw / 未指定:不自动附加,Content-Type 可选。
      return undefined
  }
}

/**
 * 按 body 类型为请求头补一个默认 Content-Type —— 但「可选」:
 * 用户已在请求头里写了 Content-Type(大小写不敏感)时尊重用户的,不覆盖;
 * body 类型为 none/raw 时不附加。返回新数组,不改入参。
 */
export function withBodyContentType<T extends { key: string; value: string }>(
  headers: T[],
  bodyType?: ApiBodyType,
): T[] {
  const ct = contentTypeForBodyType(bodyType)
  if (!ct) return headers
  if (headers.some((h) => h.key.trim().toLowerCase() === 'content-type')) return headers
  return [...headers, { key: 'Content-Type', value: ct } as T]
}
/** 认证模式。 */
export interface ApiSpecAuth {
  type?: 'none' | 'bearer' | 'basic'
  token?: string
}
/** JSON 请求体 Schema 树节点(Schema 表格)。 */
export interface BodySchemaNode {
  name: string
  type: 'string' | 'integer' | 'number' | 'boolean' | 'object' | 'array'
  value?: string
  description?: string
  children?: BodySchemaNode[]
}

export interface ApiSpec {
  // 基本信息
  description?: string
  tags?: string[]
  // 请求
  requestHeaders?: ApiSpecKV[]
  requestQuery?: ApiSpecKV[]
  /** REST 路径参数(/users/{id} 里的 {id})。 */
  restParams?: ApiSpecKV[]
  bodyType?: ApiBodyType
  requestBody?: string
  /** form-data / urlencoded 的键值体。 */
  formBody?: ApiSpecKV[]
  /** JSON 请求体的 Schema 树(参数名/类型/值/描述 + 嵌套);与 requestBody 文本并存。 */
  bodySchema?: BodySchemaNode[]
  auth?: ApiSpecAuth
  responses?: ApiSpecResponse[]
  /** 前置/后置处理器(api-runner Processor JSON;wait/extract/script/sql)。 */
  preProcessors?: unknown[]
  postProcessors?: unknown[]
  /** 断言(api-runner Assertion JSON)。 */
  assertions?: unknown[]
}

export interface ApiDefinition {
  id: string
  /** 人类可读编号(【101093】式 ID;见 0042 迁移)。 */
  num?: number
  projectId: string
  name: string
  protocol: string
  method: string
  path: string
  status: string
  moduleId?: string | null
  /** 不透明 JSON 规格;服务端往返存取(见 0037 迁移)。 */
  spec?: ApiSpec
  /** 创建人 user_id(见 0039 迁移)。 */
  createdBy?: string
  /** 创建/更新时间(服务端文本承载,如 "2026-06-21 12:34:56+00")。 */
  createdAt?: string
  updatedAt?: string
}

/** 接口列表视图(保存的筛选/列/分页快照;config 为不透明 JSON)。 */
export interface ApiView {
  id: string
  projectId: string
  userId: string
  name: string
  config: Record<string, unknown>
  shared: boolean
  createdAt: string
}

export interface ApiModule {
  id: string
  projectId: string
  parentId?: string | null
  name: string
}

/** 支持的导入来源格式。 */
export type ImportFormat = 'openapi' | 'postman' | 'har' | 'jmeter' | 'metersphere'

/** 定时导入计划(后端不回吐 token)。 */
export interface ImportSchedule {
  id: string
  projectId: string
  name: string
  format: ImportFormat
  sourceUrl: string
  basicAuth: boolean
  moduleId?: string | null
  groupByTag: boolean
  overwrite: boolean
  syncModule: boolean
  cron: string
  enabled: boolean
  lastRunAt?: string | null
  lastResult: string
  /** 最近一次运行的操作人(手动「立即执行」记触发用户;cron 自动运行为空 → 前端展示「自动」;见 0062 迁移)。 */
  lastRunBy?: string
  /** 创建人 user_id(后端审计列;见 0061 迁移)。 */
  createdBy: string
  createdAt: string
}

/** URL 导入 / 定时导入共用的导入选项。 */
export interface ImportOpts {
  format?: ImportFormat
  moduleId?: string | null
  groupByTag?: boolean
  overwrite?: boolean
  syncModule?: boolean
  token?: string
  basicAuth?: boolean
}

/** 接口定义变更历史一条(审计)。 */
export interface ApiDefinitionChange {
  id: string
  definitionId: string
  action: string
  detail: string
  actor: string
  createdAt: string
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
  /** 优先级 / 状态 / 标签 / 请求头(见 0040 迁移)。 */
  priority?: string
  status?: string
  tags?: string[]
  headers?: { key: string; value: string }[]
  queryParams?: { key: string; value: string }[]
  restParams?: { key: string; value: string }[]
  auth?: { type: string; token?: string }
}

export interface ApiMock {
  id: string
  apiDefinitionId: string
  name: string
  matchRule: unknown
  responseStatus: number
  responseBody: string | null
  enabled: boolean
  /** 创建人 user_id(审计列;见 0057 迁移)。 */
  createdBy?: string
  /** 标签 / 响应头 / 响应延时(ms)/ 跟随定义(见 0040 迁移)。 */
  tags?: string[]
  responseHeaders?: { key: string; value: string }[]
  responseDelayMs?: number
  followDefinition?: boolean
}

/** 项目级 Mock 行(MOCK 视图):Mock + 所属定义的 方法/路径/协议/名称。 */
export interface ProjectMock {
  id: string
  apiDefinitionId: string
  name: string
  enabled: boolean
  responseStatus: number
  tags?: string[]
  method: string
  path: string
  protocol: string
  definitionName: string
  /** 操作人 / 更新时间(0057 后回填)。 */
  operator?: string
  updatedAt?: string
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
  /** 元信息(描述/标签/等级/模块/参数);不透明 JSON(见 0044 迁移)。 */
  meta?: Record<string, unknown>
  /** 审计(只读;见 0046 迁移)。 */
  createdBy?: string | null
  createdAt?: string
  updatedAt?: string
  /** 列表接口已返回步骤,用于显示步骤数。 */
  steps?: ScenarioStep[]
  /** 最近一次执行结果状态(SUCCESS/ERROR;列表回填,单查为 null)。 */
  lastResult?: string | null
}

export interface ScenarioStep {
  id: string
  order: number
  kind: string
  refMode: string
  caseId?: string | null
  scenarioId?: string | null
  request?: {
    method: string
    url: string
    body?: string | null
    assertions?: unknown
    /** 完整请求规格(后端 0056 起内联请求支持):请求头/Query/REST/认证/处理器,与用例同构。 */
    headers?: { key: string; value: string }[]
    queryParams?: { key: string; value: string }[]
    restParams?: { key: string; value: string }[]
    auth?: { type: string; token?: string }
    processors?: unknown[]
  } | null
  control?: unknown
  snapshot?: unknown
}

export interface ScenarioRunResult {
  reportId: string
  status: string
  caseCount: number
}

/** 场景报告明细行(逐用例结果)。注:响应时间/大小/状态码暂未持久化(待执行器扩展)。 */
export interface ReportResultItem {
  caseId: string
  outcome: string // SUCCESS | ERROR
  failures: string[]
  executedAt: string
  /** 响应明细(0045 后回填;旧报告为 null)。 */
  statusCode?: number | null
  latencyMs?: number | null
  respSize?: number | null
  body?: string | null
  headers?: [string, string][]
  /** 逐条断言结果(含通过项)+ 提取变量(0048 后回填;旧报告为空数组)。 */
  assertions?: AssertionResult[]
  extractions?: [string, string][]
  /** 实际发送的请求(0060 后回填;变量/baseUrl/认证已解析)。CASE 引用步骤也有。 */
  request?: { method: string; url: string; headers: [string, string][]; body?: string | null } | null
}
export interface ScenarioReportDetail {
  reportId: string
  status: string
  caseCount: number
  /** 报告起止时间与总耗时(毫秒,wall-clock;0056 后回填,旧报告为 null)。 */
  startedAt?: string | null
  finishedAt?: string | null
  durationMs?: number | null
  results: ReportResultItem[]
}
/** 项目级接口用例执行汇总(GET /api/case-exec-summary)。 */
export interface CaseExecSummary {
  executions: number
  passed: number
  executedCases: number
}
/** 近 N 天执行趋势的单日数据点(GET /api/exec-trend)。 */
export interface ExecTrendPoint {
  date: string // YYYY-MM-DD
  executions: number
  passed: number
}
/** 项目文件(文件管理)。 */
export interface ProjectFile {
  id: string
  name: string
  fileFormat: string
  sizeBytes: number
  moduleId?: string | null
  createdBy?: string | null
  createdAt: string
  updatedAt: string
}

/** 场景变更历史一条(审计日志)。 */
export interface ScenarioChange {
  id: string
  action: string // CREATE | UPDATE | ADD_STEP | DELETE_STEP | REORDER
  detail?: string | null
  userId?: string | null
  createdAt: string
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

export interface PoolNode {
  ip: string
  port: string
  concurrentNumber: number
  singleTaskConcurrentNumber: number
}

/** 类型相关配置:Node 用 nodes[];Kubernetes 用 ip/token/namespace/deployName + 并发。 */
export interface ResourcePoolConfig {
  nodes?: PoolNode[]
  ip?: string
  token?: string
  namespace?: string
  deployName?: string
  concurrentNumber?: number
  singleTaskConcurrentNumber?: number
}

export interface ResourcePool {
  id: string
  name: string
  enabled: boolean
  description: string
  maxConcurrency: number
  poolType: string // 'Node' | 'Kubernetes'
  allOrg: boolean
  orgIds: string[]
  serverUrl: string
  config: ResourcePoolConfig
  createdAt: string
  updatedAt: string
}

/** 创建/更新资源池入参(后端字段缺省值见 ResourcePoolBody)。 */
export interface ResourcePoolInput {
  name: string
  enabled?: boolean
  description?: string
  maxConcurrency?: number
  poolType?: string
  allOrg?: boolean
  orgIds?: string[]
  serverUrl?: string
  config?: ResourcePoolConfig
}

export interface Role {
  id: string
  name: string
  /** SYSTEM | ORGANIZATION | PROJECT。 */
  scope?: string
  permissions?: string[]
}

export interface User {
  id: string
  name: string
  email: string
  enable?: boolean
  /** 用户组(角色名);列表接口附带。 */
  userGroups?: string[]
}

export interface ApiKey {
  id: string
  name: string
  /** 明文密钥(sak_…);仅创建响应携带一次,列表接口不返回。 */
  key?: string
  /** 权限串,如 "DELIVERY:READ+UPDATE+EXECUTE"(资源:动作+动作)。 */
  permissions: string[]
  createdAt: string
  revoked?: boolean
}

export interface ProjectMember {
  projectId: string
  userId: string
  /** OWNER | MEMBER。 */
  role: string
  addedAt: string
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
  /** 创建人 user_id(见 0063 迁移)。 */
  createdBy?: string
}

/** 需求覆盖里的一条功能用例(按验收标准下标 criterionIndex)。 */
export interface CoverageCase {
  criterionIndex: number
  caseId: string
  caseName: string
  module: string
  priority: string
}
/** 一条功能用例覆盖的需求/标准(反查)。 */
export interface CaseRequirementLink {
  requirementId: string
  requirementTitle: string
  criterionIndex: number
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
  /** 工作量(task point);0 = 未估。 */
  points?: number
  /** 负责人 id/名;空 = 未分配。 */
  assignee?: string
  /** 负责人类型:HUMAN / AGENT。 */
  assigneeKind?: string
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
  /** 定向派发的目标 runtime name;空 = 任意同能力 runtime。 */
  targetRuntime?: string | null
  runId?: string | null
  deliverable?: { kind: string; reference: string; summary: string } | null
  error?: string | null
}

export interface DeliveryEvent {
  seq?: number
  kind: string
  message?: string
  detail?: unknown
}

/** 可派发的执行者类型 → 展示名(任务中心筛选与派发选择共用)。 */
export const EXECUTOR_LABEL: Record<string, string> = {
  CLAUDE_CODE: 'Claude Code',
  CODEX: 'Codex',
  OPENCODE: 'OpenCode',
  CODEBUDDY: 'CodeBuddy',
}

/** 任务中心一行:全系统交付尝试的执行状态/方式/结果/完成率聚合视图。 */
export interface TaskCenterItem {
  id: string
  decompositionId: string
  taskId: string
  title: string
  /** 任务描述(基本信息;无关联任务则空串)。 */
  description: string
  /** 所属模块(基本信息;任务归属需求标题,无则空串)。 */
  module: string
  /** 执行方式(执行者):CLAUDE_CODE / CODEX / OPENCODE / CODEBUDDY。 */
  executor: string
  /** 定向派发的目标 runtime name;空 = 任意同能力 runtime。 */
  targetRuntime?: string | null
  /** 执行状态:DISPATCHED / RUNNING / DELIVERED / FAILED / STOPPED。 */
  status: string
  /** 执行结果:SUCCESS / FAILED / STOPPED / PENDING。 */
  result: string
  /** 完成率 0..100。 */
  completionRate: number
  runId?: string
  error?: string
  createdAt: number
  eventCount: number
}

export interface TaskCenterPage {
  total: number
  current: number
  pageSize: number
  totalPages: number
  items: TaskCenterItem[]
}

export interface TaskCenterQuery {
  status?: string
  executor?: string
  /** 仅实时任务(未终态)。 */
  active?: boolean
  q?: string
  page?: number
  pageSize?: number
}

/** 执行机 / AI agent(Claude Code、Codex 等远程执行者),注册时自报支持协议。 */
export interface RunnerAgent {
  id: string
  name: string
  baseUrl: string
  enabled: boolean
  protocols: string[]
}

// AI 执行者机群:远程 Claude/Codex runtime,出站 register/heartbeat,server 据心跳判活。
export interface FleetRuntime {
  id: string
  name: string
  caps: string[]
  maxConcurrency: number
  lastSeenMs: number
  online: boolean
}

// 队列计数(机群可观测):各能力的积压 / 在飞 / 最久在飞空闲。
export interface FleetStat {
  executor: string
  ready: number
  inFlight: number
  oldestInFlightMs: number
}

export interface RunnerExecution {
  id: string
  agentId: string
  method: string
  url: string
  outcome: string
  status?: number
  elapsedMs?: number
  failures: string[]
  executedAt: string
}

/** 验证缺口:某条验收标准未被覆盖(UNCOVERED)或已覆盖但未交付验证(UNVERIFIED)。 */
export interface VerificationGap {
  criterionIndex: number
  text: string
  kind: string
}
export interface VerificationReport {
  satisfied?: number
  total?: number
  complete?: boolean
  gaps?: VerificationGap[]
  [k: string]: unknown
}

/** 用例评审(队列概览):通过规则 + 用例总数/已通过数。 */
export interface CaseReviewSummary {
  id: string
  passRule: string
  reviewerCount: number
  total: number
  passed: number
  createdAt: string
}
export interface CaseReviewCase {
  caseId: string
  status: string
}
export interface CaseReviewDetail {
  id: string
  passRule: string
  reviewerCount: number
  cases: CaseReviewCase[]
}

export interface Bug {
  id: string
  projectId?: string
  title?: string
  status: string
  createdAt?: number
}

/** 人机协同人效:需求维度的 AI/人工 交付拆分(口径:VERIFIED + 有无 DELIVERED 交付记录)。 */
export interface CollabRequirementItem {
  requirementId: string
  title: string
  aiTasks: number
  humanTasks: number
  aiPoints: number
  humanPoints: number
}
export interface CollabDayItem {
  /** 日期 YYYY-MM-DD。 */
  date: string
  ai: number
  human: number
}
export interface CollabStats {
  items: CollabRequirementItem[]
  daily: CollabDayItem[]
}

export interface BugRelation {
  /** REQUIREMENT | SCENARIO | FUNCTIONAL_CASE。 */
  kind: string
  targetId: string
}

/** 关注状态:某对象的关注人列表 + 当前用户是否在关注(后端 /follow 回读)。 */
export interface FollowStatus {
  entityType: string
  entityId: string
  following: boolean
  followers: string[]
  followerCount: number
}

export interface McpTool {
  name: string
  description?: string
}

export interface Skill {
  id: string
  projectId: string
  name: string
  description: string
  instructions: string
  includes: string[]
  enabled: boolean
}

export interface AssertionResult {
  item: string
  condition: string
  expected: string
  actual: string
  passed: boolean
  reason: string
}
export interface DebugResponse {
  status: number
  latencyMs: number
  headers: [string, string][]
  body: string
  /** 断言逐条结果(服务端执行时求值;本地执行无)。 */
  assertions?: AssertionResult[]
  /** 提取到的变量(变量名, 值)。 */
  extractions?: [string, string][]
}

export type RunMode = 'PARALLEL' | 'SERIAL'

// ---------- 端点封装 ----------

const emptyPage = <T>(): Page<T> => ({ total: 0, current: 1, pageSize: 0, totalPages: 0, items: [] })

export const api = {
  login: (username: string, password: string) =>
    http.post<{ token: string }>('/auth/login', { username, password }),

  organizations: () => http.get<Page<Organization>>('/organization?pageSize=100'),
  createOrganization: (name: string) => http.post<Organization>('/organization', { name }),
  updateOrganization: (id: string, b: { name: string; enable: boolean }) =>
    http.put<Organization>(`/organization/${id}`, b),
  deleteOrganization: (id: string) => http.del(`/organization/${id}`),
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

  // 列表视图(保存筛选/列/分页快照;own + shared)。
  views: (projectId: string) =>
    projectId
      ? http.get<ApiView[]>(`/api/api-view?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as ApiView[]),
  createView: (b: { projectId: string; name: string; config: unknown; shared?: boolean }) =>
    http.post<ApiView>('/api/api-view', b),
  deleteView: (id: string) => http.del(`/api/api-view/${id}`),
  getDefinition: (id: string) => http.get<ApiDefinition>(`/api/definition/${id}`),
  // 更新接口定义基础字段(名称/协议/方法/路径);缺省字段后端保持原值,返回更新后的定义。
  updateDefinition: (
    id: string,
    b: { name?: string; protocol?: string; method?: string; path?: string },
  ) => http.put<ApiDefinition>(`/api/definition/${id}`, b),
  deleteDefinition: (id: string) => http.del(`/api/definition/${id}`),
  updateDefinitionSpec: (id: string, spec: ApiSpec) =>
    http.put(`/api/definition/${id}/spec`, { spec }),
  updateDefinitionStatus: (id: string, status: string) =>
    http.put(`/api/definition/${id}/status`, { status }),
  definitionReferences: (id: string) =>
    http.get<{ cases: { id: string; name: string }[]; scenarios: { id: string; name: string }[] }>(
      `/api/definition/${id}/references`,
    ),
  definitionChanges: (id: string) =>
    http.get<ApiDefinitionChange[]>(`/api/definition/${id}/changes`),
  createDefinition: (b: {
    projectId: string
    name: string
    protocol?: string
    method?: string
    path?: string
  }) => http.post<ApiDefinition>('/api/definition', b),
  // 文件/粘贴导入:OpenAPI/Postman/HAR/MeterSphere 传 JSON 对象;JMeter(.jmx XML)传原始文本字符串。
  importDefinitions: (projectId: string, content: unknown, opts?: ImportOpts) =>
    http.post<{ created: ApiDefinition[]; updated: number; skipped: number }>('/api/definition/import', {
      projectId,
      content,
      format: opts?.format ?? 'openapi',
      moduleId: opts?.moduleId || undefined,
      groupByTag: opts?.groupByTag ?? true,
      overwrite: opts?.overwrite ?? true,
      syncModule: opts?.syncModule ?? false,
    }),
  // URL 导入:服务端拉取来源 URL(绕开浏览器跨域)并按格式导入,返回新增/覆盖/跳过计数。
  importFromUrl: (projectId: string, url: string, opts?: ImportOpts) =>
    http.post<{ created: number; updated: number; skipped: number }>('/api/definition/import-url', {
      projectId,
      url,
      format: opts?.format ?? 'openapi',
      token: opts?.token || undefined,
      basicAuth: opts?.basicAuth ?? false,
      moduleId: opts?.moduleId || undefined,
      groupByTag: opts?.groupByTag ?? true,
      overwrite: opts?.overwrite ?? true,
      syncModule: opts?.syncModule ?? false,
    }),

  // 定时导入计划
  importSchedules: (projectId: string) =>
    projectId
      ? http.get<ImportSchedule[]>(`/api/import-schedule?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as ImportSchedule[]),
  createImportSchedule: (b: {
    projectId: string
    name?: string
    url: string
    cron: string
    format?: ImportFormat
    token?: string
    basicAuth?: boolean
    moduleId?: string | null
    groupByTag?: boolean
    overwrite?: boolean
    syncModule?: boolean
    enabled?: boolean
  }) => http.post<ImportSchedule>('/api/import-schedule', b),
  setImportScheduleEnabled: (id: string, enabled: boolean) =>
    http.put(`/api/import-schedule/${id}/enabled`, { enabled }),
  runImportSchedule: (id: string) =>
    http.post<{ result: string }>(`/api/import-schedule/${id}/run`),
  deleteImportSchedule: (id: string) => http.del(`/api/import-schedule/${id}`),

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
    b: {
      name: string
      method: string
      url: string
      body?: string
      assertions?: unknown
      processors?: unknown
      priority?: string
      status?: string
      tags?: string[]
      headers?: { key: string; value: string }[]
      queryParams?: { key: string; value: string }[]
      restParams?: { key: string; value: string }[]
      auth?: { type: string; token?: string }
    },
  ) => http.post<ApiCase>(`/api/definition/${definitionId}/case`, b),
  runCase: (caseId: string, projectId: string, runMode: RunMode, poolId?: string) =>
    http.post<{ reportId: string; status: string }>(`/api/case/${caseId}/run`, {
      projectId,
      runMode,
      poolId,
    }),

  resourcePools: () => http.get<ResourcePool[]>('/api/resource-pool'),
  getResourcePool: (id: string) => http.get<ResourcePool>(`/api/resource-pool/${id}`),
  createResourcePool: (body: ResourcePoolInput) =>
    http.post<ResourcePool>('/api/resource-pool', body),
  updateResourcePool: (id: string, body: ResourcePoolInput) =>
    http.put<ResourcePool>(`/api/resource-pool/${id}`, body),
  deleteResourcePool: (id: string) => http.del<void>(`/api/resource-pool/${id}`),

  // 项目成员
  projectMembers: (projectId: string) =>
    http.get<ProjectMember[]>(`/project/${encodeURIComponent(projectId)}/member`),
  addProjectMember: (projectId: string, b: { userId: string; role?: string }) =>
    http.post<ProjectMember>(`/project/${encodeURIComponent(projectId)}/member`, b),
  removeProjectMember: (projectId: string, userId: string) =>
    http.del(`/project/${encodeURIComponent(projectId)}/member/${encodeURIComponent(userId)}`),

  // 角色 / 用户(平台级)
  roles: () => http.get<Page<Role>>('/role?pageSize=100'),
  createRole: (b: { name: string; scope?: string; permissions?: string[] }) => http.post<Role>('/role', b),
  updateRole: (id: string, b: { name: string; permissions?: string[] }) => http.put<Role>(`/role/${id}`, b),
  deleteRole: (id: string) => http.del(`/role/${id}`),
  grantUserRole: (userId: string, roleId: string) => http.post('/user-role/grant', { userId, roleId }),
  revokeUserRole: (userId: string, roleId: string) => http.post('/user-role/revoke', { userId, roleId }),
  users: () => http.get<Page<User>>('/system/user?pageSize=100'),
  createUser: (b: { name: string; email: string }) => http.post<User>('/system/user', b),
  updateUser: (id: string, b: { name: string; email: string; enable: boolean }) =>
    http.put<User>(`/system/user/${id}`, b),
  deleteUser: (id: string) => http.del(`/system/user/${id}`),
  resetUserPassword: (id: string) => http.post<{ password: string }>(`/system/user/${id}/reset-password`),

  // API 密钥(系统级;创建响应含一次性明文 key,吊销即 DELETE)
  apiKeys: () => http.get<{ items: ApiKey[] }>('/system/apikey'),
  createApiKey: (b: { name: string; permissions: string[] }) => http.post<ApiKey>('/system/apikey', b),
  revokeApiKey: (id: string) => http.del(`/system/apikey/${encodeURIComponent(id)}`),

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
  // 全量更新一条功能用例(PUT 语义:未传字段会被覆盖为缺省,调用方需带齐字段)。
  updateFunctionalCase: (
    id: string,
    b: {
      projectId: string
      name: string
      priority?: string
      status?: string
      module?: string
      steps?: CaseStep[]
      customFields?: Record<string, string>
    },
  ) => http.put<FunctionalCase>(`/functional-case/${id}`, b),
  deleteFunctionalCase: (id: string) => http.del<void>(`/functional-case/${id}`),
  // 导出 xlsx(二进制下载)/ 导入 xlsx(原始字节上传,返回导入条数)。
  exportFunctionalCases: (projectId: string) =>
    http.getBlob(`/functional-case/export?projectId=${encodeURIComponent(projectId)}`),
  importFunctionalCases: (projectId: string, file: File) =>
    http.upload<{ imported: number }>(`/functional-case/import?projectId=${encodeURIComponent(projectId)}`, file),
  // 需求 ↔ 功能用例 覆盖关联。
  linkRequirementCase: (b: { requirementId: string; criterionIndex: number; functionalCaseId: string; projectId?: string }) =>
    http.post('/requirement-case/link', b),
  unlinkRequirementCase: (b: { requirementId: string; criterionIndex: number; functionalCaseId: string }) =>
    http.post('/requirement-case/unlink', b),
  requirementCoverage: (requirementId: string) =>
    http.get<CoverageCase[]>(`/requirement/${requirementId}/functional-coverage`),
  caseRequirements: (caseId: string) =>
    http.get<CaseRequirementLink[]>(`/functional-case/${caseId}/requirements`),

  // 项目接口用例(供测试计划挂载选择)
  projectCases: (projectId: string) =>
    projectId
      ? http.get<Page<ApiCase>>(`/api/case?projectId=${encodeURIComponent(projectId)}&pageSize=100`)
      : Promise.resolve(emptyPage<ApiCase>()),
  // 翻页拉全部项目用例(场景步骤 id→名 映射用:用例数 >100 时单页会漏,导致步骤名回落短 id)。
  projectCasesAll: async (projectId: string): Promise<ApiCase[]> => {
    if (!projectId) return []
    const size = 200
    const out: ApiCase[] = []
    for (let page = 1; page <= 50; page++) {
      const p = await http.get<Page<ApiCase>>(
        `/api/case?projectId=${encodeURIComponent(projectId)}&current=${page}&pageSize=${size}`,
      )
      out.push(...(p.items || []))
      if (out.length >= (p.total || out.length) || !p.items?.length) break
    }
    return out
  },

  // 测试计划(无 list 端点 → 列表由前端注册表维护)
  createPlan: (b: { projectId: string; name: string; type?: string }) =>
    http.post<TestPlan>('/test-plan', { type: 'TEST_PLAN', ...b }),
  planStats: (id: string) => http.get<PlanStats>(`/test-plan/${id}/statistics`),
  planCases: (id: string) => http.get<PlanCase[] | Page<PlanCase>>(`/test-plan/${id}/cases`),
  linkPlanCase: (id: string, caseId: string, name: string) =>
    http.post(`/test-plan/${id}/cases`, { caseId, name }),
  // 手动登记一条用例的执行结果(通过/不通过/阻塞/误报);status: SUCCESS|ERROR|BLOCK|FAKE_ERROR|PENDING
  recordPlanCaseResult: (id: string, caseId: string, status: string) =>
    http.post(`/test-plan/${id}/cases/${caseId}/result`, { status }),
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
    iterations?: number
    durationMs?: number
  }) => http.post<{ reportId: string; status: string }>('/perf/run', b),
  // 场景压测:一次施压 = 跑完整条场景链(登录→提取→鉴权调用→…),报告复用 PerfReport。
  runScenarioPerf: (b: {
    projectId: string
    scenarioId: string
    environmentId?: string
    concurrency: number
    iterations?: number
    durationMs?: number
  }) => http.post<{ reportId: string; status: string; stepCount: number }>('/perf/scenario/run', b),
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
  getRequirementVersion: (id: string, n: number) =>
    http.get<RequirementVersion>(`/requirement/${id}/version/${n}`),
  renameRequirement: (id: string, title: string) =>
    http.put<Requirement>(`/requirement/${id}`, { title }),
  deleteRequirement: (id: string) => http.del(`/requirement/${id}`),
  archiveRequirement: (id: string) => http.post<Requirement>(`/requirement/${id}/archive`, {}),
  deliverRequirement: (id: string) => http.post<Requirement>(`/requirement/${id}/deliver`, {}),
  setBaseline: (id: string, version: number) =>
    http.put<Requirement>(`/requirement/${id}/baseline`, { version }),
  breakdown: (id: string) =>
    http.post<{ id: string; verificationId: string; tasks: Task[] }>(`/requirement/${id}/breakdown`, {}),

  // 拆分图 / 任务
  decomposition: (id: string) => http.get<Decomposition>(`/decomposition/${id}`),
  decompositionReady: (id: string) => http.get<Task[]>(`/decomposition/${id}/ready`),
  addTask: (id: string, b: { title: string; acceptanceCriteria: string[]; dependencies: string[]; points?: number }) =>
    http.post<{ taskId: string }>(`/decomposition/${id}/task`, b),
  setTaskPoints: (decompId: string, taskId: string, points: number) =>
    http.post(`/decomposition/${decompId}/task/${taskId}/points`, { points }),
  setTaskAssignee: (decompId: string, taskId: string, assignee: string, kind: string) =>
    http.post(`/decomposition/${decompId}/task/${taskId}/assignee`, { assignee, kind }),
  runDecomposition: (id: string) =>
    http.post<{ total: number; verified: number; failed: number; blocked: number; rounds: number }>(
      `/decomposition/${id}/run`,
      {},
    ),

  // 交付
  createDelivery: (b: { decompositionId: string; taskId: string; title: string; executor: string; targetRuntime?: string }) =>
    http.post<DeliveryAttempt>('/delivery', b),
  deliveries: (decompositionId: string, taskId: string) =>
    http.get<DeliveryAttempt[]>(
      `/delivery?decompositionId=${encodeURIComponent(decompositionId)}&taskId=${encodeURIComponent(taskId)}`,
    ),
  deliveryEvents: (attemptId: string) => http.get<DeliveryEvent[]>(`/delivery/${attemptId}/events`),

  // 任务中心(系统级:后台任务/即时任务列表 + 执行详情 + 停止/删除)
  taskCenter: (params: TaskCenterQuery = {}) => {
    const sp = new URLSearchParams()
    if (params.status) sp.set('status', params.status)
    if (params.executor) sp.set('executor', params.executor)
    if (params.active) sp.set('active', 'true')
    if (params.q) sp.set('q', params.q)
    sp.set('page', String(params.page ?? 1))
    sp.set('pageSize', String(params.pageSize ?? 20))
    return http.get<TaskCenterPage>(`/delivery/tasks?${sp.toString()}`)
  },
  stopTask: (id: string, reason?: string) =>
    http.post<DeliveryAttempt>(`/delivery/${id}/stop`, { reason }),
  deleteTask: (id: string) => http.del<void>(`/delivery/${id}`),

  // 执行机 / AI agent 管理(人机协同的执行者侧)
  // AI 执行者机群(SHEPHERD_AGENT_FLEET 模式):列出远程 runtime + 在线状态。
  fleetRuntimes: () => http.get<FleetRuntime[]>('/agent/runtime'),
  // 机群队列计数:各能力积压/在飞(机群可观测),供机群视图 backlog 概览。
  fleetStats: () => http.get<FleetStat[]>('/agent/work/stats'),
  runnerAgents: () => http.get<RunnerAgent[]>('/runner-agent'),
  registerRunnerAgent: (b: { name: string; baseUrl: string; token?: string; enabled?: boolean }) =>
    http.post<RunnerAgent>('/runner-agent', b),
  refreshRunnerAgent: (id: string) => http.post<string[]>(`/runner-agent/${id}/refresh`, {}),
  runnerExecutions: (id: string) => http.get<RunnerExecution[]>(`/runner-agent/${id}/executions`),

  // 验证(覆盖链 / 报告)
  verificationReport: (id: string) => http.get<VerificationReport>(`/verification/${id}/report`),
  verificationLink: (id: string, b: { criterionIndex: number; decompositionId: string; taskId: string }) =>
    http.post(`/verification/${id}/link`, b),
  verificationSync: (id: string, b: { decompositionId: string; taskId: string; satisfied: boolean }) =>
    http.post(`/verification/${id}/sync`, b),

  // 用例评审队列(创建/列表/详情/提交裁决)
  caseReviews: (projectId: string) =>
    projectId ? http.get<CaseReviewSummary[]>(`/case-review?projectId=${encodeURIComponent(projectId)}`) : Promise.resolve([] as CaseReviewSummary[]),
  createCaseReview: (b: { projectId: string; passRule: string; reviewerCount: number; caseIds: string[] }) =>
    http.post<{ id: string }>('/case-review', b),
  caseReview: (id: string) => http.get<CaseReviewDetail>(`/case-review/${id}`),
  submitCaseReview: (reviewId: string, caseId: string, b: { reviewerId: string; status: string; content?: string }) =>
    http.post<{ status: string }>(`/case-review/${reviewId}/${caseId}`, b),

  // 缺陷 — 列表/创建/状态流转全走后端(按项目隔离,按创建时间倒序)
  bugs: (projectId: string) =>
    projectId ? http.get<Bug[]>(`/bug?projectId=${encodeURIComponent(projectId)}`) : Promise.resolve([] as Bug[]),
  createBug: (b: { projectId: string; title: string; initialStatus: string }) => http.post<Bug>('/bug', b),
  setBugStatus: (id: string, status: string) => http.post<Bug>(`/bug/${id}/status`, { status }),
  // 人机协同人效(需求维度 AI/人工 拆分 + 周趋势)
  collabStats: (projectId: string) =>
    http.get<CollabStats>(`/delivery/collab-stats?projectId=${encodeURIComponent(projectId)}`),

  // 缺陷 ↔ 资产关联(需求/场景用例/功能用例)
  bugRelations: (id: string) =>
    http.get<{ relations: BugRelation[] }>(`/bug/${encodeURIComponent(id)}/relation`),
  linkBugRelation: (id: string, b: { kind: string; targetId: string }) =>
    http.post<{ relations: BugRelation[] }>(`/bug/${encodeURIComponent(id)}/relation`, b),
  unlinkBugRelation: (id: string, kind: string, targetId: string) =>
    http.del<{ relations: BugRelation[] }>(
      `/bug/${encodeURIComponent(id)}/relation/${encodeURIComponent(kind)}/${encodeURIComponent(targetId)}`,
    ),

  // 关注人(通用):任意对象按 (projectId, entityType, entityId) 关注/取消/查询。
  follow: (b: { projectId: string; entityType: string; entityId: string }) =>
    http.post<FollowStatus>('/follow', b),
  unfollow: (b: { projectId: string; entityType: string; entityId: string }) =>
    http.del<FollowStatus>('/follow', b),
  followStatus: (projectId: string, entityType: string, entityId: string) =>
    http.get<FollowStatus>(
      `/follow?projectId=${encodeURIComponent(projectId)}&entityType=${encodeURIComponent(entityType)}&entityId=${encodeURIComponent(entityId)}`,
    ),
  myFollows: (projectId: string, entityType?: string) =>
    http.get<{ entityIds: string[] }>(
      `/follow/mine?projectId=${encodeURIComponent(projectId)}${entityType ? `&entityType=${encodeURIComponent(entityType)}` : ''}`,
    ),

  // 技能 — 列表/详情/更新/删除走后端
  skills: (projectId: string) =>
    projectId ? http.get<Skill[]>(`/skill?projectId=${encodeURIComponent(projectId)}`) : Promise.resolve([] as Skill[]),
  getSkill: (id: string) => http.get<Skill>(`/skill/${id}`),
  createSkill: (b: { projectId: string; name: string; instructions: string; description?: string; includes?: string[] }) =>
    http.post<Skill>('/skill', b),
  updateSkill: (id: string, b: { name: string; description?: string; instructions: string; includes?: string[]; enabled?: boolean }) =>
    http.put<Skill>(`/skill/${id}`, b),
  deleteSkill: (id: string) => http.del(`/skill/${id}`),
  composeSkills: (projectId: string, skillIds: string[]) =>
    http.post<{ instructions: string }>('/skill/compose', { projectId, skillIds }),

  // 接口调试台:进程内即时发起请求(POST /api/debug/send)
  debugSend: (b: { protocol?: string; method: string; url: string; headers?: { key: string; value: string }[]; body?: string; meta?: Record<string, string>; assertions?: unknown[]; processors?: unknown[] }) =>
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
  projectMocks: (projectId: string) =>
    projectId ? http.get<ProjectMock[]>(`/api/mock?projectId=${encodeURIComponent(projectId)}`) : Promise.resolve([] as ProjectMock[]),
  deleteMock: (mockId: string) => http.del(`/api/mock/${mockId}`),
  updateCase: (
    id: string,
    b: { name: string; method: string; url: string; body?: string | null; assertions?: unknown; processors?: unknown; priority?: string; status?: string; tags?: string[]; headers?: { key: string; value: string }[]; queryParams?: { key: string; value: string }[]; restParams?: { key: string; value: string }[]; auth?: { type: string; token?: string } },
  ) => http.put(`/api/case/${id}`, b),
  deleteCase: (id: string) => http.del(`/api/case/${id}`),
  createMock: (
    definitionId: string,
    b: {
      name: string
      matchRule?: unknown
      responseStatus?: number
      responseBody?: string
      enabled?: boolean
      tags?: string[]
      responseHeaders?: { key: string; value: string }[]
      responseDelayMs?: number
      followDefinition?: boolean
    },
  ) => http.post<ApiMock>(`/api/definition/${definitionId}/mock`, b),
  updateMock: (
    mockId: string,
    b: {
      name: string
      matchRule?: unknown
      responseStatus?: number
      responseBody?: string
      enabled?: boolean
      tags?: string[]
      responseHeaders?: { key: string; value: string }[]
      responseDelayMs?: number
      followDefinition?: boolean
    },
  ) => http.put(`/api/mock/${mockId}`, b),

  scenarios: (projectId: string) =>
    projectId
      ? http.get<Scenario[]>(`/api/scenario?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as Scenario[]),
  getScenario: (id: string) => http.get<Scenario & { steps: ScenarioStep[] }>(`/api/scenario/${id}`),
  createScenario: (projectId: string, name: string) =>
    http.post<Scenario>('/api/scenario', { projectId, name }),
  deleteScenario: (id: string) => http.del(`/api/scenario/${id}`),
  addStep: (
    scenarioId: string,
    b: { kind: string; order: number; refId?: string; request?: unknown; control?: unknown },
  ) => http.post<ScenarioStep>(`/api/scenario/${scenarioId}/step`, b),
  updateScenario: (id: string, b: { name: string; status?: string; meta?: Record<string, unknown> }) =>
    http.patch<Scenario>(`/api/scenario/${id}`, b),
  deleteScenarioStep: (scenarioId: string, stepId: string) =>
    http.del(`/api/scenario/${scenarioId}/step/${stepId}`),
  reorderScenarioSteps: (scenarioId: string, order: string[]) =>
    http.patch(`/api/scenario/${scenarioId}/steps/order`, { order }),
  runScenario: (scenarioId: string, projectId: string, opts?: { environmentId?: string; failureStrategy?: 'CONTINUE' | 'STOP' }) =>
    http.post<ScenarioRunResult>(`/api/scenario/${scenarioId}/run`, { projectId, environmentId: opts?.environmentId, failureStrategy: opts?.failureStrategy }),
  scenarioExecutions: (scenarioId: string) =>
    http.get<Page<ScenarioExecution>>(`/api/scenario/${scenarioId}/executions`),
  scenarioReport: (reportId: string) =>
    http.get<ScenarioReportDetail>(`/api/scenario-report/${reportId}`),
  caseExecSummary: (projectId: string) =>
    http.get<CaseExecSummary>(`/api/case-exec-summary?projectId=${encodeURIComponent(projectId)}`),
  execTrend: (projectId: string, days = 7) =>
    http.get<ExecTrendPoint[]>(`/api/exec-trend?projectId=${encodeURIComponent(projectId)}&days=${days}`),
  projectFiles: (projectId: string) =>
    http.get<ProjectFile[]>(`/api/project-file?projectId=${encodeURIComponent(projectId)}`),
  uploadProjectFile: (b: { projectId: string; name: string; fileFormat: string; sizeBytes: number; contentBase64: string; moduleId?: string | null }) =>
    http.post<{ id: string }>('/api/project-file', b),
  downloadProjectFile: (id: string) =>
    http.get<{ name: string; contentBase64: string }>(`/api/project-file/${id}/download`),
  deleteProjectFile: (id: string) => http.del(`/api/project-file/${id}`),
  moveProjectFile: (id: string, moduleId: string | null) =>
    http.put(`/api/project-file/${id}/module`, { moduleId }),
  scenarioChanges: (scenarioId: string) =>
    http.get<ScenarioChange[]>(`/api/scenario/${scenarioId}/changes`),
}
