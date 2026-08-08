import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Button, Drawer, Empty, Input, Modal, Popconfirm, Segmented, Select, Table, Tag, Tooltip, message,
} from 'antd'
import {
  SendOutlined, BulbOutlined, PlusOutlined, NodeIndexOutlined, LineChartOutlined,
  DatabaseOutlined, DeleteOutlined, CopyOutlined, MessageOutlined, CloseOutlined, FormOutlined,
  LikeOutlined, DislikeOutlined, LikeFilled, DislikeFilled,
} from '@ant-design/icons'
import dagre from '@dagrejs/dagre'
import { api, tokenStore, type RagDoc, type RagVisibilityGroup } from '../api'
import { useChatHistory, newConversationId, type StoredMsg } from '../hooks/useChatHistory'
import { useApp } from '../context'
import { MarkdownRenderer } from '../components/MarkdownRenderer'
import LottieLogo from '../components/LottieLogo'
import { useI18n } from '../i18n'

// ---- decision-chain types (mirror the backend AskTrace/TraceStep) ----
type TraceHit = { topic: string; score: number }
type TraceStep =
  | { kind: 'embedding'; dim: number; latency_ms: number }
  | { kind: 'semantic_search'; fetched: number; top: TraceHit[]; latency_ms: number }
  | { kind: 'context_built'; chunks: { index: number; topic: string }[]; approx_tokens: number }
  | { kind: 'llm_generation'; latency_ms: number; answer_chars: number }
  | { kind: string; [k: string]: unknown }
type AskTrace = { question: string; started_at: string; steps: TraceStep[]; total_ms: number }
type Citation = { title?: string; heading?: string; doc_id?: string; content_preview?: string; relevance_score?: number }
type Msg = {
  role: 'user' | 'assistant'
  content: string
  displayed?: number
  citations?: Citation[]
  trace?: AskTrace
  traceOpen?: boolean
  streaming?: boolean
  feedback?: 'up' | 'down'
}

const STEP_META: Record<string, { label: string; glyph: string; tone: string }> = {
  embedding: { label: '生成向量', glyph: '∿', tone: '#8a7fd1' },
  keyword_search: { label: '关键词检索', glyph: '⌕', tone: '#4ea394' },
  semantic_search: { label: '语义检索', glyph: '◌', tone: '#4ea394' },
  fusion: { label: '融合 (RRF)', glyph: '⇶', tone: '#4ea394' },
  rerank: { label: '重排', glyph: '✶', tone: '#e29a6b' },
  context_built: { label: '上下文组装', glyph: '☷', tone: '#64748b' },
  llm_generation: { label: '模型生成', glyph: '✦', tone: '#8a7fd1' },
}
const metaOf = (k: string) => STEP_META[k] || { label: k, glyph: '•', tone: '#94a3b8' }
const summarise = (s: TraceStep): string => {
  const a = s as Record<string, unknown>
  const ms = a.latency_ms != null ? ` · ${a.latency_ms}ms` : ''
  switch (s.kind) {
    case 'embedding': return `${a.dim} 维${ms}`
    case 'keyword_search': return `${a.fetched} 条命中${ms}`
    case 'semantic_search': return `${a.fetched} 条命中${ms}`
    case 'fusion': return `${a.candidates} → ${a.selected} 候选`
    case 'rerank': return `${a.applied ? '已重排' : '跳过'} · ${a.candidates} 候选${ms}`
    case 'context_built': return `${(a.chunks as unknown[]).length} 段 · ~${a.approx_tokens} tokens`
    case 'llm_generation': return `${a.answer_chars} 字${ms}`
    default: return ''
  }
}

/** Shared step-detail panel (top hits + raw payload). */
function StepDetail({ s, tone }: { s: TraceStep; tone: string }) {
  return (
    <div className="dc-fade" style={{ marginTop: 8, border: '1px solid var(--border-soft)', borderRadius: 8, background: 'var(--panel-2)', padding: '8px 10px' }}>
      {'top' in s && Array.isArray((s as { top: TraceHit[] }).top) && (s as { top: TraceHit[] }).top.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 3, marginBottom: 6 }}>
          {(s as { top: TraceHit[] }).top.map((h, j) => (
            <div key={j} style={{ display: 'flex', gap: 8, fontSize: 12 }}>
              <span style={{ color: 'var(--text-3)' }}>{j + 1}</span>
              <span style={{ flex: 1, color: 'var(--text-2)' }}>{h.topic || '(无标题)'}</span>
              <span className="ms-mono" style={{ color: tone }}>{h.score.toFixed(3)}</span>
            </div>
          ))}
        </div>
      )}
      <pre className="ms-mono" style={{ margin: 0, fontSize: 11, color: 'var(--text-3)', whiteSpace: 'pre-wrap', maxHeight: 160, overflow: 'auto' }}>{JSON.stringify(s, null, 2)}</pre>
    </div>
  )
}

/** Decision chain: a linear pipeline shown as a timeline (vertical) or node graph (horizontal). */
const dcStepId = (i: number) => `step-${i}`

// dagre layout: linear step-i-1 → step-i chain, rankdir LR, orthogonal L-shaped edges between node
// centres (across to the midpoint, down to the target row, across) so it reads as a flow.
interface DagreNode { id: string; i: number; x: number; y: number; w: number; h: number; label: string; glyph: string; tone: string; meta: string }
function useDagreLayout(trace: AskTrace) {
  return useMemo(() => {
    if (!trace.steps.length) return null
    const g = new dagre.graphlib.Graph({ directed: true })
    g.setGraph({ rankdir: 'LR', nodesep: 26, ranksep: 54, marginx: 18, marginy: 18 })
    g.setDefaultEdgeLabel(() => ({}))
    const NODE_W = 208, NODE_H = 66
    trace.steps.forEach((_, i) => {
      g.setNode(dcStepId(i), { width: NODE_W, height: NODE_H })
      if (i > 0) g.setEdge(dcStepId(i - 1), dcStepId(i))
    })
    dagre.layout(g)
    const nodes: DagreNode[] = g.nodes().map((id) => {
      const n = g.node(id) as { x: number; y: number; width: number; height: number }
      const i = parseInt(id.replace('step-', ''), 10)
      const s = trace.steps[i]
      const m = metaOf(s.kind)
      return { id, i, x: n.x, y: n.y, w: n.width, h: n.height, label: m.label, glyph: m.glyph, tone: m.tone, meta: summarise(s) }
    })
    const edges = g.edges().map((e) => {
      const src = g.node(e.v) as { x: number; y: number; width: number }
      const dst = g.node(e.w) as { x: number; y: number; width: number }
      const x1 = src.x + src.width / 2, x2 = dst.x - dst.width / 2 - 4, mid = (x1 + x2) / 2
      return { d: `M ${x1} ${src.y} H ${mid} V ${dst.y} H ${x2}` }
    })
    const gr = g.graph() as { width: number; height: number }
    return { width: gr.width, height: gr.height, nodes, edges }
  }, [trace])
}

function DecisionChain({ trace }: { trace: AskTrace }) {
  const [sel, setSel] = useState<number | null>(null)
  const [view, setView] = useState<'timeline' | 'graph'>('graph')
  const selStep = sel != null ? trace.steps[sel] : null
  const layout = useDagreLayout(trace)
  return (
    <div style={{ marginTop: 8, border: '1px solid var(--border-soft)', borderRadius: 10, background: 'var(--panel)', padding: '10px 14px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8, color: 'var(--text-2)', fontSize: 12 }}>
        <NodeIndexOutlined /> <b style={{ color: 'var(--text)' }}>决策链</b>
        <span>{trace.steps.length} 步 · {trace.total_ms} ms</span>
        <div style={{ flex: 1 }} />
        <Segmented size="small" value={view} onChange={(v) => setView(v as 'timeline' | 'graph')} options={[{ label: '节点图', value: 'graph' }, { label: '时间线', value: 'timeline' }]} />
      </div>

      {view === 'graph' ? (
        <div className="dc-tree-wrap">
          {layout && (
            <div className="dc-canvas" style={{ position: 'relative', width: layout.width, height: layout.height }}>
              <svg width={layout.width} height={layout.height} style={{ position: 'absolute', inset: 0, pointerEvents: 'none', overflow: 'visible' }} aria-hidden>
                <defs>
                  <marker id="dc-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--text-3)" />
                  </marker>
                </defs>
                {layout.edges.map((e, i) => (
                  <path key={i} d={e.d} className="dc-edge" markerEnd="url(#dc-arrow)" />
                ))}
              </svg>
              {layout.nodes.map((n) => {
                const on = sel === n.i
                return (
                  <button
                    key={n.id}
                    onClick={() => setSel(on ? null : n.i)}
                    className={`dc-card${on ? ' selected' : ''}`}
                    style={{ position: 'absolute', left: n.x - n.w / 2, top: n.y - n.h / 2, width: n.w, height: n.h, ['--tone' as string]: n.tone }}
                  >
                    <span className="dc-card-tone" style={{ background: n.tone }} />
                    <span className="dc-card-step">{n.i + 1}</span>
                    <span className="dc-card-glyph" style={{ color: n.tone }}>{n.glyph}</span>
                    <span className="dc-card-text">
                      <span className="dc-card-label">{n.label}</span>
                      <span className="dc-card-meta ms-mono">{n.meta}</span>
                    </span>
                  </button>
                )
              })}
            </div>
          )}
        </div>
      ) : (
        <div>
          {trace.steps.map((s, i) => {
            const m = metaOf(s.kind)
            const on = sel === i
            return (
              <div key={i} className="dc-step" style={{ display: 'flex', gap: 10, alignItems: 'flex-start' }}>
                <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', alignSelf: 'stretch' }}>
                  <span style={{ width: 22, height: 22, borderRadius: '50%', background: m.tone, color: '#fff', display: 'grid', placeItems: 'center', fontSize: 12, flexShrink: 0 }}>{m.glyph}</span>
                  {i < trace.steps.length - 1 && <span style={{ flex: 1, width: 2, background: 'var(--border-soft)', minHeight: 10 }} />}
                </div>
                <button onClick={() => setSel(on ? null : i)} style={{ flex: 1, textAlign: 'left', background: on ? 'var(--panel-2)' : 'transparent', border: '1px solid ' + (on ? m.tone : 'transparent'), borderRadius: 8, padding: '6px 10px', cursor: 'pointer', marginBottom: 6 }}>
                  <div style={{ display: 'flex', gap: 8, alignItems: 'baseline' }}>
                    <span style={{ fontWeight: 600, fontSize: 13, color: 'var(--text)' }}>{m.label}</span>
                    <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{summarise(s)}</span>
                  </div>
                </button>
              </div>
            )
          })}
        </div>
      )}

      {selStep && <StepDetail s={selStep} tone={metaOf(selStep.kind).tone} />}
    </div>
  )
}

type Eval = { relevance: number; faithfulness: number; completeness: number; overall: number; comment: string }

/** Animated SVG gauge ring (0-100). */
function Ring({ label, value, color, size = 84 }: { label: string; value: number; color: string; size?: number }) {
  const [v, setV] = useState(0)
  useEffect(() => { const id = setTimeout(() => setV(value), 60); return () => clearTimeout(id) }, [value])
  const r = size / 2 - 8
  const C = 2 * Math.PI * r
  const fs = size > 100 ? 30 : 20
  return (
    <div style={{ textAlign: 'center' }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--border-soft)" strokeWidth={8} />
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={color} strokeWidth={8} strokeLinecap="round"
          strokeDasharray={C} strokeDashoffset={C * (1 - v / 100)} transform={`rotate(-90 ${size / 2} ${size / 2})`}
          style={{ transition: 'stroke-dashoffset 0.9s cubic-bezier(.2,.8,.2,1)' }} />
        <text x={size / 2} y={size / 2 + fs / 3} textAnchor="middle" fontSize={fs} fontWeight={700} fill="var(--text)">{v}</text>
      </svg>
      <div style={{ fontSize: 12, color: 'var(--text-2)' }}>{label}</div>
    </div>
  )
}

/** Parse the backend SSE stream (event/data frames) and dispatch each event. */
async function askStream(
  projectId: string,
  question: string,
  history: [string, string][],
  onEvent: (ev: string, data: Record<string, unknown>) => void,
  trace = true,
): Promise<void> {
  const res = await fetch('/rag/ask/stream', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tokenStore.get()}` },
    body: JSON.stringify({ projectId, question, trace, topK: 8, rerank: true, history }),
  })
  if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`)
  const reader = res.body.getReader()
  const dec = new TextDecoder()
  let buf = ''
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    buf += dec.decode(value, { stream: true })
    let idx
    while ((idx = buf.indexOf('\n\n')) >= 0) {
      const frame = buf.slice(0, idx)
      buf = buf.slice(idx + 2)
      let ev = 'message'
      let data = ''
      for (const line of frame.split('\n')) {
        if (line.startsWith('event:')) ev = line.slice(6).trim()
        else if (line.startsWith('data:')) data += line.slice(5).trim()
      }
      if (data) { try { onEvent(ev, JSON.parse(data)) } catch { /* skip */ } }
    }
  }
}

// Compress citation numbers for display: [1,2,3,5] → "1–3, 5" (runs ≥3 collapse to a range).
function compressNums(nums: number[]): string {
  const u = [...new Set(nums)].sort((a, b) => a - b)
  const parts: string[] = []
  let i = 0
  while (i < u.length) {
    let j = i
    while (j + 1 < u.length && u[j + 1] === u[j] + 1) j++
    parts.push(j > i + 1 ? `${u[i]}–${u[j]}` : u.slice(i, j + 1).join(', '))
    i = j + 1
  }
  return parts.join(', ')
}

// Turn citation refs in a text run ([1], [1,2], [1][2][3]) into clickable <sup> badges carrying the
// referenced source numbers. Returns null if the run has no valid refs (≤ max).
function citeFragment(text: string, max: number): DocumentFragment | null {
  const re = /\[\d+(?:\s*,\s*\d+)*\](?:\s*\[\d+(?:\s*,\s*\d+)*\])*/g
  const frag = document.createDocumentFragment()
  let m: RegExpExecArray | null
  let last = 0
  let found = false
  while ((m = re.exec(text))) {
    const nums = (m[0].match(/\d+/g) || []).map(Number).filter((n) => n >= 1 && n <= max)
    if (!nums.length) continue
    found = true
    if (m.index > last) frag.appendChild(document.createTextNode(text.slice(last, m.index)))
    const sup = document.createElement('sup')
    sup.className = 'md-cite'
    sup.dataset.cites = nums.join(',')
    sup.tabIndex = 0
    sup.textContent = `[${compressNums(nums)}]`
    frag.appendChild(sup)
    last = m.index + m[0].length
  }
  if (!found) return null
  if (last < text.length) frag.appendChild(document.createTextNode(text.slice(last)))
  return frag
}

// Assistant answer with inline, clickable citation badges. Walks the rendered markdown's text nodes
// (skipping code/pre/links/existing badges) and replaces [n] refs with <sup> badges; clicking one
// opens the source panel and flashes the matching bottom chips. Mirrors feishu's inline-ref UX.
function CitedAnswer({ text, citations, onOpen }: {
  text: string
  citations?: Citation[]
  onOpen: (idx: number) => void
}) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const root = ref.current?.querySelector('.markdown-renderer')
    const max = citations?.length || 0
    if (!root || !max) return
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode: (n) => {
        let p = (n as Text).parentElement
        while (p && p !== root) {
          if (p.tagName === 'CODE' || p.tagName === 'PRE' || p.tagName === 'A' || p.tagName === 'SUP') {
            return NodeFilter.FILTER_REJECT
          }
          p = p.parentElement
        }
        return /\[\d/.test(n.nodeValue || '') ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT
      },
    })
    const targets: Text[] = []
    let node: Node | null
    while ((node = walker.nextNode())) targets.push(node as Text)
    for (const tn of targets) {
      const f = citeFragment(tn.nodeValue || '', max)
      if (f) tn.parentNode?.replaceChild(f, tn)
    }
  })
  const onClick = (e: React.MouseEvent) => {
    const el = (e.target as HTMLElement).closest?.('sup.md-cite') as HTMLElement | null
    if (!el) return
    const nums = (el.dataset.cites || '').split(',').map(Number).filter(Boolean)
    if (!nums.length) return
    onOpen(nums[0] - 1)
    const msg = ref.current?.closest('.dc-msg')
    nums.forEach((n) => {
      const chip = msg?.querySelector(`[data-chip="${n}"]`)
      if (chip) { chip.classList.add('cite-flash'); window.setTimeout(() => chip.classList.remove('cite-flash'), 700) }
    })
  }
  return <div ref={ref} onClick={onClick}><MarkdownRenderer value={text} /></div>
}

export default function KnowledgeQA() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [input, setInput] = useState('')
  const [messages, setMessages] = useState<Msg[]>([])
  const [loading, setLoading] = useState(false)
  const [ingestOpen, setIngestOpen] = useState(false)
  const [ingTitle, setIngTitle] = useState('')
  const [ingText, setIngText] = useState('')
  const [ingGroups, setIngGroups] = useState<string[]>([])
  const [groups, setGroups] = useState<RagVisibilityGroup[]>([])
  const [manageOpen, setManageOpen] = useState(false)
  const [evalData, setEvalData] = useState<Eval | null>(null)
  const [evalLoading, setEvalLoading] = useState(false)
  const [citePanel, setCitePanel] = useState<Citation | null>(null)
  const [stats, setStats] = useState<{ documents: number; chunks: number } | null>(null)
  // One-time fullscreen Lottie intro (per session), shrinking away to reveal the landing.
  const [intro, setIntro] = useState<boolean>(() => !sessionStorage.getItem('rag_intro_seen'))
  const [introLeaving, setIntroLeaving] = useState(false)
  // 决策追溯 toggle (persisted) — whether the stream captures the decision chain.
  const [traceOn, setTraceOn] = useState<boolean>(() => localStorage.getItem('rag_trace_on') !== '0')
  const [phIdx, setPhIdx] = useState(0)
  const hist = useChatHistory(projectId || '')
  const [sessionId, setSessionId] = useState<string>(() => newConversationId())
  const scroller = useRef<HTMLDivElement>(null)
  const typer = useRef<number | undefined>(undefined)

  useEffect(() => { scroller.current?.scrollTo({ top: scroller.current.scrollHeight, behavior: 'smooth' }) }, [messages])
  useEffect(() => () => { if (typer.current) clearInterval(typer.current) }, [])
  useEffect(() => { localStorage.setItem('rag_trace_on', traceOn ? '1' : '0') }, [traceOn])
  // Rotating placeholder suggestions (Tab accepts the current one).
  const placeholders = useMemo(
    () => ['这个项目怎么部署?', '支持哪些登录方式?', '接口鉴权怎么做?', '数据库用的是什么?', '如何本地启动?', '有哪些对外接口?'],
    [],
  )
  useEffect(() => {
    const id = window.setInterval(() => setPhIdx((i) => (i + 1) % placeholders.length), 4000)
    return () => clearInterval(id)
  }, [placeholders.length])

  const newConversation = () => { setSessionId(newConversationId()); setMessages([]); setInput(''); setEvalData(null) }

  // Knowledge-base size for the landing card.
  useEffect(() => {
    if (!projectId) { setStats(null); return }
    api.ragStats(projectId).then(setStats).catch(() => setStats(null))
  }, [projectId])

  // Play the intro once, then shrink/fade it out.
  useEffect(() => {
    if (!intro) return
    const t1 = window.setTimeout(() => setIntroLeaving(true), 1200)
    const t2 = window.setTimeout(() => { setIntro(false); sessionStorage.setItem('rag_intro_seen', '1') }, 1850)
    return () => { clearTimeout(t1); clearTimeout(t2) }
  }, [intro])

  // Persist the conversation after each completed exchange (streaming finished).
  useEffect(() => {
    if (!projectId || loading || !messages.length) return
    const stored: StoredMsg[] = messages
      .filter((m) => m.content.trim())
      .map((m) => ({ role: m.role, content: m.content, citations: m.citations }))
    if (stored.length) hist.upsert(sessionId, stored)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [messages, loading])

  // Thumbs feedback on an answer → recorded server-side (ms_rag_feedback).
  const vote = async (i: number, v: 'up' | 'down') => {
    const m = messages[i]
    if (!projectId || !m?.content) return
    const question = messages[i - 1]?.content || ''
    setMessages((ms) => ms.map((x, k) => (k === i ? { ...x, feedback: v } : x)))
    try {
      await api.submitRagFeedback({ projectId, sessionId, vote: v, question, answer: m.content })
      message.success(t('rag.thanksFeedback', '感谢反馈'))
    } catch (e) {
      message.error(e instanceof Error ? e.message : t('rag.feedbackFail', '反馈失败'))
    }
  }

  // Restore a stored conversation into the chat.
  const loadConversation = (id: string) => {
    const c = hist.get(id)
    if (!c) return
    setSessionId(c.id)
    setEvalData(null)
    setMessages(c.messages.map((m) => ({
      role: m.role, content: m.content, displayed: m.content.length, streaming: false,
      citations: m.citations as Citation[] | undefined,
    })))
  }

  const send = async () => {
    const q = input.trim()
    if (!q || loading) return
    if (!projectId) return message.warning(t('rag.needProject', '请先选择项目'))
    setInput('')
    // Prior turns (before this question) as [role, content] pairs for multi-turn context.
    const history: [string, string][] = messages.filter((m) => m.content.trim()).map((m) => [m.role, m.content])
    setMessages((m) => [...m, { role: 'user', content: q }, { role: 'assistant', content: '', displayed: 0, streaming: true }])
    setLoading(true)
    const patchLast = (fn: (m: Msg) => Msg) => setMessages((ms) => ms.map((m, i) => (i === ms.length - 1 ? fn(m) : m)))
    try {
      await askStream(projectId, q, history, (ev, data) => {
        if (ev === 'sources') patchLast((m) => ({ ...m, citations: (data.sources as Citation[]) || [] }))
        else if (ev === 'chunk') {
          // Real token stream: append each delta and show it immediately (streaming IS the animation).
          const delta = (data.delta as string) || ''
          patchLast((m) => ({ ...m, content: m.content + delta, displayed: m.content.length + delta.length }))
        } else if (ev === 'trace') patchLast((m) => ({ ...m, trace: data as unknown as AskTrace }))
        else if (ev === 'error') {
          // Unify the "provider not configured" case with the eval flow: a "去配置" prompt, not a raw error.
          const notConfigured = data.code === 'not_configured'
          const hint = notConfigured
            ? t('rag.notConfigured', '⚠ 尚未配置 RAG 模型:请到「系统参数 → RAG 配置」填写 Embedding 与生成模型后重试')
            : `⚠ ${data.message}`
          patchLast((m) => ({ ...m, content: m.content + `\n\n${hint}`, streaming: false }))
          if (notConfigured) message.warning(t('rag.notConfiguredToast', '尚未配置 RAG 模型,请到 系统参数 → RAG 配置'))
        }
      }, traceOn)
    } catch (e) {
      patchLast((m) => ({ ...m, content: m.content + `\n\n⚠ ${e instanceof Error ? e.message : '请求失败'}` }))
    } finally {
      patchLast((m) => ({ ...m, streaming: false, displayed: m.content.length }))
      setLoading(false)
    }
  }

  // Visibility groups (audience taxonomy). Loaded lazily when the user opens ingest / management.
  const loadGroups = useCallback(() => {
    api.ragGroups().then(setGroups).catch(() => setGroups([]))
  }, [])
  const groupOptions = useMemo(() => groups.map((g) => ({ value: g.id, label: g.name })), [groups])
  const groupName = useCallback((id: string) => groups.find((g) => g.id === id)?.name || id, [groups])

  const ingest = async () => {
    if (!ingText.trim() || !projectId) return
    try {
      const j = await api.ingestRagDoc({
        projectId,
        title: ingTitle.trim() || '未命名文档',
        text: ingText,
        visibilityGroups: ingGroups,
      })
      if (j.embedded) {
        message.success(t('rag.ingested', `已入库 ${j.chunks} 段`))
      } else {
        // No embedding provider configured — stored keyword-only; semantic recall needs a backfill.
        message.warning(t('rag.ingestedKw', `已按关键词入库 ${j.chunks} 段;未配置 Embedding,语义检索暂不可用,配置后可在「知识库管理」回填`))
      }
      setIngestOpen(false); setIngTitle(''); setIngText(''); setIngGroups([])
    } catch (e) {
      message.error(e instanceof Error ? e.message : t('rag.ingestFail', '入库失败'))
    }
  }

  const runEval = async (i: number) => {
    const answer = messages[i]?.content || ''
    const question = messages[i - 1]?.content || ''
    if (!answer || !projectId) return
    setEvalData(null)
    setEvalLoading(true)
    try {
      const res = await fetch('/rag/evaluate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tokenStore.get()}` },
        body: JSON.stringify({ projectId, question, answer }),
      })
      if (!res.ok) {
        // 503 = RAG not configured; surface the backend's actionable detail rather than a bare code.
        const body = await res.json().catch(() => null)
        const detail = body?.detail || `HTTP ${res.status}`
        if (res.status === 503) {
          message.warning(detail)
          setEvalData(null); return
        }
        throw new Error(detail)
      }
      setEvalData(await res.json())
    } catch (e) {
      message.error(e instanceof Error ? e.message : t('rag.evalFail', '评估失败'))
      setEvalData(null)
    } finally {
      setEvalLoading(false)
    }
  }

  const empty = messages.length === 0
  // Landing suggestions: the user's recent questions first, then generic starters.
  const suggestions = useMemo(() => {
    const recent = hist.recentQuestions(3)
    const base = ['这个项目怎么部署?', '支持哪些登录方式?', '接口鉴权怎么做?']
    return [...recent, ...base.filter((b) => !recent.includes(b))].slice(0, 5)
  }, [hist])

  return (
    <div style={{ display: 'flex', height: '100%', background: 'var(--bg-base)' }}>
      <HistorySidebar
        list={hist.list}
        activeId={sessionId}
        onNew={newConversation}
        onPick={loadConversation}
        onRemove={(id) => { hist.remove(id); if (id === sessionId) newConversation() }}
        onClear={() => { hist.clear(); newConversation() }}
      />
      <div style={{ display: 'flex', flexDirection: 'column', height: '100%', flex: 1, minWidth: 0, background: 'var(--bg-base)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 16px', borderBottom: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
        <BulbOutlined style={{ color: 'var(--brand)' }} />
        <b>{t('rag.title', '聊天')}</b>
        <div style={{ flex: 1 }} />
        <Button icon={<DatabaseOutlined />} onClick={() => { loadGroups(); setManageOpen(true) }}>{t('rag.manage', '知识库管理')}</Button>
        <Button icon={<PlusOutlined />} onClick={() => { loadGroups(); setIngestOpen((v) => !v) }}>{t('rag.addKnowledge', '添加知识')}</Button>
      </div>

      {ingestOpen && (
        <div style={{ padding: 12, borderBottom: '1px solid var(--border-soft)', background: 'var(--panel)', display: 'flex', flexDirection: 'column', gap: 8 }}>
          <Input placeholder={t('rag.docTitle', '文档标题')} value={ingTitle} onChange={(e) => setIngTitle(e.target.value)} />
          <Input.TextArea rows={4} placeholder={t('rag.docText', '粘贴 Markdown 文档内容,会被切块并向量化入库')} value={ingText} onChange={(e) => setIngText(e.target.value)} />
          <Select
            mode="multiple"
            allowClear
            value={ingGroups}
            onChange={setIngGroups}
            options={groupOptions}
            placeholder={t('rag.pickGroups', '可见组(留空 = 仅自己与管理员可见)')}
            notFoundContent={<span style={{ color: 'var(--text-3)' }}>{t('rag.noGroups', '还没有可见组,去「系统设置 → RAG」创建')}</span>}
          />
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
            <Button onClick={() => setIngestOpen(false)}>{t('a.cancel', '取消')}</Button>
            <Button type="primary" onClick={ingest} disabled={!ingText.trim()}>{t('rag.ingest', '入库')}</Button>
          </div>
        </div>
      )}

      <ManageDrawer
        open={manageOpen}
        onClose={() => setManageOpen(false)}
        projectId={projectId}
        groupOptions={groupOptions}
        groupName={groupName}
      />

      {intro && (
        <div className={`rag-intro-overlay${introLeaving ? ' leaving' : ''}`}>
          <LottieLogo size={260} />
        </div>
      )}
      <div ref={scroller} style={{ flex: 1, overflow: 'auto', padding: empty ? 0 : '16px 0' }}>
        {empty ? (
          <div style={{ maxWidth: 680, margin: '0 auto', paddingTop: 64, textAlign: 'center' }}>
            <div className="rag-orb-wrap"><LottieLogo size={120} /></div>
            <div className="rag-in rag-in-1" style={{ fontSize: 26, fontWeight: 700, marginBottom: 6 }}>{t('rag.hero', '问我关于这个项目的任何问题')}</div>
            <div className="rag-in rag-in-2" style={{ color: 'var(--text-3)', marginBottom: 20 }}>
              {stats && stats.chunks > 0
                ? <>{t('rag.heroStatsA', '整合本项目')} <b style={{ color: 'var(--brand)' }}>{stats.chunks.toLocaleString()}</b> {t('rag.heroStatsB', '个知识点,检索 + 大模型生成,答案带来源与决策链')}</>
                : t('rag.heroSub', '基于知识库检索 + 大模型生成,答案带来源引用与决策链')}
            </div>
            <div className="rag-in rag-in-3" style={{ display: 'flex', gap: 8, justifyContent: 'center', flexWrap: 'wrap' }}>
              {suggestions.map((s) => (
                <Tag key={s} style={{ cursor: 'pointer', padding: '4px 12px', borderRadius: 999 }} onClick={() => setInput(s)}>{s}</Tag>
              ))}
            </div>
          </div>
        ) : (
          <div style={{ maxWidth: 760, margin: '0 auto', display: 'flex', flexDirection: 'column', gap: 18 }}>
            {messages.map((m, i) => (
              <div key={i} className="dc-msg" style={{ display: 'flex', gap: 10, flexDirection: m.role === 'user' ? 'row-reverse' : 'row', padding: '0 16px' }}>
                <span style={{ width: 30, height: 30, borderRadius: '50%', display: 'grid', placeItems: 'center', background: m.role === 'user' ? 'var(--brand)' : 'var(--panel-2)', color: m.role === 'user' ? '#fff' : 'var(--brand)', flexShrink: 0, fontSize: 13 }}>
                  {m.role === 'user' ? '我' : <BulbOutlined />}
                </span>
                <div style={{ maxWidth: 640, minWidth: 0 }}>
                  <div style={{ background: 'var(--panel)', border: '1px solid var(--border-soft)', borderRadius: 12, padding: '10px 14px' }}>
                    {m.role === 'assistant'
                      ? <CitedAnswer
                          text={(m.displayed != null ? m.content.slice(0, m.displayed) : m.content) + (m.streaming && (m.displayed ?? 0) >= m.content.length ? ' ▋' : '')}
                          citations={m.citations}
                          onOpen={(idx) => m.citations?.[idx] && setCitePanel(m.citations[idx])}
                        />
                      : <span>{m.content}</span>}
                    {m.role === 'assistant' && m.streaming && !m.content && <span style={{ color: 'var(--text-3)' }}>思考中<span className="dc-dots" /></span>}
                  </div>
                  {!!m.citations?.length && (
                    <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginTop: 6 }}>
                      {m.citations.map((c, j) => (
                        <Tooltip key={j} title={t('rag.viewSource', '查看原文片段')}>
                          <span data-chip={j + 1} onClick={() => setCitePanel(c)}
                            style={{ fontSize: 12, padding: '3px 8px', borderRadius: 6, background: 'var(--panel-2)', border: '1px solid var(--border-soft)', color: 'var(--text-2)', cursor: 'pointer' }}>
                            <b style={{ color: 'var(--brand)' }}>[{j + 1}]</b> {c.heading || c.title}
                            {c.relevance_score != null && <span style={{ color: 'var(--text-3)' }}> · {c.relevance_score.toFixed(2)}</span>}
                          </span>
                        </Tooltip>
                      ))}
                    </div>
                  )}
                  {m.role === 'assistant' && !m.streaming && !!m.content && (
                    <div style={{ marginTop: 6, display: 'flex', gap: 4, flexWrap: 'wrap', alignItems: 'center' }}>
                      <Tooltip title={t('rag.copy', '复制回答')}>
                        <Button size="small" type="text" icon={<CopyOutlined />} onClick={() => {
                          navigator.clipboard?.writeText(m.content).then(
                            () => message.success(t('rag.copied', '已复制')),
                            () => message.error(t('rag.copyFail', '复制失败')),
                          )
                        }} />
                      </Tooltip>
                      <Button size="small" type="text" icon={<LineChartOutlined />} onClick={() => runEval(i)}>{t('rag.evaluate', '评估')}</Button>
                      {m.trace && (
                        <Button size="small" type="text" icon={<NodeIndexOutlined />} onClick={() => setMessages((ms) => ms.map((x, k) => (k === i ? { ...x, traceOpen: !x.traceOpen } : x)))}>
                          {m.traceOpen ? t('rag.hideChain', '收起决策链') : t('rag.showChain', '决策链')}
                        </Button>
                      )}
                      {(m.trace || m.citations?.length) && (
                        <span style={{ color: 'var(--text-3)', fontSize: 12, marginLeft: 2 }}>
                          {m.trace ? `${m.trace.total_ms}ms · ` : ''}{m.citations?.length ? `${m.citations.length} ${t('rag.chunks', '来源')}` : ''}
                        </span>
                      )}
                      <span style={{ flex: 1 }} />
                      <Tooltip title={t('rag.good', '有帮助')}>
                        <Button size="small" type="text" onClick={() => vote(i, 'up')}
                          icon={m.feedback === 'up' ? <LikeFilled style={{ color: 'var(--brand)' }} /> : <LikeOutlined />} />
                      </Tooltip>
                      <Tooltip title={t('rag.bad', '待改进')}>
                        <Button size="small" type="text" onClick={() => vote(i, 'down')}
                          icon={m.feedback === 'down' ? <DislikeFilled style={{ color: '#e29a6b' }} /> : <DislikeOutlined />} />
                      </Tooltip>
                    </div>
                  )}
                  {m.role === 'assistant' && m.traceOpen && m.trace && <DecisionChain trace={m.trace} />}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div style={{ padding: '12px 16px', borderTop: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
        <div style={{ maxWidth: 760, margin: '0 auto' }}>
          {/* pill row: trace toggle + new conversation */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <Tooltip title={traceOn ? t('rag.traceOnTip', '记录每一步检索/融合/生成的决策链路') : t('rag.traceOffTip', '开启后回答带决策链')}>
              <Button
                size="small"
                type={traceOn ? 'primary' : 'default'}
                ghost={traceOn}
                icon={<NodeIndexOutlined />}
                onClick={() => setTraceOn((v) => !v)}
              >
                {t('rag.trace', '决策追溯')}
              </Button>
            </Tooltip>
            <div style={{ flex: 1 }} />
            {messages.length > 0 && (
              <Button size="small" type="text" onClick={newConversation}>{t('rag.newChat', '新对话')}</Button>
            )}
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <Input.TextArea
              autoSize={{ minRows: 1, maxRows: 4 }}
              placeholder={`${t('rag.askTry', '试试')}:${placeholders[phIdx]}  (Enter 发送 · Shift+Enter 换行 · Tab 采纳)`}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey && !(e.nativeEvent as { isComposing?: boolean }).isComposing) {
                  e.preventDefault(); send()
                } else if (e.key === 'Tab' && !input) {
                  e.preventDefault(); setInput(placeholders[phIdx])
                }
              }}
            />
            <Button type="primary" shape="circle" icon={<SendOutlined />} loading={loading} onClick={send} style={{ flexShrink: 0 }} />
          </div>
        </div>
      </div>

      <Modal open={evalLoading || !!evalData} onCancel={() => { setEvalData(null); setEvalLoading(false) }} footer={null} width={520} centered title={t('rag.evalTitle', '答案评估')}>
        {evalLoading && !evalData ? (
          <div style={{ padding: 48, textAlign: 'center', color: 'var(--text-3)' }}>{t('rag.evaluating', '评估中')}<span className="dc-dots" /></div>
        ) : evalData ? (
          <div>
            <div style={{ display: 'flex', justifyContent: 'center', marginBottom: 16 }}>
              <Ring label={t('rag.overall', '综合')} value={evalData.overall} color="var(--brand)" size={120} />
            </div>
            <div style={{ display: 'flex', justifyContent: 'space-around', marginBottom: 14 }}>
              <Ring label={t('rag.relevance', '相关性')} value={evalData.relevance} color="#4ea394" />
              <Ring label={t('rag.faithfulness', '忠实度')} value={evalData.faithfulness} color="#8a7fd1" />
              <Ring label={t('rag.completeness', '完整性')} value={evalData.completeness} color="#e29a6b" />
            </div>
            {evalData.comment && <div style={{ background: 'var(--panel-2)', borderRadius: 8, padding: '10px 12px', color: 'var(--text-2)', fontSize: 13 }}>{evalData.comment}</div>}
          </div>
        ) : null}
      </Modal>

      <Drawer
        open={!!citePanel}
        onClose={() => setCitePanel(null)}
        width={420}
        mask={false}
        title={citePanel ? (citePanel.heading || citePanel.title || t('rag.source', '来源')) : ''}
      >
        {citePanel && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px 18px', fontSize: 12, color: 'var(--text-3)' }}>
              {citePanel.title && <span>{t('rag.docTitle', '文档标题')}: <span style={{ color: 'var(--text-2)' }}>{citePanel.title}</span></span>}
              {citePanel.heading && <span>{t('rag.position', '位置')}: <span style={{ color: 'var(--text-2)' }}>{citePanel.heading}</span></span>}
              {citePanel.relevance_score != null && <span>{t('rag.relevance', '相关性')}: <span style={{ color: 'var(--brand)' }}>{citePanel.relevance_score.toFixed(3)}</span></span>}
            </div>
            <div style={{ fontSize: 12, color: 'var(--text-3)', textTransform: 'uppercase', letterSpacing: 0.4 }}>{t('rag.excerpt', '原文片段')}</div>
            <div style={{ background: 'var(--panel-2)', border: '1px solid var(--border-soft)', borderRadius: 8, padding: '12px 14px', fontSize: 13.5, lineHeight: 1.75, color: 'var(--text)', whiteSpace: 'pre-wrap' }}>
              {citePanel.content_preview || <span style={{ color: 'var(--text-3)' }}>{t('rag.noExcerpt', '(无预览内容)')}</span>}
            </div>
          </div>
        )}
      </Drawer>
      </div>
    </div>
  )
}

// Left rail of recent conversations (localStorage-backed), faithful to feishu's "最近对话" sidebar:
// new-chat button, title list with active highlight, per-item delete, clear-all.
function HistorySidebar({ list, activeId, onNew, onPick, onRemove, onClear }: {
  list: { id: string; title: string }[]
  activeId: string
  onNew: () => void
  onPick: (id: string) => void
  onRemove: (id: string) => void
  onClear: () => void
}) {
  const { t } = useI18n()
  return (
    <div style={{ width: 232, flexShrink: 0, borderRight: '1px solid var(--border-soft)', background: 'var(--panel)', display: 'flex', flexDirection: 'column' }}>
      <div style={{ padding: 10 }}>
        <Button block icon={<FormOutlined />} onClick={onNew}>{t('rag.newChat', '新对话')}</Button>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', padding: '2px 14px 6px', fontSize: 12, color: 'var(--text-3)' }}>
        <span style={{ flex: 1 }}>{t('rag.recentChats', '最近对话')}</span>
        {list.length > 0 && (
          <Tooltip title={t('rag.clearHistory', '清空历史')}>
            <DeleteOutlined style={{ cursor: 'pointer' }} onClick={onClear} />
          </Tooltip>
        )}
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: '0 6px 10px' }}>
        {list.length === 0 ? (
          <div style={{ padding: '8px 10px', color: 'var(--text-3)', fontSize: 13 }}>{t('rag.noHistory', '暂无')}</div>
        ) : list.slice(0, 50).map((c) => (
          <div
            key={c.id}
            onClick={() => onPick(c.id)}
            className="rag-conv-item"
            style={{
              display: 'flex', alignItems: 'center', gap: 8, padding: '7px 10px', margin: '2px 0',
              borderRadius: 6, cursor: 'pointer', fontSize: 13,
              background: c.id === activeId ? 'var(--brand-soft)' : 'transparent',
              color: c.id === activeId ? 'var(--brand)' : 'var(--text)',
            }}
          >
            <MessageOutlined style={{ fontSize: 13, flexShrink: 0, opacity: 0.7 }} />
            <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{c.title || t('rag.untitledChat', '(新对话)')}</span>
            <CloseOutlined
              className="rag-conv-del"
              style={{ fontSize: 11, color: 'var(--text-3)', flexShrink: 0 }}
              onClick={(e) => { e.stopPropagation(); onRemove(c.id) }}
            />
          </div>
        ))}
      </div>
    </div>
  )
}

// Knowledge-base management: list this project's documents and re-assign each one's visibility groups
// (or delete it). Only documents the caller may see are returned by the backend.
function ManageDrawer({
  open, onClose, projectId, groupOptions, groupName,
}: {
  open: boolean
  onClose: () => void
  projectId: string | undefined
  groupOptions: { value: string; label: string }[]
  groupName: (id: string) => string
}) {
  const { t } = useI18n()
  const [docs, setDocs] = useState<RagDoc[]>([])
  const [loading, setLoading] = useState(false)
  const [savingId, setSavingId] = useState<string | null>(null)
  const [reindexing, setReindexing] = useState(false)

  const load = useCallback(() => {
    if (!projectId) return
    setLoading(true)
    api.ragDocs(projectId)
      .then(setDocs)
      .catch((e) => message.error(e instanceof Error ? e.message : t('rag.loadDocsFail', '加载文档失败')))
      .finally(() => setLoading(false))
  }, [projectId, t])
  useEffect(() => { if (open) load() }, [open, load])

  const setAudience = async (doc: RagDoc, next: string[]) => {
    setSavingId(doc.id)
    try {
      await api.setRagDocAudience(doc.id, next)
      setDocs((ds) => ds.map((d) => (d.id === doc.id ? { ...d, visibilityGroups: next } : d)))
      message.success(t('rag.audienceSaved', '可见组已更新'))
    } catch (e) {
      message.error(e instanceof Error ? e.message : t('rag.audienceFail', '更新失败'))
    } finally {
      setSavingId(null)
    }
  }

  const remove = async (doc: RagDoc) => {
    try {
      await api.deleteRagDoc(doc.id)
      setDocs((ds) => ds.filter((d) => d.id !== doc.id))
      message.success(t('common.deleted', '已删除'))
    } catch (e) {
      message.error(e instanceof Error ? e.message : t('common.delFail', '删除失败'))
    }
  }

  const reindex = async () => {
    if (!projectId) return
    setReindexing(true)
    try {
      const r = await api.reindexRag(projectId)
      message.success(t('rag.reindexed', `已回填 ${r.reindexed} 个知识块的语义向量`))
    } catch (e) {
      message.error(e instanceof Error ? e.message : t('rag.reindexFail', '回填失败(检查 Embedding 配置)'))
    } finally {
      setReindexing(false)
    }
  }

  return (
    <Drawer
      open={open}
      onClose={onClose}
      width={720}
      title={t('rag.manageTitle', '知识库文档管理')}
      extra={
        <Tooltip title={t('rag.reindexTip', '把关键词-only 入库的知识块用当前 Embedding 配置补齐语义向量')}>
          <Button size="small" loading={reindexing} disabled={!projectId} onClick={reindex}>
            {t('rag.reindex', '回填语义向量')}
          </Button>
        </Tooltip>
      }
    >
      {!projectId ? (
        <Empty description={t('rag.needProject', '请先选择项目')} />
      ) : (
        <Table<RagDoc>
          rowKey="id"
          size="small"
          loading={loading}
          dataSource={docs}
          pagination={false}
          locale={{ emptyText: <Empty description={t('rag.docsEmpty', '暂无文档')} /> }}
          columns={[
            { title: t('rag.docTitle', '文档标题'), dataIndex: 'title', ellipsis: true },
            {
              title: t('rag.visibleTo', '可见组'),
              dataIndex: 'visibilityGroups',
              width: 300,
              render: (ids: string[], doc: RagDoc) => (
                <Select
                  mode="multiple"
                  size="small"
                  style={{ width: '100%' }}
                  value={ids}
                  loading={savingId === doc.id}
                  options={groupOptions}
                  onChange={(next) => setAudience(doc, next)}
                  placeholder={t('rag.restrictedPh', '未设 = 仅上传者/管理员')}
                  maxTagCount="responsive"
                  tagRender={(p) => <Tag color="geekblue" closable={p.closable} onClose={p.onClose}>{groupName(String(p.value))}</Tag>}
                />
              ),
            },
            {
              title: t('common.actions', '操作'),
              width: 72,
              render: (_: unknown, doc: RagDoc) => (
                <Popconfirm
                  title={t('rag.docDelConfirm', '删除该文档及其向量?')}
                  onConfirm={() => remove(doc)}
                  okText={t('common.ok', '确定')}
                  cancelText={t('common.cancel', '取消')}
                >
                  <Button size="small" type="link" danger icon={<DeleteOutlined />} />
                </Popconfirm>
              ),
            },
          ]}
        />
      )}
    </Drawer>
  )
}
