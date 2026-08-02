// REST client + types for the Shepherd backend (mirrors Rust DTOs, camelCase).
// All requests carry a Bearer token; 401 triggers the logout callback. Dev goes through Vite proxy → :9180.

const TOKEN_KEY = 'shepherd.token'
const USER_KEY = 'shepherd.user'

export const tokenStore = {
  get: () => localStorage.getItem(TOKEN_KEY) || '',
  set: (t: string) => localStorage.setItem(TOKEN_KEY, t),
  clear: () => localStorage.removeItem(TOKEN_KEY),
}

// Current login username: written at login, shown in personal center / creator columns.
// Single source so pages don't each hardcode the localStorage key and fallback.
export const userStore = {
  get: () => localStorage.getItem(USER_KEY) || 'admin',
  set: (u: string) => localStorage.setItem(USER_KEY, u),
  clear: () => localStorage.removeItem(USER_KEY),
}

// Session user id (login response userId): the value audit fields like created_by use; "created by me" matches on it.
// Old sessions never stored it → fall back to username (usually identical on self-hosted deploys).
const USER_ID_KEY = 'shepherd.userId'
export const userIdStore = {
  get: () => localStorage.getItem(USER_ID_KEY) || localStorage.getItem(USER_KEY) || 'admin',
  set: (u: string) => localStorage.setItem(USER_ID_KEY, u),
  clear: () => localStorage.removeItem(USER_ID_KEY),
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
  // 2xx but HTML body (typically: dev proxy miss → Vite returns index.html).
  // Fail fast with a clear error instead of stuffing an HTML string into state and blanking the page.
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

// Raw text GET (non-JSON responses like Markdown reports).
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

// Raw binary GET (file downloads like xlsx export).
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

// Raw bytes POST (xlsx import upload; body is the file itself).
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

// ---------- Types ----------

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

/** Request/response spec ("Definition" tab): headers/query/body + sample responses. */
export interface ApiSpecKV {
  name: string
  value?: string
  desc?: string
}
export interface ApiSpecResponse {
  status?: number
  body?: string
  /** Sample response headers ("Response headers" tab under response content). */
  headers?: ApiSpecKV[]
}
export type ApiBodyType = 'none' | 'form-data' | 'x-www-form-urlencoded' | 'json' | 'xml' | 'raw' | 'binary'

/** body type → default Content-Type; none has no body, raw is not forced (user/headers decide) → undefined. */
export function contentTypeForBodyType(t?: ApiBodyType): string | undefined {
  switch (t) {
    case 'json':
      return 'application/json'
    case 'xml':
      return 'application/xml'
    case 'form-data':
      // multipart needs a boundary param but the editor only holds a raw text body; don't auto-attach (a boundary-less header breaks server parsing) — user sets it in request headers.
      return undefined
    case 'x-www-form-urlencoded':
      return 'application/x-www-form-urlencoded'
    case 'binary':
      return 'application/octet-stream'
    default:
      // none / raw / unspecified: don't auto-attach; Content-Type is optional.
      return undefined
  }
}

/**
 * Add a default Content-Type header for the body type — but only as a fallback:
 * if the user already set Content-Type (case-insensitive) it is respected, never overridden;
 * none/raw body types attach nothing. Returns a new array, input untouched.
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
export interface ApiSpecAuth {
  type?: 'none' | 'bearer' | 'basic'
  token?: string
}
/** JSON request-body schema tree node (schema table). */
export interface BodySchemaNode {
  name: string
  type: 'string' | 'integer' | 'number' | 'boolean' | 'object' | 'array'
  value?: string
  description?: string
  children?: BodySchemaNode[]
}

export interface ApiSpec {
  // basics
  description?: string
  tags?: string[]
  // request
  requestHeaders?: ApiSpecKV[]
  requestQuery?: ApiSpecKV[]
  /** REST path params (the {id} in /users/{id}). */
  restParams?: ApiSpecKV[]
  bodyType?: ApiBodyType
  requestBody?: string
  /** Key-value body for form-data / urlencoded. */
  formBody?: ApiSpecKV[]
  /** Schema tree for the JSON body (name/type/value/description + nesting); coexists with the requestBody text. */
  bodySchema?: BodySchemaNode[]
  auth?: ApiSpecAuth
  responses?: ApiSpecResponse[]
  /** Pre/post processors (api-runner Processor JSON; wait/extract/script/sql). */
  preProcessors?: unknown[]
  postProcessors?: unknown[]
  /** Assertions (api-runner Assertion JSON). */
  assertions?: unknown[]
}

export interface ApiDefinition {
  id: string
  /** Human-readable number ("101093"-style ID; migration 0042). */
  num?: number
  projectId: string
  name: string
  protocol: string
  method: string
  path: string
  status: string
  moduleId?: string | null
  /** Opaque JSON spec; round-tripped through the server (migration 0037). */
  spec?: ApiSpec
  /** Creator user_id (migration 0039). */
  createdBy?: string
  /** Create/update time (server sends text, e.g. "2026-06-21 12:34:56+00"). */
  createdAt?: string
  updatedAt?: string
}

/** API list view (saved filter/column/pagination snapshot; config is opaque JSON). */
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

export type ImportFormat = 'openapi' | 'postman' | 'har' | 'jmeter' | 'metersphere'

/** Scheduled import plan (backend never echoes the token back). */
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
  /** Operator of the last run (manual "run now" records the user; cron runs leave it empty → UI shows "auto"; migration 0062). */
  lastRunBy?: string
  /** Creator user_id (audit column; migration 0061). */
  createdBy: string
  createdAt: string
}

/** Import options shared by URL import and scheduled import. */
export interface ImportOpts {
  format?: ImportFormat
  moduleId?: string | null
  groupByTag?: boolean
  overwrite?: boolean
  syncModule?: boolean
  token?: string
  basicAuth?: boolean
}

/** One API-definition change record (audit). */
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
  /** Priority / status / tags / headers (migration 0040). */
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
  /** Creator user_id (audit column; migration 0057). */
  createdBy?: string
  /** Tags / response headers / response delay (ms) / follow-definition (migration 0040). */
  tags?: string[]
  responseHeaders?: { key: string; value: string }[]
  responseDelayMs?: number
  followDefinition?: boolean
}

/** Project-level mock row (MOCK view): mock + owning definition's method/path/protocol/name. */
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
  /** Operator / update time (backfilled since 0057). */
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
  /** Per-project display number shown as the list ID (100001…). */
  num?: number
  name: string
  status: string
  /** Meta (description/tags/level/module/params); opaque JSON (migration 0044). */
  meta?: Record<string, unknown>
  /** Audit fields (read-only; migration 0046). */
  createdBy?: string | null
  createdAt?: string
  updatedAt?: string
  /** List endpoint includes steps, used to show the step count. */
  steps?: ScenarioStep[]
  /** Latest run status (SUCCESS/ERROR; filled in list responses, null on single fetch). */
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
    /** Full request spec (inline requests supported since backend 0056): headers/query/REST/auth/processors, same shape as a case. */
    headers?: { key: string; value: string }[]
    queryParams?: { key: string; value: string }[]
    restParams?: { key: string; value: string }[]
    auth?: { type: string; token?: string }
    processors?: unknown[]
    /** Copy provenance (COPY_CASE / COPY_API / COPY_SCENARIO) for materialized steps. */
    source?: string
  } | null
  control?: unknown
  snapshot?: unknown
}

/** Remote execution location of a run (explicit or auto-picked pool); null = local. */
export interface ExecutedOn {
  poolId: string
  poolName: string
  /** Assigned runner's display name; empty while the run sat queued. */
  runner: string
}

export interface ScenarioRunResult {
  reportId: string
  /** SUCCESS | ERROR, or RUNNING when started with asyncRun. */
  status: string
  caseCount: number
  /** Ordered step identities (asyncRun): CASE = case id, REQUEST = "METHOD url". */
  steps?: string[]
  executedOn?: ExecutedOn | null
}

/** One connected pool-runner in the detailed status endpoint. */
export interface PoolRunnerInfo {
  name: string
  maxConcurrent: number
  inFlight: number
}

/** Live run event pushed over /api/run-events/ws?runId=… */
export interface RunEvent {
  type: 'runStarted' | 'stepStarted' | 'stepFinished' | 'stepDetail' | 'runComplete'
  runId: string
  stepId?: string
  steps?: string[]
  status?: string
  statusCode?: number
  latencyMs?: number | null
  failures?: string[]
  timings?: PhaseTimings | null
}

/** WebSocket URL for live run events (token via query: browsers can't set WS headers). */
export const runEventsWsUrl = (runId: string) => {
  const proto = window.location.protocol === 'https:' ? 'wss' : 'ws'
  return `${proto}://${window.location.host}/api/run-events/ws?runId=${encodeURIComponent(runId)}&token=${encodeURIComponent(tokenStore.get())}`
}

/** Scenario report detail row (per-case result). Note: latency/size/status code not yet persisted (pending runner support). */
/** Per-phase timing of one HTTP exchange (waterfall on latency hover). */
export interface PhaseTimings {
  dnsMs?: number | null
  ttfbMs?: number | null
  downloadMs?: number | null
}

export interface ReportResultItem {
  caseId: string
  outcome: string // SUCCESS | ERROR
  failures: string[]
  executedAt: string
  /** Response details (backfilled since 0045; null on older reports). */
  statusCode?: number | null
  latencyMs?: number | null
  respSize?: number | null
  body?: string | null
  headers?: [string, string][]
  /** Per-assertion results (including passes) + extracted variables (since 0048; empty arrays on older reports). */
  assertions?: AssertionResult[]
  extractions?: [string, string][]
  /** Per-phase HTTP timings (since 0095; absent on older reports). */
  timings?: PhaseTimings | null
  /** Request actually sent (since 0060; variables/baseUrl/auth already resolved). Present for CASE reference steps too. */
  request?: { method: string; url: string; headers: [string, string][]; body?: string | null } | null
}
export interface ScenarioReportDetail {
  reportId: string
  status: string
  caseCount: number
  /** Display name (union batch reports); absent for single runs. */
  name?: string
  /** Report start/end and total duration (ms, wall-clock; since 0056, null on older reports). */
  startedAt?: string | null
  finishedAt?: string | null
  durationMs?: number | null
  /** Owning scenario id (present only on public share reads) → enables the step-tree view. */
  scenarioId?: string
  results: ReportResultItem[]
}
/** Project-level API case execution summary (GET /api/case-exec-summary). */
export interface CaseExecSummary {
  executions: number
  passed: number
  executedCases: number
}
/** Single-day point of the last-N-days execution trend (GET /api/exec-trend). */
export interface ExecTrendPoint {
  date: string // YYYY-MM-DD
  executions: number
  passed: number
}
/** Project file (file management). */
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

/** One scenario change record (audit log). */
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

/** Type-specific config: Node uses nodes[]; Kubernetes uses ip/token/namespace/deployName + concurrency. */
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

/** Create/update resource pool input (backend defaults: see ResourcePoolBody). */
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
  /** SYSTEM | ORGANIZATION | PROJECT. */
  scope?: string
  permissions?: string[]
}

export interface User {
  id: string
  name: string
  email: string
  enable?: boolean
  /** User groups (role names); included by the list endpoint. */
  userGroups?: string[]
}

export interface ApiKey {
  id: string
  name: string
  /** Plaintext key (sak_…); returned once in the create response only, never by list. */
  key?: string
  /** Permission strings like "DELIVERY:READ+UPDATE+EXECUTE" (resource:action+action). */
  permissions: string[]
  createdAt: string
  /** Expiry; empty = never expires. */
  expiresAt?: string | null
  revoked?: boolean
}

// Session identity (GET /auth/me): data source for the personal-center info page; falls back to local store on failure.
export interface AuthMe {
  userId: string
  permissions: string[]
}

// Personal LLM model config (/me/llm-model): apiKey is write-only; list returns a masked value.
export interface LlmModel {
  id: string
  provider: string
  name: string
  baseUrl?: string
  apiKeyMasked?: string
  enabled: boolean
  createdAt: string
}

// RAG config (system settings). GET masks keys as *KeySet booleans; PUT sends keys only to change them.
export interface RagConfigView {
  embedUrl: string
  embedModel: string
  embedDim: number
  embedKeySet: boolean
  chatUrl: string
  chatModel: string
  chatKeySet: boolean
  maxTokens: number
  topK: number
  rerank: boolean
}
export interface RagConfigBody {
  embedUrl: string
  embedModel: string
  embedDim: number
  embedKey?: string
  chatUrl: string
  chatModel: string
  chatKey?: string
  maxTokens: number
  topK: number
  rerank: boolean
}

export interface ProjectMember {
  projectId: string
  userId: string
  /** OWNER | MEMBER. */
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
  /** Per-project display number shown as the case ID in the UI (100001…). */
  num?: number
  name: string
  module?: string
  priority?: string
  status?: string
  tags?: string[]
  steps?: CaseStep[]
  customFields?: Record<string, string>
  /** Creator user_id (migration 0063). */
  createdBy?: string
  createdAt?: string
  updatedAt?: string
}

/** One audit entry of a functional case (field-level old → new). */
export interface CaseChange {
  field: string
  oldValue: string
  newValue: string
  actor: string
  createdAt: string
}

/** Bug linked to a functional case (via ms_bug_relation). */
export interface CaseBugLink {
  bugId: string
  title: string
  status: string
  createdBy: string
  handler: string
}

/** Review containing the case + the case's own review status. */
export interface CaseReviewLink {
  reviewId: string
  status: string
  createdAt: string
}

/** Test plan containing the case + its execution outcome. */
export interface CasePlanLink {
  planId: string
  planName: string
  projectName: string
  archived: boolean
  execStatus: string
  executedAt: string
}

/** Pre/post case dependency, resolved to the other case's identity. */
export interface CaseDependencyLink {
  caseId: string
  num: number
  name: string
  createdBy: string
}

/** Generic comment attached to any entity (targetType + targetId). */
export interface CommentItem {
  id: string
  targetType: string
  targetId: string
  content: string
  author: string
  createdAt: string
}

/** One functional case in requirement coverage (keyed by acceptance-criterion index). */
export interface CoverageCase {
  criterionIndex: number
  caseId: string
  caseName: string
  module: string
  priority: string
}
/** Requirement/criterion covered by a functional case (reverse lookup). */
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

/** Step result of a scenario mounted on a plan (recursive for controllers). */
export interface PlanStepResult {
  name: string
  kind: string
  status: string
  latencyMs: number
  statusCode?: number | null
  children: PlanStepResult[]
}

export interface PlanCase {
  caseId: string
  name: string
  status: string
  latencyMs?: number | null
  statusCode?: number | null
  /** Non-empty for scenario-mounted entries executed by the plan runner. */
  steps?: PlanStepResult[]
  /** Scenario report id for scenario-mounted entries; detail lives in the scenario report. */
  reportId?: string | null
}

/** Per-node execution config in the plan mind-map (inherit = use the parent's config). */
export interface PlanningNodeConfig {
  inherit?: boolean
  poolId?: string
  envId?: string
  mode?: 'serial' | 'parallel'
  stopOnFail?: boolean
  retry?: boolean
}

/** Mind-map node: category (功能/接口/场景用例) or custom 测试点; leaves carry linked case/scenario ids. */
export interface PlanningNode {
  id: string
  name: string
  kind: 'category' | 'point'
  children?: PlanningNode[]
  config?: PlanningNodeConfig
  caseIds?: string[]
  scenarioIds?: string[]
}

/** Planning doc stored verbatim; caseNames/scenarioNames feed the backend link sync display names. */
export interface PlanningDoc {
  nodes: PlanningNode[]
  caseNames?: Record<string, string>
  scenarioNames?: Record<string, string>
}

/** Row of GET /test-plan?projectId=; plan groups share the table (type GROUP). */
export interface PlanListItem {
  id: string
  projectId: string
  name: string
  type: string
  /** 'NONE' = not in a plan group. */
  groupId: string
  createdBy: string | null
  createdAt: number
  description: string
  tags: string[]
  moduleId: string | null
  startAt: number | null
  endAt: number | null
  /** Percent 0-100. */
  passThreshold: number
}

export interface PlanDetailInfo {
  id: string
  projectId: string
  name: string
  type: string
  groupId: string
  archived: boolean
  createdAt: number
  description: string
  tags: string[]
  moduleId: string | null
  startAt: number | null
  endAt: number | null
  allowDuplicateCases: boolean
  autoUpdateStatus: boolean
  /** Percent 0-100. */
  passThreshold: number
  planning: PlanningDoc | null
}

/** Absent fields keep current values; moduleId '' clears, groupId ''/'NONE' = root, startAt/endAt <= 0 clears. */
export interface PlanUpdateBody {
  name?: string
  description?: string
  tags?: string[]
  moduleId?: string
  groupId?: string
  startAt?: number
  endAt?: number
  allowDuplicateCases?: boolean
  autoUpdateStatus?: boolean
  passThreshold?: number
  /** true archives the plan (hidden from the list), false restores it. */
  archived?: boolean
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

/** Requirement pipeline stages (fixed 7, in order: created → audit → review → dev → test → acceptance → delivery). */
export type RequirementStageKey = 'CREATED' | 'AUDIT' | 'REVIEW' | 'DEV' | 'TEST' | 'ACCEPTANCE' | 'DELIVERY'
export type RequirementStageStatus = 'PENDING' | 'IN_PROGRESS' | 'DONE' | 'SKIPPED'

/** Requirement stage detail (backend always returns all 7 stages per requirement, in pipeline order). */
export interface RequirementStage {
  stage: RequirementStageKey
  status: RequirementStageStatus
  /** Planned start/end (YYYY-MM-DD); null = not scheduled. */
  plannedStart: string | null
  plannedEnd: string | null
  /** Actual start/end (epoch ms); null = hasn't happened. */
  startedAt: number | null
  finishedAt: number | null
  /** Planned end passed but stage not done → overdue. */
  overdue: boolean
}

export interface Requirement {
  id: string
  projectId?: string
  title: string
  /** P0-P3. */
  priority?: string
  /** FEATURE | ENHANCEMENT | TECH_DEBT | BUGFIX. */
  reqType?: string
  baselineVersion: number
  latestVersion?: number
  status: string
  // Supports both response shapes: top-level acceptanceCriteria (old) or versions[].acceptanceCriteria (current).
  acceptanceCriteria?: string[]
  versions?: RequirementVersion[]
  /** Tags (max 10). */
  tags?: string[]
  /** Parent requirement id; null = top-level. */
  parentId?: string | null
  /** Due date (YYYY-MM-DD); null = unset. */
  dueDate?: string | null
  /** Any stage overdue or overall due date passed → overdue. */
  overdue?: boolean
  /** Create/update time (epoch ms). */
  createdAt?: number
  updatedAt?: number
  /** Creator; may be empty on legacy data. */
  createdBy?: string
  currentStage?: RequirementStageKey
  stages?: RequirementStage[]
  /** Custom field values from the field template (key → string value; multiselect joined with commas). */
  customFields?: Record<string, string>
  /** Module id (shared project module tree); empty/missing = unplanned. */
  moduleId?: string
  /** Selected project skill ids (composed into agent instructions at dispatch time). */
  skillIds?: string[]
}

/** Field types in a field template (drives the create-form control). */
export type TemplateFieldType = 'text' | 'textarea' | 'select' | 'multiselect' | 'date' | 'number'

/** One field in a field template (system fields have fixed keys; custom field keys are c_-prefixed short ids). */
export interface TemplateField {
  key: string
  /** Display name for custom fields; empty for system fields, which render via i18n. */
  label: string
  type: TemplateFieldType
  required: boolean
  /** false = hidden in the create form. */
  enabled: boolean
  system: boolean
  /** Options for select/multiselect. */
  options?: string[]
}

/** Field template config: stored as opaque JSON on the backend; array order = form order. */
export interface FieldTemplateConfig {
  fields: TemplateField[]
}

/** Project-level template: kind selects the module (requirement / functional-case / bug); config shape depends on kind. */
export interface ProjectTemplate {
  id: string
  projectId: string
  kind: string
  name: string
  config: FieldTemplateConfig
  createdBy: string
  createdAt: number
  updatedAt: number
}

/** Requirement field change record (time/actor/field/old/new). */
export interface RequirementChange {
  changedAt: number
  changedBy: string
  field: string
  oldValue: string
  newValue: string
}

export interface Task {
  id: string
  title: string
  description?: string
  status: string
  acceptanceCriteria?: string[]
  dependencies?: string[]
  /** Effort (task points); 0 = unestimated. */
  points?: number
  /** Assignee id/name; empty = unassigned. */
  assignee?: string
  /** Assignee kind: HUMAN / AGENT. */
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
  /** Target runtime name for directed dispatch; empty = any runtime with the capability. */
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

/** Dispatchable executor type → display name (shared by task-center filter and dispatch picker). */
export const EXECUTOR_LABEL: Record<string, string> = {
  CLAUDE_CODE: 'Claude Code',
  CODEX: 'Codex',
  OPENCODE: 'OpenCode',
  CODEBUDDY: 'CodeBuddy',
}

/** One task-center row: system-wide delivery attempts aggregated by status/executor/result/completion rate. */
export interface TaskCenterItem {
  id: string
  decompositionId: string
  taskId: string
  title: string
  /** Task description (empty string when no linked task). */
  description: string
  /** Module (title of the owning requirement; empty string when none). */
  module: string
  /** Executor: CLAUDE_CODE / CODEX / OPENCODE / CODEBUDDY. */
  executor: string
  /** Target runtime name for directed dispatch; empty = any runtime with the capability. */
  targetRuntime?: string | null
  /** Execution status: DISPATCHED / RUNNING / DELIVERED / FAILED / STOPPED. */
  status: string
  /** Result: SUCCESS / FAILED / STOPPED / PENDING. */
  result: string
  /** Completion rate, 0..100. */
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
  /** Live tasks only (not in a terminal state). */
  active?: boolean
  q?: string
  page?: number
  pageSize?: number
}

/** Runner machine / AI agent (remote executors like Claude Code, Codex); self-reports supported protocols at registration. */
export interface RunnerAgent {
  id: string
  name: string
  baseUrl: string
  enabled: boolean
  protocols: string[]
}

// AI executor fleet: remote Claude/Codex runtimes; outbound register/heartbeat, server derives liveness from heartbeats.
export interface FleetRuntime {
  id: string
  name: string
  caps: string[]
  maxConcurrency: number
  lastSeenMs: number
  online: boolean
}

// Queue counters (fleet observability): per-capability backlog / in-flight / oldest in-flight age.
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

/** Verification gap: a criterion either not covered (UNCOVERED) or covered but not verified by a delivery (UNVERIFIED). */
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

/** Case review (list row): pass rule + counts + header meta (name/tags/module/schedule/creator). */
export interface CaseReviewSummary {
  id: string
  passRule: string
  reviewerCount: number
  total: number
  passed: number
  createdAt: string
  status: string
  name: string
  description: string
  tags: string[]
  moduleId?: string | null
  startAt?: string | null
  endAt?: string | null
  createdBy?: string | null
  reviewers: string[]
}
/** Editable review header fields (create extras + PUT body). */
export interface CaseReviewMetaInput {
  name: string
  description?: string
  tags?: string[]
  moduleId?: string | null
  startAt?: string | null
  endAt?: string | null
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
  createdBy?: string | null
  /** Severity level P0..P3 (P0 highest). */
  severity?: string | null
  /** Handler user id. */
  handler?: string | null
  /** Last mutator's user id (stamped on every mutation). */
  updatedBy?: string | null
  /** Last mutation time (timestamptz text). */
  updatedAt?: string | null
}

/** Human-AI collaboration stats: per-requirement AI/human delivery split (measured as VERIFIED + presence of a DELIVERED attempt). */
export interface CollabRequirementItem {
  requirementId: string
  title: string
  aiTasks: number
  humanTasks: number
  aiPoints: number
  humanPoints: number
  /** Delivery quality (attempt level): total/success/failed attempts; tasks accepted on the first delivery. */
  aiAttempts: number
  aiDelivered: number
  aiFailed: number
  aiFirstPass: number
}
export interface CollabDayItem {
  /** Date, YYYY-MM-DD. */
  date: string
  ai: number
  human: number
}
export interface CollabStats {
  items: CollabRequirementItem[]
  daily: CollabDayItem[]
}

export interface BugRelation {
  /** REQUIREMENT | SCENARIO | FUNCTIONAL_CASE | PLAN. */
  kind: string
  targetId: string
}

/** Follow status: an entity's follower list + whether the current user follows it (read back from /follow). */
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
  /** Per-phase HTTP timings (absent for non-HTTP protocols). */
  timings?: PhaseTimings | null
  headers: [string, string][]
  body: string
  /** Per-assertion results (evaluated on server-side runs; absent for local runs). */
  assertions?: AssertionResult[]
  /** Extracted variables (name, value). */
  extractions?: [string, string][]
}

export type RunMode = 'PARALLEL' | 'SERIAL'

/** In-app notification (message center). */
export interface Notice {
  id: string
  projectId: string
  /** PLAN | BUG | CASE | API | SCHEDULE (comment mentions may carry other entity categories). */
  category: string
  eventType: string
  title: string
  content: string
  resourceType: string
  resourceId: string
  operator: string
  atMention: boolean
  read: boolean
  /** Epoch millis. */
  createdAt: number
}

export interface NoticePage {
  items: Notice[]
  total: number
}

export interface NoticeUnreadCount {
  total: number
  byCategory: Record<string, number>
}

export type NoticeRobotPlatform = 'FEISHU' | 'DINGTALK' | 'WECOM'
export type NoticeChannel = 'IN_APP' | 'ROBOT'

/** Webhook robot (Feishu / DingTalk / WeCom) receiving notification events. */
export interface NoticeRobot {
  id: string
  projectId: string
  name: string
  platform: NoticeRobotPlatform
  webhookUrl: string
  /** DingTalk sign secret (empty = no signing). */
  secret: string
  enabled: boolean
  /** Epoch millis. */
  createdAt: number
}

/** Server-side notification rule: routes an event type to channels/robots. */
export interface NoticeRule {
  id: string
  projectId: string
  /** Producer event type or '*' for all. */
  eventType: string
  channels: NoticeChannel[]
  robotIds: string[]
  /** Supports ${title} ${operator} ${time}; empty uses the default text. */
  template: string
  enabled: boolean
  /** Epoch millis. */
  createdAt: number
}

export interface NoticeRobotTestResult {
  status: number
  body: string
}

// ---------- Endpoint wrappers ----------

const emptyPage = <T>(): Page<T> => ({ total: 0, current: 1, pageSize: 0, totalPages: 0, items: [] })

export const api = {
  login: (username: string, password: string) =>
    http.post<{ token: string; userId?: string }>('/auth/login', { username, password }),

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

  // List views (saved filter/column/pagination snapshots; own + shared).
  views: (projectId: string) =>
    projectId
      ? http.get<ApiView[]>(`/api/api-view?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as ApiView[]),
  createView: (b: { projectId: string; name: string; config: unknown; shared?: boolean }) =>
    http.post<ApiView>('/api/api-view', b),
  updateView: (id: string, b: { name?: string; config?: unknown; shared?: boolean }) =>
    http.put<ApiView>(`/api/api-view/${id}`, b),
  deleteView: (id: string) => http.del(`/api/api-view/${id}`),
  getDefinition: (id: string) => http.get<ApiDefinition>(`/api/definition/${id}`),
  // Update definition base fields (name/protocol/method/path); omitted fields keep their value; returns the updated definition.
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
  // File/paste import: OpenAPI/Postman/HAR/MeterSphere send a JSON object; JMeter (.jmx XML) sends the raw text string.
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
  // URL import: server fetches the source URL (sidesteps browser CORS) and imports by format; returns created/updated/skipped counts.
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

  // Scheduled imports
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

  // API modules (folders)
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

  // Task ↔ case links (task → case direction)
  taskCases: (decompositionId: string, taskId: string) =>
    http.get<ApiCase[]>(`/api/task-case?decompositionId=${encodeURIComponent(decompositionId)}&taskId=${encodeURIComponent(taskId)}`),
  linkTaskCase: (decompositionId: string, taskId: string, caseId: string) =>
    http.post('/api/task-case', { decompositionId, taskId, caseId }),
  unlinkTaskCase: (decompositionId: string, taskId: string, caseId: string) =>
    http.post('/api/task-case/unlink', { decompositionId, taskId, caseId }),
  // Case → plan reverse lookup
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

  // Project members
  projectMembers: (projectId: string) =>
    http.get<ProjectMember[]>(`/project/${encodeURIComponent(projectId)}/member`),
  // Resolve user ids -> display names (id-only members become usable labels).
  userNames: (ids: string[]) =>
    ids.length
      ? http.get<Record<string, string>>(`/system/user/names?ids=${encodeURIComponent(ids.join(','))}`)
      : Promise.resolve({} as Record<string, string>),
  addProjectMember: (projectId: string, b: { userId: string; role?: string }) =>
    http.post<ProjectMember>(`/project/${encodeURIComponent(projectId)}/member`, b),
  removeProjectMember: (projectId: string, userId: string) =>
    http.del(`/project/${encodeURIComponent(projectId)}/member/${encodeURIComponent(userId)}`),

  // Project templates (kind selects the purpose, currently only requirement; name unique per project+kind, duplicate → 409)
  projectTemplates: (projectId: string, kind: string) =>
    http.get<{ items: ProjectTemplate[] }>(`/project/${encodeURIComponent(projectId)}/template?kind=${encodeURIComponent(kind)}`),
  createProjectTemplate: (projectId: string, b: { kind: string; name: string; config: FieldTemplateConfig }) =>
    http.post<ProjectTemplate>(`/project/${encodeURIComponent(projectId)}/template`, b),
  updateProjectTemplate: (id: string, b: { name?: string; config?: FieldTemplateConfig }) =>
    http.put<ProjectTemplate>(`/template/${encodeURIComponent(id)}`, b),
  deleteProjectTemplate: (id: string) => http.del(`/template/${encodeURIComponent(id)}`),

  // Roles / users (platform level)
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

  // API keys (system level; create response carries the one-time plaintext key, revoke = DELETE)
  apiKeys: () => http.get<{ items: ApiKey[] }>('/system/apikey'),
  createApiKey: (b: { name: string; permissions: string[] }) => http.post<ApiKey>('/system/apikey', b),
  revokeApiKey: (id: string) => http.del(`/system/apikey/${encodeURIComponent(id)}`),

  // Personal center: identity / password change / my API keys / my model configs
  me: () => http.get<AuthMe>('/auth/me'),
  changePassword: (b: { oldPassword: string; newPassword: string }) => http.post<void>('/auth/password', b),
  myApiKeys: () => http.get<{ items: ApiKey[] }>('/system/apikey/mine'),
  createMyApiKey: (b: { name?: string; ttlSecs?: number }) => http.post<ApiKey>('/system/apikey/mine', b),
  setApiKeyEnabled: (id: string, enabled: boolean) =>
    http.put(`/system/apikey/${encodeURIComponent(id)}/enabled`, { enabled }),
  llmModels: () => http.get<{ items: LlmModel[] }>('/me/llm-model'),
  createLlmModel: (b: { provider: string; name: string; baseUrl?: string; apiKey?: string }) =>
    http.post<LlmModel>('/me/llm-model', b),
  updateLlmModel: (id: string, b: { name?: string; baseUrl?: string; apiKey?: string; enabled?: boolean }) =>
    http.put<LlmModel>(`/me/llm-model/${encodeURIComponent(id)}`, b),
  deleteLlmModel: (id: string) => http.del(`/me/llm-model/${encodeURIComponent(id)}`),

  // RAG config (system-level, /system/rag/config): keys are write-only; GET returns *KeySet booleans.
  ragConfig: () => http.get<RagConfigView>('/system/rag/config'),
  saveRagConfig: (b: RagConfigBody) => http.put('/system/rag/config', b),

  // Functional cases (project level)
  functionalCases: (projectId: string) =>
    projectId
      ? http.get<FunctionalCase[]>(`/functional-case?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as FunctionalCase[]),
  createFunctionalCase: (b: {
    projectId: string
    name: string
    priority?: string
    module?: string
    tags?: string[]
    steps?: CaseStep[]
    customFields?: Record<string, string>
  }) => http.post<FunctionalCase>('/functional-case', b),
  // Full update of a functional case (PUT semantics: omitted fields reset to defaults — callers must send everything).
  updateFunctionalCase: (
    id: string,
    b: {
      projectId: string
      name: string
      priority?: string
      status?: string
      module?: string
      tags?: string[]
      steps?: CaseStep[]
      customFields?: Record<string, string>
    },
  ) => http.put<FunctionalCase>(`/functional-case/${id}`, b),
  getFunctionalCase: (id: string) => http.get<FunctionalCase>(`/functional-case/${id}`),
  deleteFunctionalCase: (id: string) => http.del<void>(`/functional-case/${id}`),
  // Export xlsx (binary download) / import xlsx (raw byte upload, returns imported count).
  exportFunctionalCases: (projectId: string) =>
    http.getBlob(`/functional-case/export?projectId=${encodeURIComponent(projectId)}`),
  importFunctionalCases: (projectId: string, file: File) =>
    http.upload<{ imported: number }>(`/functional-case/import?projectId=${encodeURIComponent(projectId)}`, file),
  // Requirement ↔ functional-case coverage links.
  linkRequirementCase: (b: { requirementId: string; criterionIndex: number; functionalCaseId: string; projectId?: string }) =>
    http.post('/requirement-case/link', b),
  unlinkRequirementCase: (b: { requirementId: string; criterionIndex: number; functionalCaseId: string }) =>
    http.post('/requirement-case/unlink', b),
  requirementCoverage: (requirementId: string) =>
    http.get<CoverageCase[]>(`/requirement/${requirementId}/functional-coverage`),
  caseRequirements: (caseId: string) =>
    http.get<CaseRequirementLink[]>(`/functional-case/${caseId}/requirements`),
  // Case detail drawer companions: audit trail, linked bugs/reviews/plans, pre/post dependencies.
  caseChanges: (caseId: string) => http.get<CaseChange[]>(`/functional-case/${caseId}/changes`),
  caseBugs: (caseId: string) => http.get<CaseBugLink[]>(`/functional-case/${caseId}/bugs`),
  caseReviewLinks: (caseId: string) => http.get<CaseReviewLink[]>(`/functional-case/${caseId}/reviews`),
  casePlanLinks: (caseId: string) => http.get<CasePlanLink[]>(`/functional-case/${caseId}/plans`),
  caseDependencies: (caseId: string, direction: 'PRE' | 'POST') =>
    http.get<CaseDependencyLink[]>(`/functional-case/${caseId}/dependencies?direction=${direction}`),
  addCaseDependency: (caseId: string, b: { targetCaseId: string; direction: 'PRE' | 'POST'; projectId: string }) =>
    http.post(`/functional-case/${caseId}/dependencies`, b),
  removeCaseDependency: (caseId: string, targetId: string, direction: 'PRE' | 'POST') =>
    http.del(`/functional-case/${caseId}/dependencies/${targetId}?direction=${direction}`),
  // Generic comments (comment crate): list/add/delete by (targetType, targetId).
  comments: (targetType: string, targetId: string) =>
    http.get<CommentItem[]>(
      `/comment?targetType=${encodeURIComponent(targetType)}&targetId=${encodeURIComponent(targetId)}`,
    ),
  addComment: (b: { targetType: string; targetId: string; content: string }) =>
    http.post<CommentItem>('/comment', b),
  deleteComment: (id: string) => http.del(`/comment/${id}`),

  // Project API cases (for test-plan case selection)
  projectCases: (projectId: string) =>
    projectId
      ? http.get<Page<ApiCase>>(`/api/case?projectId=${encodeURIComponent(projectId)}&pageSize=100`)
      : Promise.resolve(emptyPage<ApiCase>()),
  // Page through all project cases (for the scenario step id→name map: a single page misses cases when there are >100, making step names fall back to short ids).
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

  // Test plans
  listPlans: (projectId: string) =>
    projectId
      ? http.get<PlanListItem[]>(`/test-plan?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as PlanListItem[]),
  createPlan: (b: { projectId: string; name: string; type?: string }) =>
    http.post<TestPlan>('/test-plan', { type: 'TEST_PLAN', ...b }),
  planDetail: (id: string) => http.get<PlanDetailInfo>(`/test-plan/${id}`),
  updatePlan: (id: string, b: PlanUpdateBody) => http.put<PlanDetailInfo>(`/test-plan/${id}`, b),
  savePlanPlanning: (id: string, doc: PlanningDoc) =>
    http.put<{ linkedCases: number }>(`/test-plan/${id}/planning`, doc),
  planStats: (id: string) => http.get<PlanStats>(`/test-plan/${id}/statistics`),
  planCases: (id: string) => http.get<PlanCase[] | Page<PlanCase>>(`/test-plan/${id}/cases`),
  linkPlanCase: (id: string, caseId: string, name: string) =>
    http.post(`/test-plan/${id}/cases`, { caseId, name }),
  unlinkPlanCase: (id: string, caseId: string) => http.del(`/test-plan/${id}/cases/${caseId}`),
  // Runs exactly one linked case/scenario and records its result. Scenario
  // entries auto-route to an applicable pool with online runners (or local).
  // asyncRun: scenario entries return RUNNING + reportId immediately (live
  // events on runEventsWsUrl(reportId), row recorded at completion); plain API
  // cases always complete synchronously.
  runPlanCase: (id: string, caseId: string, opts?: { asyncRun?: boolean }) =>
    http.post<{ caseId: string; status: string; reportId?: string | null; executedOn?: ExecutedOn | null }>(`/test-plan/${id}/cases/${caseId}/run`, { asyncRun: opts?.asyncRun }),
  // Manually record a case result (pass/fail/blocked/false alarm); status: SUCCESS|ERROR|BLOCK|FAKE_ERROR|PENDING
  recordPlanCaseResult: (id: string, caseId: string, status: string) =>
    http.post(`/test-plan/${id}/cases/${caseId}/result`, { status }),
  runPlan: (id: string, environmentId?: string, poolId?: string) =>
    http.post<{ status?: string; total: number; executed: number }>(`/test-plan/${id}/run`, { environmentId, poolId }),
  planSchedule: (id: string, cron: string, enabled = true) =>
    http.post(`/test-plan/${id}/schedule`, { cron, enabled }),
  deletePlanSchedule: (id: string) => http.del(`/test-plan/${id}/schedule`),
  planRuns: (id: string) => http.get<unknown[]>(`/test-plan/${id}/runs`),
  planReportMd: (id: string) => http.getText(`/test-plan/${id}/report.md`),
  sharePlanReport: (id: string) => http.post<{ token: string }>(`/test-plan/${id}/report/share`),
  publicPlanReportMd: (token: string) => http.getText(`/public/test-plan-report/${token}`),

  // Perf testing (no list endpoint → report list lives in the frontend registry)
  runPerf: (b: {
    projectId: string
    method: string
    url: string
    concurrency: number
    iterations?: number
    durationMs?: number
  }) => http.post<{ reportId: string; status: string }>('/perf/run', b),
  // Scenario perf: one load unit = running the full scenario chain (login → extract → authed calls → …); report reuses PerfReport.
  runScenarioPerf: (b: {
    projectId: string
    scenarioId: string
    environmentId?: string
    concurrency: number
    iterations?: number
    durationMs?: number
  }) => http.post<{ reportId: string; status: string; stepCount: number }>('/perf/scenario/run', b),
  perfReport: (id: string) => http.get<PerfReport>(`/perf/report/${id}`),

  // Requirements (versions / baseline / breakdown) — no list endpoint, list uses the frontend registry
  createRequirement: (b: { projectId: string; title: string; description?: string; acceptanceCriteria: string[]; priority?: string; reqType?: string; tags?: string[]; dueDate?: string; parentId?: string; customFields?: Record<string, string>; moduleId?: string; skillIds?: string[] }) =>
    http.post<Requirement>('/requirement', b),
  /** MRD/raw material → structured requirement draft (AI-drafted if an LLM is configured, heuristic otherwise). */
  draftRequirement: (raw: string) =>
    http.post<{ title: string; description: string; acceptanceCriteria: string[]; priority: string; source: 'llm' | 'heuristic' }>('/requirement/draft', { raw }),
  getRequirement: (id: string) => http.get<Requirement>(`/requirement/${id}`),
  // List comes from the backend (paged); large pageSize pulls everything at once so CLI/API-created requirements show in the UI.
  requirements: (projectId: string) =>
    http.get<Page<Requirement>>(`/requirement?projectId=${encodeURIComponent(projectId)}&current=1&pageSize=200`),
  addRequirementVersion: (id: string, b: { description: string; acceptanceCriteria: string[] }) =>
    http.post<{ version: number }>(`/requirement/${id}/version`, b),
  getRequirementVersion: (id: string, n: number) =>
    http.get<RequirementVersion>(`/requirement/${id}/version/${n}`),
  /** Edit requirement basics: title required; others optional and untouched when omitted; empty-string dueDate clears it; customFields replaced wholesale. */
  updateRequirement: (id: string, b: { title: string; priority?: string; reqType?: string; tags?: string[]; dueDate?: string; customFields?: Record<string, string>; moduleId?: string }) =>
    http.put<Requirement>(`/requirement/${id}`, b),
  renameRequirement: (id: string, title: string) =>
    http.put<Requirement>(`/requirement/${id}`, { title }),
  deleteRequirement: (id: string) => http.del(`/requirement/${id}`),
  archiveRequirement: (id: string) => http.post<Requirement>(`/requirement/${id}/archive`, {}),
  deliverRequirement: (id: string) => http.post<Requirement>(`/requirement/${id}/deliver`, {}),
  setBaseline: (id: string, version: number) =>
    http.put<Requirement>(`/requirement/${id}/baseline`, { version }),
  breakdown: (id: string, version?: number) =>
    http.post<{ id: string; verificationId: string; tasks: Task[] }>(
      `/requirement/${id}/breakdown` + (version != null ? `?version=${version}` : ''),
      {},
    ),
  /** Read-only: fetch the existing decomposition for a requirement version (404 if none). Used to restore the orchestration tab across browsers. */
  requirementBreakdown: (reqId: string, version?: number) =>
    http.get<{ id: string; requirementVersion: number; verificationId?: string }>(
      `/requirement/${reqId}/breakdown` + (version != null ? `?version=${version}` : ''),
    ),
  /** Stage transition/scheduling: status moves the stage (IN_PROGRESS/DONE/SKIPPED); plannedStart/plannedEnd schedule it (empty string clears); returns the fresh requirement. */
  setRequirementStage: (id: string, stage: string, b: { status?: string; plannedStart?: string; plannedEnd?: string }) =>
    http.put<Requirement>(`/requirement/${id}/stage/${stage}`, b),
  /** Set/unset parent requirement (parentId=null unsets). */
  setRequirementParent: (id: string, parentId: string | null) =>
    http.put<Requirement>(`/requirement/${id}/parent`, { parentId }),
  requirementChildren: (id: string) =>
    http.get<{ items: Requirement[] }>(`/requirement/${id}/children`),
  /** Field change records (sort order decided by the backend). */
  requirementChanges: (id: string) =>
    http.get<{ items: RequirementChange[] }>(`/requirement/${id}/changes`),

  // Decomposition / tasks
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

  // Delivery
  createDelivery: (b: {
    decompositionId: string
    taskId: string
    title: string
    description?: string
    acceptanceCriteria?: string[]
    executor: string
    targetRuntime?: string
    context?: string
    instructions?: string
  }) => http.post<DeliveryAttempt>('/delivery', b),
  deliveries: (decompositionId: string, taskId: string) =>
    http.get<DeliveryAttempt[]>(
      `/delivery?decompositionId=${encodeURIComponent(decompositionId)}&taskId=${encodeURIComponent(taskId)}`,
    ),
  deliveryEvents: (attemptId: string) => http.get<DeliveryEvent[]>(`/delivery/${attemptId}/events`),

  // Task center (system level: background/live task lists + execution detail + stop/delete)
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

  // Runner / AI agent management (the executor side of human-AI collaboration)
  // AI executor fleet (SHEPHERD_AGENT_FLEET mode): list remote runtimes + online status.
  fleetRuntimes: () => http.get<FleetRuntime[]>('/agent/runtime'),
  // Fleet queue counters: per-capability backlog/in-flight, feeds the fleet view's backlog overview.
  fleetStats: () => http.get<FleetStat[]>('/agent/work/stats'),
  runnerAgents: () => http.get<RunnerAgent[]>('/runner-agent'),
  registerRunnerAgent: (b: { name: string; baseUrl: string; token?: string; enabled?: boolean }) =>
    http.post<RunnerAgent>('/runner-agent', b),
  refreshRunnerAgent: (id: string) => http.post<string[]>(`/runner-agent/${id}/refresh`, {}),
  runnerExecutions: (id: string) => http.get<RunnerExecution[]>(`/runner-agent/${id}/executions`),

  // Verification (coverage chain / report)
  verificationReport: (id: string) => http.get<VerificationReport>(`/verification/${id}/report`),
  verificationLink: (id: string, b: { criterionIndex: number; decompositionId: string; taskId: string }) =>
    http.post(`/verification/${id}/link`, b),
  verificationSync: (id: string, b: { decompositionId: string; taskId: string; satisfied: boolean }) =>
    http.post(`/verification/${id}/sync`, b),

  // Case review queues (create/list/detail/submit verdict)
  caseReviews: (projectId: string) =>
    projectId ? http.get<CaseReviewSummary[]>(`/case-review?projectId=${encodeURIComponent(projectId)}`) : Promise.resolve([] as CaseReviewSummary[]),
  createCaseReview: (b: { projectId: string; passRule: string; reviewerCount: number; caseIds: string[] } & Partial<CaseReviewMetaInput>) =>
    http.post<{ id: string }>('/case-review', b),
  updateCaseReview: (id: string, b: CaseReviewMetaInput & { passRule: string; reviewerCount: number }) =>
    http.put(`/case-review/${id}`, b),
  caseReview: (id: string) => http.get<CaseReviewDetail>(`/case-review/${id}`),
  deleteCaseReview: (id: string) => http.del(`/case-review/${id}`),
  submitCaseReview: (reviewId: string, caseId: string, b: { reviewerId: string; status: string; content?: string }) =>
    http.post<{ status: string }>(`/case-review/${reviewId}/${caseId}`, b),

  // Bugs — list/create/status transitions all backend-driven (project-scoped, newest first)
  bugs: (projectId: string) =>
    projectId ? http.get<Bug[]>(`/bug?projectId=${encodeURIComponent(projectId)}`) : Promise.resolve([] as Bug[]),
  createBug: (b: { projectId: string; title: string; initialStatus: string; severity?: string; handler?: string; customFields?: Record<string, string> }) => http.post<Bug>('/bug', b),
  // Meta update: severity/handler are full replacements (omit to clear); omitted title keeps the current one.
  updateBug: (id: string, b: { title?: string; severity?: string; handler?: string }) => http.put<Bug>(`/bug/${encodeURIComponent(id)}`, b),
  setBugStatus: (id: string, status: string) => http.post<Bug>(`/bug/${id}/status`, { status }),

  // In-app notifications (message center): always scoped to the current session user.
  notices: (q: { projectId?: string; category?: string; tab?: string; page?: number; pageSize?: number }) => {
    const params = new URLSearchParams()
    if (q.projectId) params.set('projectId', q.projectId)
    if (q.category) params.set('category', q.category)
    if (q.tab) params.set('tab', q.tab)
    if (q.page) params.set('page', String(q.page))
    if (q.pageSize) params.set('pageSize', String(q.pageSize))
    return http.get<NoticePage>(`/notice?${params.toString()}`)
  },
  noticeUnreadCount: (projectId?: string) =>
    http.get<NoticeUnreadCount>(`/notice/unread-count?projectId=${encodeURIComponent(projectId || '')}`),
  markNoticeRead: (id: string) => http.post(`/notice/${encodeURIComponent(id)}/read`),
  markAllNoticesRead: (projectId?: string) =>
    http.post(`/notice/read-all?projectId=${encodeURIComponent(projectId || '')}`),

  // Notification settings: webhook robots + server-side routing rules (project-scoped)
  noticeRobots: (projectId: string) =>
    http.get<NoticeRobot[]>(`/notice/robots?projectId=${encodeURIComponent(projectId)}`),
  createNoticeRobot: (b: { projectId: string; name: string; platform: NoticeRobotPlatform; webhookUrl: string; secret?: string; enabled?: boolean }) =>
    http.post<NoticeRobot>('/notice/robots', b),
  updateNoticeRobot: (id: string, b: { projectId: string; name: string; platform: NoticeRobotPlatform; webhookUrl: string; secret?: string; enabled?: boolean }) =>
    http.put<NoticeRobot>(`/notice/robots/${encodeURIComponent(id)}`, b),
  deleteNoticeRobot: (id: string, projectId: string) =>
    http.del(`/notice/robots/${encodeURIComponent(id)}?projectId=${encodeURIComponent(projectId)}`),
  testNoticeRobot: (id: string, projectId: string) =>
    http.post<NoticeRobotTestResult>(`/notice/robots/${encodeURIComponent(id)}/test?projectId=${encodeURIComponent(projectId)}`),
  noticeRules: (projectId: string) =>
    http.get<NoticeRule[]>(`/notice/rules?projectId=${encodeURIComponent(projectId)}`),
  createNoticeRule: (b: { projectId: string; eventType: string; channels: NoticeChannel[]; robotIds?: string[]; template?: string; enabled?: boolean }) =>
    http.post<NoticeRule>('/notice/rules', b),
  updateNoticeRule: (id: string, b: { projectId: string; eventType: string; channels: NoticeChannel[]; robotIds?: string[]; template?: string; enabled?: boolean }) =>
    http.put<NoticeRule>(`/notice/rules/${encodeURIComponent(id)}`, b),
  deleteNoticeRule: (id: string, projectId: string) =>
    http.del(`/notice/rules/${encodeURIComponent(id)}?projectId=${encodeURIComponent(projectId)}`),
  // Human-AI collaboration stats (per-requirement AI/human split + weekly trend)
  collabStats: (projectId: string, requirementId?: string) =>
    http.get<CollabStats>(
      `/delivery/collab-stats?projectId=${encodeURIComponent(projectId)}${requirementId ? `&requirementId=${encodeURIComponent(requirementId)}` : ''}`,
    ),

  // Bug ↔ asset links (requirement/scenario case/functional case/test plan)
  bugRelations: (id: string) =>
    http.get<{ relations: BugRelation[] }>(`/bug/${encodeURIComponent(id)}/relation`),
  linkBugRelation: (id: string, b: { kind: string; targetId: string }) =>
    http.post<{ relations: BugRelation[] }>(`/bug/${encodeURIComponent(id)}/relation`, b),
  unlinkBugRelation: (id: string, kind: string, targetId: string) =>
    http.del<{ relations: BugRelation[] }>(
      `/bug/${encodeURIComponent(id)}/relation/${encodeURIComponent(kind)}/${encodeURIComponent(targetId)}`,
    ),
  // Reverse lookup: bugs linked to a test plan (kind = PLAN), newest first.
  bugsByPlan: (planId: string) => http.get<Bug[]>(`/bug/by-plan/${encodeURIComponent(planId)}`),

  // Followers (generic): follow/unfollow/query any entity by (projectId, entityType, entityId).
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

  // Skills — list/detail/update/delete backend-driven
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

  // API debug console: fire a request in-process (POST /api/debug/send)
  debugSend: (b: { protocol?: string; method: string; url: string; headers?: { key: string; value: string }[]; body?: string; meta?: Record<string, string>; assertions?: unknown[]; processors?: unknown[] }) =>
    http.post<DebugResponse>('/api/debug/send', b),
  // Protocol plugins enabled on the backend (returns whichever features are compiled in); debug console renders them dynamically.
  debugProtocols: () => http.get<{ protocols: string[] }>('/api/debug/protocols'),

  // MCP tools
  mcpTools: () =>
    http.post<{ result: { tools: McpTool[] } }>('/mcp', { jsonrpc: '2.0', id: 1, method: 'tools/list' }),

  // Environments (project level)
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
  // Replaces the step payload; ordering still goes through reorderScenarioSteps.
  updateScenarioStep: (
    scenarioId: string,
    stepId: string,
    b: { kind: string; refId?: string; request?: unknown; control?: unknown },
  ) => http.patch<ScenarioStep>(`/api/scenario/${scenarioId}/step/${stepId}`, b),
  deleteScenarioStep: (scenarioId: string, stepId: string) =>
    http.del(`/api/scenario/${scenarioId}/step/${stepId}`),
  reorderScenarioSteps: (scenarioId: string, order: string[]) =>
    http.patch(`/api/scenario/${scenarioId}/steps/order`, { order }),
  copyScenario: (scenarioId: string, name?: string) =>
    http.post<Scenario>(`/api/scenario/${scenarioId}/copy`, { name }),
  // Recycle bin: soft-deleted scenarios keep their steps, so restore is lossless; purge is final.
  recycleScenarios: (projectId: string) =>
    projectId
      ? http.get<Scenario[]>(`/api/scenario/recycle?projectId=${encodeURIComponent(projectId)}`)
      : Promise.resolve([] as Scenario[]),
  restoreScenario: (id: string) => http.post<void>(`/api/scenario/${id}/restore`),
  purgeScenario: (id: string) => http.del<void>(`/api/scenario/${id}/purge`),
  // Batch scenario execution: serial/parallel, optional env override, union report, pool-bound concurrency.
  batchRunScenarios: (b: {
    projectId: string
    scenarioIds: string[]
    environmentId?: string
    mode?: 'SERIAL' | 'PARALLEL'
    stopOnFail?: boolean
    unionReport?: boolean
    reportName?: string
    poolId?: string
  }) =>
    http.post<{
      status: string
      reportId?: string
      total: number
      success: number
      results: { scenarioId: string; reportId?: string | null; status: string }[]
    }>('/api/scenario/batch-run', b),
  runScenario: (scenarioId: string, projectId: string, opts?: { environmentId?: string; failureStrategy?: 'CONTINUE' | 'STOP'; poolId?: string; asyncRun?: boolean }) =>
    http.post<ScenarioRunResult>(`/api/scenario/${scenarioId}/run`, { projectId, environmentId: opts?.environmentId, failureStrategy: opts?.failureStrategy, poolId: opts?.poolId, asyncRun: opts?.asyncRun }),
  /** Online pool-runner count per resource pool (in-memory WS registry). */
  poolRunnerStatus: () => http.get<Record<string, number>>('/api/pool-runner/status'),
  /** Per-pool connected runner details (name / cap / in-flight). */
  poolRunnerStatusDetail: () =>
    http.get<Record<string, PoolRunnerInfo[]>>('/api/pool-runner/status/detail'),
  scenarioExecutions: (scenarioId: string) =>
    http.get<Page<ScenarioExecution>>(`/api/scenario/${scenarioId}/executions`),
  scenarioReport: (reportId: string) =>
    http.get<ScenarioReportDetail>(`/api/scenario-report/${reportId}`),
  // Public share: mint an unguessable token, then read the report anonymously (no auth) by that token.
  // `scenarioId` is remembered so the public page can render the same step tree as the in-app report.
  shareScenarioReport: (reportId: string, scenarioId?: string) =>
    http.post<{ token: string }>(`/api/scenario-report/${reportId}/share`, { scenarioId }),
  publicScenarioReport: (token: string) =>
    http.get<ScenarioReportDetail>(`/public/scenario-report/${token}`),
  // Token-guarded public read of a scenario's structure (for the shared report's step tree).
  publicScenario: (token: string, id: string) =>
    http.get<Scenario & { steps: ScenarioStep[] }>(`/public/scenario/${token}/${id}`),
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
