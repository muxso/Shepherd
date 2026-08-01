import { useEffect, useMemo, useRef, useState } from 'react'
import { Button, Input, Tag, Tooltip, message } from 'antd'
import { SendOutlined, BulbOutlined, PlusOutlined, NodeIndexOutlined } from '@ant-design/icons'
import { tokenStore } from '../api'
import { useApp } from '../context'
import { MarkdownRenderer } from '../components/MarkdownRenderer'
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
}

const STEP_META: Record<string, { label: string; glyph: string; tone: string }> = {
  embedding: { label: '生成向量', glyph: '∿', tone: '#8a7fd1' },
  semantic_search: { label: '语义检索', glyph: '◌', tone: '#4ea394' },
  context_built: { label: '上下文组装', glyph: '☷', tone: '#64748b' },
  llm_generation: { label: '模型生成', glyph: '✦', tone: '#8a7fd1' },
}
const metaOf = (k: string) => STEP_META[k] || { label: k, glyph: '•', tone: '#94a3b8' }
const summarise = (s: TraceStep): string => {
  const anyS = s as Record<string, unknown>
  const ms = anyS.latency_ms != null ? ` · ${anyS.latency_ms}ms` : ''
  switch (s.kind) {
    case 'embedding': return `${(s as { dim: number }).dim} 维${ms}`
    case 'semantic_search': return `${(s as { fetched: number }).fetched} 条命中${ms}`
    case 'context_built': return `${(s as { chunks: unknown[] }).chunks.length} 段 · ~${(s as { approx_tokens: number }).approx_tokens} tokens`
    case 'llm_generation': return `${(s as { answer_chars: number }).answer_chars} 字${ms}`
    default: return ''
  }
}

/** Decision chain: a vertical rail of pipeline steps, each expandable to its raw payload / hits. */
function DecisionChain({ trace }: { trace: AskTrace }) {
  const [sel, setSel] = useState<number | null>(null)
  return (
    <div style={{ marginTop: 8, border: '1px solid var(--border-soft)', borderRadius: 10, background: 'var(--panel)', padding: '10px 14px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6, color: 'var(--text-2)', fontSize: 12 }}>
        <NodeIndexOutlined /> <b style={{ color: 'var(--text)' }}>决策链</b>
        <span>{trace.steps.length} 步 · {trace.total_ms} ms</span>
      </div>
      {trace.steps.map((s, i) => {
        const m = metaOf(s.kind)
        const open = sel === i
        return (
          <div key={i} className="dc-step" style={{ display: 'flex', gap: 10, alignItems: 'flex-start' }}>
            <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', alignSelf: 'stretch' }}>
              <span style={{ width: 22, height: 22, borderRadius: '50%', background: m.tone, color: '#fff', display: 'grid', placeItems: 'center', fontSize: 12, flexShrink: 0 }}>{m.glyph}</span>
              {i < trace.steps.length - 1 && <span style={{ flex: 1, width: 2, background: 'var(--border-soft)', minHeight: 10 }} />}
            </div>
            <button
              onClick={() => setSel(open ? null : i)}
              style={{ flex: 1, textAlign: 'left', background: open ? 'var(--panel-2)' : 'transparent', border: '1px solid ' + (open ? m.tone : 'transparent'), borderRadius: 8, padding: '6px 10px', cursor: 'pointer', marginBottom: 6 }}
            >
              <div style={{ display: 'flex', gap: 8, alignItems: 'baseline' }}>
                <span style={{ fontWeight: 600, fontSize: 13, color: 'var(--text)' }}>{m.label}</span>
                <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{summarise(s)}</span>
              </div>
              {open && (
                <div style={{ marginTop: 6 }}>
                  {'top' in s && Array.isArray((s as { top: TraceHit[] }).top) && (
                    <div style={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
                      {(s as { top: TraceHit[] }).top.map((h, j) => (
                        <div key={j} style={{ display: 'flex', gap: 8, fontSize: 12 }}>
                          <span style={{ color: 'var(--text-3)' }}>{j + 1}</span>
                          <span style={{ flex: 1, color: 'var(--text-2)' }}>{h.topic || '(无标题)'}</span>
                          <span className="ms-mono" style={{ color: m.tone }}>{h.score.toFixed(3)}</span>
                        </div>
                      ))}
                    </div>
                  )}
                  <pre className="ms-mono" style={{ margin: '6px 0 0', fontSize: 11, color: 'var(--text-3)', whiteSpace: 'pre-wrap', maxHeight: 160, overflow: 'auto' }}>
                    {JSON.stringify(s, null, 2)}
                  </pre>
                </div>
              )}
            </button>
          </div>
        )
      })}
    </div>
  )
}

/** Parse the backend SSE stream (event/data frames) and dispatch each event. */
async function askStream(
  projectId: string,
  question: string,
  onEvent: (ev: string, data: Record<string, unknown>) => void,
): Promise<void> {
  const res = await fetch('/rag/ask/stream', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tokenStore.get()}` },
    body: JSON.stringify({ projectId, question, trace: true, topK: 8 }),
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

export default function KnowledgeQA() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [input, setInput] = useState('')
  const [messages, setMessages] = useState<Msg[]>([])
  const [loading, setLoading] = useState(false)
  const [ingestOpen, setIngestOpen] = useState(false)
  const [ingTitle, setIngTitle] = useState('')
  const [ingText, setIngText] = useState('')
  const scroller = useRef<HTMLDivElement>(null)
  const typer = useRef<number | undefined>(undefined)

  useEffect(() => { scroller.current?.scrollTo({ top: scroller.current.scrollHeight, behavior: 'smooth' }) }, [messages])
  useEffect(() => () => { if (typer.current) clearInterval(typer.current) }, [])

  const send = async () => {
    const q = input.trim()
    if (!q || loading) return
    if (!projectId) return message.warning(t('rag.needProject', '请先选择项目'))
    setInput('')
    setMessages((m) => [...m, { role: 'user', content: q }, { role: 'assistant', content: '', displayed: 0, streaming: true }])
    setLoading(true)
    const patchLast = (fn: (m: Msg) => Msg) => setMessages((ms) => ms.map((m, i) => (i === ms.length - 1 ? fn(m) : m)))
    try {
      await askStream(projectId, q, (ev, data) => {
        if (ev === 'sources') patchLast((m) => ({ ...m, citations: (data.sources as Citation[]) || [] }))
        else if (ev === 'chunk') {
          const delta = (data.delta as string) || ''
          patchLast((m) => ({ ...m, content: m.content + delta }))
          // typewriter reveal
          if (typer.current) clearInterval(typer.current)
          typer.current = window.setInterval(() => {
            setMessages((ms) => {
              const last = ms[ms.length - 1]
              if (!last || (last.displayed ?? 0) >= last.content.length) { if (typer.current) clearInterval(typer.current); return ms }
              return ms.map((m, i) => (i === ms.length - 1 ? { ...m, displayed: Math.min(m.content.length, (m.displayed ?? 0) + 3) } : m))
            })
          }, 16)
        } else if (ev === 'trace') patchLast((m) => ({ ...m, trace: data as unknown as AskTrace }))
        else if (ev === 'error') patchLast((m) => ({ ...m, content: m.content + `\n\n⚠ ${data.message}`, streaming: false }))
      })
    } catch (e) {
      patchLast((m) => ({ ...m, content: m.content + `\n\n⚠ ${e instanceof Error ? e.message : '请求失败'}` }))
    } finally {
      patchLast((m) => ({ ...m, streaming: false, displayed: m.content.length }))
      setLoading(false)
    }
  }

  const ingest = async () => {
    if (!ingText.trim() || !projectId) return
    try {
      const res = await fetch('/rag/document', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tokenStore.get()}` },
        body: JSON.stringify({ projectId, title: ingTitle.trim() || '未命名文档', text: ingText }),
      })
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const j = await res.json()
      message.success(t('rag.ingested', `已入库 ${j.chunks} 段`))
      setIngestOpen(false); setIngTitle(''); setIngText('')
    } catch (e) {
      message.error(e instanceof Error ? e.message : t('rag.ingestFail', '入库失败'))
    }
  }

  const empty = messages.length === 0
  const suggestions = useMemo(() => ['这个项目怎么部署?', '支持哪些登录方式?', '接口鉴权怎么做?'], [])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--bg-base)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 16px', borderBottom: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
        <BulbOutlined style={{ color: 'var(--brand)' }} />
        <b>{t('rag.title', '知识问答')}</b>
        <div style={{ flex: 1 }} />
        <Button icon={<PlusOutlined />} onClick={() => setIngestOpen((v) => !v)}>{t('rag.addKnowledge', '添加知识')}</Button>
      </div>

      {ingestOpen && (
        <div style={{ padding: 12, borderBottom: '1px solid var(--border-soft)', background: 'var(--panel)', display: 'flex', flexDirection: 'column', gap: 8 }}>
          <Input placeholder={t('rag.docTitle', '文档标题')} value={ingTitle} onChange={(e) => setIngTitle(e.target.value)} />
          <Input.TextArea rows={4} placeholder={t('rag.docText', '粘贴 Markdown 文档内容,会被切块并向量化入库')} value={ingText} onChange={(e) => setIngText(e.target.value)} />
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
            <Button onClick={() => setIngestOpen(false)}>{t('a.cancel', '取消')}</Button>
            <Button type="primary" onClick={ingest} disabled={!ingText.trim()}>{t('rag.ingest', '入库')}</Button>
          </div>
        </div>
      )}

      <div ref={scroller} style={{ flex: 1, overflow: 'auto', padding: empty ? 0 : '16px 0' }}>
        {empty ? (
          <div style={{ maxWidth: 680, margin: '0 auto', paddingTop: 80, textAlign: 'center' }}>
            <div style={{ fontSize: 26, fontWeight: 700, marginBottom: 6 }}>{t('rag.hero', '问我关于这个项目的任何问题')}</div>
            <div style={{ color: 'var(--text-3)', marginBottom: 20 }}>{t('rag.heroSub', '基于知识库检索 + 大模型生成,答案带来源引用与决策链')}</div>
            <div style={{ display: 'flex', gap: 8, justifyContent: 'center', flexWrap: 'wrap' }}>
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
                      ? <MarkdownRenderer value={(m.displayed != null ? m.content.slice(0, m.displayed) : m.content) + (m.streaming && (m.displayed ?? 0) >= m.content.length ? ' ▋' : '')} />
                      : <span>{m.content}</span>}
                    {m.role === 'assistant' && m.streaming && !m.content && <span style={{ color: 'var(--text-3)' }}>思考中<span className="dc-dots" /></span>}
                  </div>
                  {!!m.citations?.length && (
                    <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginTop: 6 }}>
                      {m.citations.map((c, j) => (
                        <Tooltip key={j} title={c.content_preview}>
                          <span style={{ fontSize: 12, padding: '3px 8px', borderRadius: 6, background: 'var(--panel-2)', border: '1px solid var(--border-soft)', color: 'var(--text-2)', cursor: 'default' }}>
                            <b style={{ color: 'var(--brand)' }}>[{j + 1}]</b> {c.heading || c.title}
                            {c.relevance_score != null && <span style={{ color: 'var(--text-3)' }}> · {c.relevance_score.toFixed(2)}</span>}
                          </span>
                        </Tooltip>
                      ))}
                    </div>
                  )}
                  {m.role === 'assistant' && m.trace && (
                    <div style={{ marginTop: 6 }}>
                      <Button size="small" type="text" icon={<NodeIndexOutlined />} onClick={() => setMessages((ms) => ms.map((x, k) => (k === i ? { ...x, traceOpen: !x.traceOpen } : x)))}>
                        {m.traceOpen ? t('rag.hideChain', '收起决策链') : t('rag.showChain', '决策链')} · {m.trace.steps.length} 步 {m.trace.total_ms}ms
                      </Button>
                      {m.traceOpen && <DecisionChain trace={m.trace} />}
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div style={{ padding: '12px 16px', borderTop: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
        <div style={{ maxWidth: 760, margin: '0 auto', display: 'flex', gap: 8 }}>
          <Input.TextArea
            autoSize={{ minRows: 1, maxRows: 4 }}
            placeholder={t('rag.ask', '输入问题,Ctrl+Enter 发送')}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onPressEnter={(e) => { if (e.ctrlKey || e.metaKey) { e.preventDefault(); send() } }}
          />
          <Button type="primary" shape="circle" icon={<SendOutlined />} loading={loading} onClick={send} style={{ flexShrink: 0 }} />
        </div>
      </div>
    </div>
  )
}
