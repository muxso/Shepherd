import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
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
        <span style={{ color: '#8a9099', fontSize: 12 }}>
          <span style={{ color: CASE_COLOR }}>●</span> {t('apidef.resCase', '接口用例')} {caseCount}
          <span style={{ color: SCN_COLOR }}>●</span> {t('apidef.resScenario', '场景')} {scnCount}
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

/** Reference graph: center = this API, periphery = referencing resources. Radial/grouped layouts + wheel zoom + drag pan + click-to-jump. Zero-dependency plain SVG. */
function ReferenceGraph({ definition, rows, loading }: { definition: ApiDefinition; rows: RefRow[]; loading: boolean }) {
  const { t } = useI18n()
  const { mode } = useThemeMode()
  const navigate = useNavigate()
  const [hover, setHover] = useState<string | null>(null)
  // SVG presentation attributes don't resolve CSS var(), so read resolved token values per theme mode for fill/stroke.
  const C = useMemo(() => {
    const cs = getComputedStyle(document.documentElement)
    const g = (n: string, fb: string) => cs.getPropertyValue(n).trim() || fb
    return {
      panel: g('--panel', '#ffffff'),
      border: g('--border-soft', '#e1e4e8'),
      text: g('--text', '#1c2024'),
      text2: g('--text-2', '#60646c'),
      text3: g('--text-3', '#8b8d98'),
      brand: g('--brand', '#5b5bd6'),
    }
  }, [mode])
  const [layout, setLayout] = useState<GLayout>('radial')
  const [view, setView] = useState({ k: 1, tx: 0, ty: 0 })
  const wrapRef = useRef<HTMLDivElement>(null)
  const svgRef = useRef<SVGSVGElement>(null)
  const drag = useRef<{ sx: number; sy: number; tx: number; ty: number; moved: boolean } | null>(null)
  const movedRef = useRef(false) // moved during drag → suppress the click on mouseup to avoid accidental jumps
  const trim = (s: string, n = 14) => (s.length > n ? s.slice(0, n) + '…' : s)
  // Viewport tracks the container (viewBox uses container pixel size → no letterboxing, content fills).
  const [size, setSize] = useState({ w: 1000, h: 560 })
  const N = rows.length
  // Note: wrapRef/svgRef are not rendered in loading/empty states → include loading and N in deps
  // so these effects re-run once the graph actually mounts and attach ResizeObserver / wheel listeners
  // (otherwise zoom/auto-fit silently break).
  useEffect(() => {
    const el = wrapRef.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect()
      setSize({ w: Math.max(320, Math.round(r.width)), h: Math.max(320, Math.round(r.height)) })
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [loading, N])
  const VW = size.w
  const VH = size.h

  // Node click jump: scenario → open in the scenario page; api case → open the case in this API's definition page.
  const jump = (n: { id: string; kind: string }) => {
    if (movedRef.current) return // just dragged; ignore this click
    if (n.kind === 'scenario') navigate(`/api/scenario?open=${encodeURIComponent(n.id)}`)
    else navigate(`/api/definition?openCase=${encodeURIComponent(n.id)}`)
  }

  // ---- Layout: node coordinates (origin-centered); `center` = this API's position ----
  const { nodes, center, bounds } = useMemo(() => {
    const colored = rows.map((r) => ({ ...r, color: r.kind === 'case' ? CASE_COLOR : SCN_COLOR }))
    let center = { x: 0, y: 0 }
    let placed: { id: string; name: string; resType: string; kind: string; color: string; x: number; y: number }[] = []
    if (layout === 'radial') {
      const PER = 18
      const rings = Math.max(1, Math.ceil(N / PER))
      const baseR = N <= 6 ? 150 : 170
      const ringGap = 104
      const perRing: number[] = []
      for (let r = 0; r < rings; r++) perRing.push(Math.min(PER, N - r * PER))
      placed = colored.map((row, i) => {
        const ring = Math.floor(i / PER)
        const idx = i - ring * PER
        const ang = (2 * Math.PI * idx) / perRing[ring] - Math.PI / 2 + (ring % 2 ? Math.PI / perRing[ring] : 0)
        const r = baseR + ring * ringGap
        return { ...row, x: r * Math.cos(ang), y: r * Math.sin(ang) }
      })
    } else {
      // Grouped: center on the left; api cases and scenarios each form a vertical column on the right.
      const cases = colored.filter((r) => r.kind === 'case')
      const scns = colored.filter((r) => r.kind === 'scenario')
      const gap = 40
      const colX = 360
      const place = (arr: typeof colored, x: number) =>
        arr.map((row, i) => ({ ...row, x, y: (i - (arr.length - 1) / 2) * gap }))
      placed = [...place(cases, colX), ...place(scns, colX + 320)]
      center = { x: -260, y: 0 }
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
  // Auto-fit when layout/data/container size changes.
  useEffect(() => { fit() /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [layout, N, VW, VH])

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
  const onDown = (e: React.MouseEvent) => {
    movedRef.current = false
    drag.current = { sx: e.clientX, sy: e.clientY, tx: view.tx, ty: view.ty, moved: false }
  }
  const onMove = (e: React.MouseEvent) => {
    if (!drag.current) return
    const rect = svgRef.current?.getBoundingClientRect()
    if (!rect) return
    const dx = ((e.clientX - drag.current.sx) / rect.width) * VW
    const dy = ((e.clientY - drag.current.sy) / rect.height) * VH
    if (Math.abs(dx) + Math.abs(dy) > 3) movedRef.current = true
    setView((v) => ({ ...v, tx: drag.current!.tx + dx, ty: drag.current!.ty + dy }))
  }
  const onUp = () => { drag.current = null }

  if (loading) return <div style={{ height: '100%', minHeight: 360, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-3)' }}>…</div>
  if (N === 0) return <Empty description={t('apidef.refEmpty', '暂无数据')} style={{ padding: 48 }} />

  const gt = `translate(${VW / 2 + view.tx} ${VH / 2 + view.ty}) scale(${view.k})`

  return (
    <div ref={wrapRef} style={{ position: 'relative', width: '100%', height: '100%', minHeight: 360, overflow: 'hidden', background: 'var(--panel)', border: '1px solid var(--border-soft)', borderRadius: 8 }}>
      {/* Controls: layout switch + zoom/fit */}
      <div style={{ position: 'absolute', left: 12, top: 12, zIndex: 2 }}>
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
      <div style={{ position: 'absolute', right: 12, top: 12, zIndex: 2, display: 'flex', flexDirection: 'column', gap: 6 }}>
        <Tooltip title={t('apidef.refZoomIn', '放大')} placement="left"><Button size="small" icon={<ZoomInOutlined />} onClick={() => zoomAround(VW / 2, VH / 2, 1.2)} /></Tooltip>
        <Tooltip title={t('apidef.refZoomOut', '缩小')} placement="left"><Button size="small" icon={<ZoomOutOutlined />} onClick={() => zoomAround(VW / 2, VH / 2, 1 / 1.2)} /></Tooltip>
        <Tooltip title={t('apidef.refFit', '适应')} placement="left"><Button size="small" icon={<ExpandOutlined />} onClick={fit} /></Tooltip>
      </div>
      <span style={{ position: 'absolute', left: 12, bottom: 8, zIndex: 2, color: 'var(--text-3)', fontSize: 11, pointerEvents: 'none' }}>
        {t('apidef.refPanZoomHint', '滚轮缩放 · 拖拽平移')} · {Math.round(view.k * 100)}%
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
        <g transform={gt}>
          {/* Edges: center → each resource; highlighted on hover */}
          {nodes.map((n) => (
            <line key={`e-${n.id}`} x1={center.x} y1={center.y} x2={n.x} y2={n.y} stroke={hover === n.id ? n.color : C.border} strokeWidth={hover === n.id ? 2 : 1} />
          ))}
          {/* Resource nodes */}
          {nodes.map((n) => {
            const on = hover === n.id
            return (
              <g
                key={n.id}
                onMouseEnter={() => setHover(n.id)}
                onMouseLeave={() => setHover((h) => (h === n.id ? null : h))}
                onClick={() => jump(n)}
                style={{ cursor: 'pointer' }}
              >
                <title>{`${n.resType} · ${n.name}（${n.id.slice(0, 8)}）— ${t('apidef.refClickJump', '点击跳转')}`}</title>
                <circle cx={n.x} cy={n.y} r={on ? 9 : 6} fill={C.panel} stroke={n.color} strokeWidth={on ? 3 : 2} />
                <text x={n.x} y={n.y - 13} textAnchor="middle" fontSize={12} fill={on ? C.text : C.text2} fontWeight={on ? 600 : 400} style={{ pointerEvents: 'none', textDecoration: on ? 'underline' : 'none' }}>
                  {trim(n.name)}
                </text>
              </g>
            )
          })}
          {/* Center: this API */}
          <g style={{ pointerEvents: 'none' }}>
            <circle cx={center.x} cy={center.y} r={46} fill={C.brand} />
            <text x={center.x} y={center.y - 6} textAnchor="middle" fontSize={13} fill="#fff" fontWeight={700}>{definition.method}</text>
            <text x={center.x} y={center.y + 14} textAnchor="middle" fontSize={11} fill="rgba(255,255,255,0.8)">{trim(definition.name, 12)}</text>
          </g>
        </g>
      </svg>
    </div>
  )
}
