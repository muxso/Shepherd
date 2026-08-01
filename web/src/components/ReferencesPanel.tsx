import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useScopedNavigate } from '../scope'
import { Button, Empty, Input, Segmented, Table, Tag, Tooltip } from 'antd'
import { SearchOutlined, TableOutlined, PartitionOutlined, ZoomInOutlined, ZoomOutOutlined, ExpandOutlined, ApartmentOutlined, DeploymentUnitOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, type ApiDefinition } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { useThemeMode } from '../themeMode'

type RefRow = { id: string; name: string; resType: string; refType: string; kind: 'case' | 'scenario' }

const CASE_COLOR = '#06a561'
const SCN_COLOR = '#1677ff'
const HUB = '__hub__' // sentinel focus id for the center (this API) node

/** API definition reference relations: which resources (api cases / scenarios) reference it. Two views: table (search + sort) / graph (SVG hub-spoke). */
export default function ReferencesPanel({ definition }: { definition: ApiDefinition }) {
  const { t } = useI18n()
  const { projects, projectId } = useApp()
  const projectName = projects.find((p) => p.id === projectId)?.name || '—'
  const [loading, setLoading] = useState(true)
  const [rows, setRows] = useState<RefRow[]>([])
  const [kw, setKw] = useState('')
  const [view, setView] = useState<'table' | 'graph'>('graph')

  useEffect(() => {
    let alive = true
    setLoading(true)
    api
      .definitionReferences(definition.id)
      .then((r) => {
        if (!alive) return
        const cs: RefRow[] = (r.cases || []).map((c) => ({ id: c.id, name: c.name, resType: t('apidef.resCase', '接口用例'), refType: t('apidef.refQuote', '引用'), kind: 'case' }))
        const ss: RefRow[] = (r.scenarios || []).map((s) => ({ id: s.id, name: s.name, resType: t('apidef.resScenario', '场景'), refType: t('apidef.refQuote', '引用'), kind: 'scenario' }))
        setRows([...cs, ...ss])
      })
      .catch(() => alive && setRows([]))
      .finally(() => alive && setLoading(false))
    return () => {
      alive = false
    }
  }, [definition.id]) // eslint-disable-line react-hooks/exhaustive-deps

  const data = useMemo(() => {
    const q = kw.trim().toLowerCase()
    return q ? rows.filter((r) => r.id.toLowerCase().includes(q) || r.name.toLowerCase().includes(q)) : rows
  }, [rows, kw])

  const cols: ColumnsType<RefRow> = [
    { title: 'ID', dataIndex: 'id', width: 120, sorter: (a, b) => a.id.localeCompare(b.id), render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v.slice(0, 8)}</span> },
    { title: t('apidef.resName', '资源名称'), dataIndex: 'name', ellipsis: true },
    { title: t('apidef.resType', '资源类型'), dataIndex: 'resType', width: 140, render: (v: string, r) => <Tag color={r.kind === 'case' ? 'green' : 'blue'}>{v}</Tag> },
    { title: t('apidef.refType', '引用类型'), dataIndex: 'refType', width: 140 },
    { title: t('apidef.belongOrg', '所属组织'), width: 160, render: () => '—' },
    { title: t('apidef.belongProject', '所属项目'), width: 180, render: () => projectName },
  ]

  const caseCount = data.filter((r) => r.kind === 'case').length
  const scnCount = data.filter((r) => r.kind === 'scenario').length

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 12, flex: 'none' }}>
        <Segmented
          value={view}
          onChange={(v) => setView(v as 'table' | 'graph')}
          options={[
            { value: 'graph', icon: <PartitionOutlined />, label: t('apidef.refGraph', '关系图') },
            { value: 'table', icon: <TableOutlined />, label: t('apidef.refTable', '表格') },
          ]}
        />
        <Input
          allowClear
          prefix={<SearchOutlined style={{ color: '#bbb' }} />}
          placeholder={t('apidef.refSearch', '输入 ID/名称搜索')}
          value={kw}
          onChange={(e) => setKw(e.target.value)}
          style={{ width: 280 }}
        />
        <div style={{ flex: 1 }} />
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, padding: '3px 10px', borderRadius: 999, background: 'var(--panel-2)', border: '1px solid var(--border-soft)', fontSize: 12, color: 'var(--text-2)' }}>
          <span style={{ width: 8, height: 8, borderRadius: '50%', background: CASE_COLOR, display: 'inline-block' }} /> {t('apidef.resCase', '接口用例')}
          <b style={{ color: 'var(--text)' }}>{caseCount}</b>
        </span>
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, padding: '3px 10px', borderRadius: 999, background: 'var(--panel-2)', border: '1px solid var(--border-soft)', fontSize: 12, color: 'var(--text-2)' }}>
          <span style={{ width: 8, height: 8, borderRadius: '50%', background: SCN_COLOR, display: 'inline-block' }} /> {t('apidef.resScenario', '场景')}
          <b style={{ color: 'var(--text)' }}>{scnCount}</b>
        </span>
      </div>

      {view === 'table' ? (
        <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
          <Table
            size="small"
            rowKey="id"
            loading={loading}
            columns={cols}
            dataSource={data}
            pagination={false}
            locale={{ emptyText: t('apidef.refEmpty', '暂无数据') }}
          />
        </div>
      ) : (
        <div style={{ flex: 1, minHeight: 0 }}>
          <ReferenceGraph definition={definition} rows={data} loading={loading} />
        </div>
      )}
    </div>
  )
}

type GLayout = 'radial' | 'grouped'
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

/** Card width from label length (monospace-ish estimate), bounded so short/long names stay tidy. */
const cardWidth = (label: string) => clamp(label.length * 7.4 + 40, 104, 190)
const CARD_H = 34
/** Quadratic-bezier edge that bows gently perpendicular to the center→node line (deck-style curved connectors). */
function edgePath(cx: number, cy: number, x: number, y: number): string {
  const mx = (cx + x) / 2
  const my = (cy + y) / 2
  const dx = x - cx
  const dy = y - cy
  const len = Math.hypot(dx, dy) || 1
  const bow = Math.min(46, len * 0.13)
  return `M ${cx} ${cy} Q ${mx - (dy / len) * bow} ${my + (dx / len) * bow} ${x} ${y}`
}

/** Reference graph: center = this API, periphery = referencing resources. Deck-style interaction (graphcon-deck):
 *  ← → step focus between nodes, click a dim node to travel/recenter, click the focused node to open, hover for a
 *  detail card, wheel zoom, drag pan, drag a node (springs back). Phyllotaxis / grouped layouts. Zero-dependency SVG. */
function ReferenceGraph({ definition, rows, loading }: { definition: ApiDefinition; rows: RefRow[]; loading: boolean }) {
  const { t } = useI18n()
  const { mode } = useThemeMode()
  const navigate = useScopedNavigate()
  const [hover, setHover] = useState<string | null>(null)
  // SVG presentation attributes don't resolve CSS var(), so read resolved token values per theme mode for fill/stroke.
  const C = useMemo(() => {
    const cs = getComputedStyle(document.documentElement)
    const g = (n: string, fb: string) => cs.getPropertyValue(n).trim() || fb
    return {
      panel: g('--panel', '#ffffff'),
      panel2: g('--panel-2', '#f7f8fa'),
      border: g('--border', '#e5e6eb'),
      borderSoft: g('--border-soft', '#f2f3f5'),
      text: g('--text', '#1c2024'),
      text2: g('--text-2', '#60646c'),
      text3: g('--text-3', '#8b8d98'),
      brand: g('--brand', '#1664ff'),
    }
  }, [mode])
  const [layout, setLayout] = useState<GLayout>('radial')
  const [view, setView] = useState({ k: 1, tx: 0, ty: 0 })
  const wrapRef = useRef<HTMLDivElement>(null)
  const svgRef = useRef<SVGSVGElement>(null)
  const drag = useRef<{ sx: number; sy: number; tx: number; ty: number; moved: boolean } | null>(null)
  const movedRef = useRef(false) // moved during drag → suppress the click on mouseup to avoid accidental jumps
  // Per-node drag offsets (deck-style: a node can be dragged, then springs back). Ephemeral; never persisted.
  const [offsets, setOffsets] = useState<Record<string, { dx: number; dy: number }>>({})
  const offsetsRef = useRef(offsets)
  offsetsRef.current = offsets
  const [draggingId, setDraggingId] = useState<string | null>(null)
  const nodeDrag = useRef<{ id: string; sx: number; sy: number; ox: number; oy: number } | null>(null)
  const springRaf = useRef<number | undefined>(undefined)
  // Deck-style focus/travel: the currently focused node (default the hub); ← → step through it, click travels to it.
  const [focus, setFocus] = useState<string>(HUB)
  const viewRef = useRef(view)
  viewRef.current = view
  const travelRaf = useRef<number | undefined>(undefined)
  const trim = (s: string, n = 16) => (s.length > n ? s.slice(0, n - 1) + '…' : s)
  // Viewport tracks the container (viewBox uses container pixel size → no letterboxing, content fills).
  const [size, setSize] = useState({ w: 1000, h: 560 })
  const N = rows.length
  // Measure the container synchronously (before paint) and fit ONCE per (layout, N). Fitting pre-paint with
  // the real size means the graph renders already settled — no open-time zoom/snap animation. Later container
  // resizes keep `size` current (for zoom math) but do NOT re-fit, so nothing animates while the tab expands.
  const fittedKey = useRef('')
  useLayoutEffect(() => {
    const el = wrapRef.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const measure = () => {
      const r = el.getBoundingClientRect()
      return { w: Math.max(320, Math.round(r.width)), h: Math.max(320, Math.round(r.height)) }
    }
    const s = measure()
    setSize(s)
    const key = `${layout}|${N}`
    if (fittedKey.current !== key) {
      fittedKey.current = key
      const w = Math.max(1, bounds.x1 - bounds.x0 + 200)
      const h = Math.max(1, bounds.y1 - bounds.y0 + 160)
      const k = clamp(Math.min(s.w / w, s.h / h), 0.25, 1.6)
      setView({ k, tx: -((bounds.x0 + bounds.x1) / 2) * k, ty: -((bounds.y0 + bounds.y1) / 2) * k })
    }
    const ro = new ResizeObserver(() => setSize(measure()))
    ro.observe(el)
    return () => ro.disconnect()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loading, N, layout])
  const VW = size.w
  const VH = size.h

  // Smoothly ease the viewport to `to` (deck-style travel animation on click / arrow-key).
  const animateView = (to: { k: number; tx: number; ty: number }) => {
    if (travelRaf.current) cancelAnimationFrame(travelRaf.current)
    const from = viewRef.current
    const start = performance.now()
    const dur = 380
    const step = (now: number) => {
      const p = Math.min(1, (now - start) / dur)
      const e = 1 - Math.pow(1 - p, 3)
      setView({ k: from.k + (to.k - from.k) * e, tx: from.tx + (to.tx - from.tx) * e, ty: from.ty + (to.ty - from.ty) * e })
      if (p < 1) travelRaf.current = requestAnimationFrame(step)
    }
    travelRaf.current = requestAnimationFrame(step)
  }
  // "Travel" to a node: mark it focused and recenter the viewport on it (deck: click a dim node / ← → to travel).
  const travelTo = (id: string) => {
    setFocus(id)
    const target = id === HUB ? center : nodes.find((n) => n.id === id)
    if (!target) return
    const k = clamp(viewRef.current.k, 0.7, 1.4)
    animateView({ k, tx: -target.x * k, ty: -target.y * k })
  }
  // Open the underlying resource: scenario → scenario page; api case → this API's definition page.
  const openNode = (n: { id: string; kind: string }) => {
    if (n.kind === 'scenario') navigate(`/api/scenario?open=${encodeURIComponent(n.id)}`)
    else navigate(`/api/definition?openCase=${encodeURIComponent(n.id)}`)
  }
  // Click a dim node → travel to it; click the already-focused node → open it (deck "click to travel / focused to open").
  const onNodeClick = (n: { id: string; kind: string }) => {
    if (movedRef.current) return // just dragged; ignore this click
    if (focus === n.id) openNode(n)
    else travelTo(n.id)
  }

  // ---- Layout: node coordinates (origin-centered); `center` = this API's position ----
  const { nodes, center, bounds } = useMemo(() => {
    const colored = rows.map((r) => ({ ...r, color: r.kind === 'case' ? CASE_COLOR : SCN_COLOR }))
    let center = { x: 0, y: 0 }
    let placed: { id: string; name: string; resType: string; kind: string; color: string; x: number; y: number }[] = []
    if (layout === 'radial') {
      // Phyllotaxis (sunflower) spread: golden-angle placement gives an organic cloud around the hub,
      // instead of rigid concentric rings — closer to the graphcon-deck look.
      const GOLDEN = Math.PI * (3 - Math.sqrt(5)) // ~137.5°, the golden angle
      const baseR = N <= 4 ? 240 : 200
      const step = N <= 8 ? 96 : 78
      placed = colored.map((row, i) => {
        const r = baseR + step * Math.sqrt(i)
        const ang = i * GOLDEN - Math.PI / 2
        return { ...row, x: r * Math.cos(ang), y: r * Math.sin(ang) }
      })
    } else {
      // Grouped: center on the left; api cases and scenarios each form a vertical column on the right.
      const cases = colored.filter((r) => r.kind === 'case')
      const scns = colored.filter((r) => r.kind === 'scenario')
      const gap = 48
      const colX = 420
      const place = (arr: typeof colored, x: number) =>
        arr.map((row, i) => ({ ...row, x, y: (i - (arr.length - 1) / 2) * gap }))
      placed = [...place(cases, colX), ...place(scns, colX + 360)]
      center = { x: -320, y: 0 }
    }
    const xs = [center.x, ...placed.map((p) => p.x)]
    const ys = [center.y, ...placed.map((p) => p.y)]
    const bounds = { x0: Math.min(...xs), x1: Math.max(...xs), y0: Math.min(...ys), y1: Math.max(...ys) }
    return { nodes: placed, center, bounds }
  }, [rows, layout, N])

  // Fit: scale and center to the content bounding box.
  const fit = () => {
    const w = Math.max(1, bounds.x1 - bounds.x0 + 200)
    const h = Math.max(1, bounds.y1 - bounds.y0 + 160)
    const k = clamp(Math.min(VW / w, VH / h), 0.25, 1.6)
    const cxC = (bounds.x0 + bounds.x1) / 2
    const cyC = (bounds.y0 + bounds.y1) / 2
    setView({ k, tx: -cxC * k, ty: -cyC * k })
  }
  const zoomAround = (px: number, py: number, factor: number) => {
    setView((v) => {
      const k2 = clamp(v.k * factor, 0.25, 4)
      const cxc = (px - (VW / 2 + v.tx)) / v.k
      const cyc = (py - (VH / 2 + v.ty)) / v.k
      return { k: k2, tx: px - VW / 2 - k2 * cxc, ty: py - VH / 2 - k2 * cyc }
    })
  }
  // Wheel zoom uses a native non-passive listener: React's onWheel is passive, so preventDefault is a no-op → the page would scroll/jitter along.
  useEffect(() => {
    const svg = svgRef.current
    if (!svg) return
    const onWheelNative = (e: WheelEvent) => {
      e.preventDefault()
      const rect = svg.getBoundingClientRect()
      const px = ((e.clientX - rect.left) / rect.width) * VW
      const py = ((e.clientY - rect.top) / rect.height) * VH
      const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12
      setView((v) => {
        const k2 = clamp(v.k * factor, 0.25, 4)
        const cxc = (px - (VW / 2 + v.tx)) / v.k
        const cyc = (py - (VH / 2 + v.ty)) / v.k
        return { k: k2, tx: px - VW / 2 - k2 * cxc, ty: py - VH / 2 - k2 * cyc }
      })
    }
    svg.addEventListener('wheel', onWheelNative, { passive: false })
    return () => svg.removeEventListener('wheel', onWheelNative)
  }, [VW, VH, loading, N])
  // Cancel any in-flight spring-back animation on unmount.
  useEffect(() => () => { if (springRaf.current) cancelAnimationFrame(springRaf.current) }, [])
  const onDown = (e: React.MouseEvent) => {
    movedRef.current = false
    drag.current = { sx: e.clientX, sy: e.clientY, tx: view.tx, ty: view.ty, moved: false }
  }
  const onMove = (e: React.MouseEvent) => {
    const d = drag.current // snapshot: the setView updater may re-run (StrictMode) after mouseup nulls the ref
    if (!d) return
    const rect = svgRef.current?.getBoundingClientRect()
    if (!rect) return
    const dx = ((e.clientX - d.sx) / rect.width) * VW
    const dy = ((e.clientY - d.sy) / rect.height) * VH
    if (Math.abs(dx) + Math.abs(dy) > 3) movedRef.current = true
    setView((v) => ({ ...v, tx: d.tx + dx, ty: d.ty + dy }))
  }
  const onUp = () => { drag.current = null }

  // ---- Per-node drag (deck-style). Uses window listeners so the pointer can leave the node; springs back on release. ----
  const startNodeDrag = (e: React.MouseEvent, id: string) => {
    e.stopPropagation() // don't start a canvas pan
    if (springRaf.current) cancelAnimationFrame(springRaf.current)
    const cur = offsetsRef.current[id] || { dx: 0, dy: 0 }
    nodeDrag.current = { id, sx: e.clientX, sy: e.clientY, ox: cur.dx, oy: cur.dy }
    movedRef.current = false
    setDraggingId(id)
    const rect = svgRef.current?.getBoundingClientRect()
    const move = (ev: MouseEvent) => {
      const nd = nodeDrag.current
      if (!nd || !rect) return
      // Convert screen delta → world delta (undo container scale and zoom).
      const dx = ((ev.clientX - nd.sx) / rect.width) * VW / view.k
      const dy = ((ev.clientY - nd.sy) / rect.height) * VH / view.k
      if (Math.abs(ev.clientX - nd.sx) + Math.abs(ev.clientY - nd.sy) > 3) movedRef.current = true
      setOffsets((o) => ({ ...o, [id]: { dx: nd.ox + dx, dy: nd.oy + dy } }))
    }
    const up = () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
      nodeDrag.current = null
      setDraggingId(null)
      springBack(id)
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }
  // Ease the released node's offset back to origin (easeOutCubic).
  const springBack = (id: string) => {
    const start = performance.now()
    const o0 = offsetsRef.current[id] || { dx: 0, dy: 0 }
    const dur = 340
    const step = (now: number) => {
      const p = Math.min(1, (now - start) / dur)
      const e = 1 - Math.pow(1 - p, 3)
      const dx = o0.dx * (1 - e)
      const dy = o0.dy * (1 - e)
      if (p < 1) {
        setOffsets((o) => ({ ...o, [id]: { dx, dy } }))
        springRaf.current = requestAnimationFrame(step)
      } else {
        setOffsets((o) => { const n = { ...o }; delete n[id]; return n })
      }
    }
    springRaf.current = requestAnimationFrame(step)
  }
  const posOf = (id: string, x: number, y: number) => {
    const o = offsets[id]
    return o ? { x: x + o.dx, y: y + o.dy } : { x, y }
  }

  // ← → steps focus through [hub, ...nodes] and travels to each (deck-style). Ignored while typing in a field.
  useEffect(() => {
    const order = [HUB, ...nodes.map((n) => n.id)]
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return
      const ae = document.activeElement as HTMLElement | null
      if (ae && (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA' || ae.isContentEditable)) return
      e.preventDefault()
      const i = Math.max(0, order.indexOf(focus))
      const ni = e.key === 'ArrowRight' ? (i + 1) % order.length : (i - 1 + order.length) % order.length
      travelTo(order[ni])
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, focus])
  useEffect(() => () => { if (travelRaf.current) cancelAnimationFrame(travelRaf.current) }, [])

  if (loading) return <div style={{ height: '100%', minHeight: 360, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-3)' }}>…</div>
  if (N === 0) return <Empty description={t('apidef.refEmpty', '暂无数据')} style={{ padding: 48 }} />

  const gt = `translate(${VW / 2 + view.tx} ${VH / 2 + view.ty}) scale(${view.k})`
  // Emphasis driver: the hovered node, else the focused node (hub focus = nothing dimmed = overview).
  const active = hover ?? (focus !== HUB ? focus : null)

  return (
    <div
      ref={wrapRef}
      style={{
        position: 'relative',
        width: '100%',
        height: '100%',
        minHeight: 360,
        overflow: 'hidden',
        // Deck-style canvas: faint dot grid over a soft radial vignette.
        backgroundColor: 'var(--panel)',
        backgroundImage: 'radial-gradient(var(--border) 1px, transparent 1.4px), radial-gradient(circle at 50% 42%, var(--panel) 0%, var(--panel-2) 130%)',
        backgroundSize: '22px 22px, 100% 100%',
        border: '1px solid var(--border-soft)',
        borderRadius: 10,
      }}
    >
      {/* Controls: layout switch + zoom/fit */}
      <div style={{ position: 'absolute', left: 12, top: 12, zIndex: 2, background: 'var(--panel)', borderRadius: 8, padding: 3, boxShadow: '0 1px 4px rgba(0,0,0,0.08)', border: '1px solid var(--border-soft)' }}>
        <Segmented
          size="small"
          value={layout}
          onChange={(v) => setLayout(v as GLayout)}
          options={[
            { value: 'radial', icon: <DeploymentUnitOutlined />, label: t('apidef.refLayoutRadial', '放射') },
            { value: 'grouped', icon: <ApartmentOutlined />, label: t('apidef.refLayoutGrouped', '分组') },
          ]}
        />
      </div>
      <div style={{ position: 'absolute', right: 12, top: 12, zIndex: 2, display: 'flex', flexDirection: 'column', gap: 6, background: 'var(--panel)', borderRadius: 8, padding: 4, boxShadow: '0 1px 4px rgba(0,0,0,0.08)', border: '1px solid var(--border-soft)' }}>
        <Tooltip title={t('apidef.refZoomIn', '放大')} placement="left"><Button size="small" icon={<ZoomInOutlined />} onClick={() => zoomAround(VW / 2, VH / 2, 1.2)} /></Tooltip>
        <Tooltip title={t('apidef.refZoomOut', '缩小')} placement="left"><Button size="small" icon={<ZoomOutOutlined />} onClick={() => zoomAround(VW / 2, VH / 2, 1 / 1.2)} /></Tooltip>
        <Tooltip title={t('apidef.refFit', '适应')} placement="left"><Button size="small" icon={<ExpandOutlined />} onClick={fit} /></Tooltip>
      </div>
      <span style={{ position: 'absolute', left: 12, bottom: 8, zIndex: 2, color: 'var(--text-3)', fontSize: 11, pointerEvents: 'none' }}>
        {t('apidef.refDeckHint', '← → 切换 · 点暗节点聚焦 · 再点打开 · 悬停查看 · 滚轮缩放 · 拖拽平移')} · {Math.round(view.k * 100)}%
      </span>

      <svg
        ref={svgRef}
        viewBox={`0 0 ${VW} ${VH}`}
        style={{ width: '100%', height: '100%', display: 'block', cursor: drag.current ? 'grabbing' : 'grab', touchAction: 'none' }}
        onMouseDown={onDown}
        onMouseMove={onMove}
        onMouseUp={onUp}
        onMouseLeave={onUp}
        role="img"
      >
        <defs>
          <filter id="ref-card-shadow" x="-30%" y="-30%" width="160%" height="180%">
            <feDropShadow dx="0" dy="2" stdDeviation="3" floodColor="#000" floodOpacity={mode === 'dark' ? 0.5 : 0.14} />
          </filter>
        </defs>
        <g transform={gt}>
          {/* Edges: center → each resource, gently curved. Focused node's edge lights up in its color; others dim. */}
          {nodes.map((n) => {
            const p = posOf(n.id, n.x, n.y)
            const on = active === n.id
            const dim = active != null && !on
            return (
              <path
                key={`e-${n.id}`}
                d={edgePath(center.x, center.y, p.x, p.y)}
                fill="none"
                stroke={on ? n.color : C.border}
                strokeWidth={on ? 2.25 : 1.25}
                strokeLinecap="round"
                opacity={dim ? 0.2 : on ? 0.95 : 0.55}
                style={{ transition: 'opacity .16s, stroke-width .16s' }}
              />
            )
          })}
          {/* Resource nodes as cards: colored kind-dot + name. Hover focuses (dims the rest); drag moves it (springs back). */}
          {nodes.map((n) => {
            const on = active === n.id
            const dim = active != null && !on
            const isFocus = focus === n.id
            const p = posOf(n.id, n.x, n.y)
            const label = trim(n.name)
            const w = cardWidth(label)
            const dragging = draggingId === n.id
            return (
              <g
                key={n.id}
                transform={`translate(${p.x} ${p.y})`}
                onMouseEnter={() => setHover(n.id)}
                onMouseLeave={() => setHover((h) => (h === n.id ? null : h))}
                onMouseDown={(e) => startNodeDrag(e, n.id)}
                onClick={() => onNodeClick(n)}
                opacity={dim ? 0.3 : 1}
                style={{ cursor: dragging ? 'grabbing' : 'pointer', transition: 'opacity .16s' }}
              >
                <title>{`${n.resType} · ${n.name}（${n.id.slice(0, 8)}）— ${isFocus ? t('apidef.refClickOpen', '点击打开') : t('apidef.refClickFocus', '点击聚焦')}`}</title>
                {isFocus && <rect x={-w / 2 - 4} y={-CARD_H / 2 - 4} width={w + 8} height={CARD_H + 8} rx={12} fill="none" stroke={n.color} strokeWidth={1.5} opacity={0.4} />}
                <rect
                  x={-w / 2}
                  y={-CARD_H / 2}
                  width={w}
                  height={CARD_H}
                  rx={9}
                  fill={C.panel}
                  stroke={on || isFocus ? n.color : C.border}
                  strokeWidth={on || isFocus ? 2 : 1.25}
                  filter={dim ? undefined : 'url(#ref-card-shadow)'}
                />
                <circle cx={-w / 2 + 15} cy={0} r={4.5} fill={n.color} />
                <text x={-w / 2 + 27} y={4} fontSize={12.5} fill={on ? C.text : C.text2} fontWeight={on ? 600 : 500} style={{ pointerEvents: 'none' }}>
                  {label}
                </text>
              </g>
            )
          })}
          {/* Center: this API — the hub card. Click recenters (travel back to overview). */}
          <g transform={`translate(${center.x} ${center.y})`} onClick={() => travelTo(HUB)} style={{ cursor: 'pointer' }}>
            <title>{`${definition.method || definition.protocol} ${definition.name} — ${t('apidef.refBackToHub', '回到中心')}`}</title>
            {focus === HUB && <rect x={-77} y={-32} width={154} height={64} rx={15} fill="none" stroke={C.brand} strokeWidth={1.5} opacity={0.35} />}
            <rect x={-72} y={-27} width={144} height={54} rx={12} fill={C.brand} filter="url(#ref-card-shadow)" />
            <text x={0} y={-4} textAnchor="middle" fontSize={13} fill="#fff" fontWeight={700} letterSpacing={0.4} style={{ pointerEvents: 'none' }}>{definition.method || definition.protocol}</text>
            <text x={0} y={15} textAnchor="middle" fontSize={11} fill="rgba(255,255,255,0.82)" style={{ pointerEvents: 'none' }}>{trim(definition.name, 15)}</text>
          </g>
        </g>
      </svg>

      {/* Hover-for-detail card (deck-style), anchored beside the hovered node. */}
      {hover && (() => {
        const n = nodes.find((x) => x.id === hover)
        if (!n) return null
        const p = posOf(n.id, n.x, n.y)
        const sx = clamp(VW / 2 + view.tx + view.k * p.x + 16, 8, VW - 210)
        const sy = clamp(VH / 2 + view.ty + view.k * p.y - 10, 8, VH - 92)
        const focused = focus === n.id
        return (
          <div style={{ position: 'absolute', left: sx, top: sy, zIndex: 3, pointerEvents: 'none', width: 194, background: 'var(--panel)', border: `1px solid ${n.color}`, borderRadius: 9, boxShadow: '0 6px 20px rgba(0,0,0,0.18)', padding: '8px 11px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 3 }}>
              <span style={{ width: 8, height: 8, borderRadius: '50%', background: n.color, flexShrink: 0 }} />
              <span style={{ fontSize: 12, color: 'var(--text-2)' }}>{n.resType}</span>
            </div>
            <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)', wordBreak: 'break-all', marginBottom: 3 }}>{n.name}</div>
            <div className="ms-mono" style={{ fontSize: 11, color: 'var(--text-3)', marginBottom: 4 }}>{n.id.slice(0, 8)}</div>
            <div style={{ fontSize: 11, color: focused ? n.color : 'var(--text-3)' }}>{focused ? t('apidef.refClickOpen', '点击打开') : t('apidef.refClickFocus', '点击聚焦')}</div>
          </div>
        )
      })()}
    </div>
  )
}
