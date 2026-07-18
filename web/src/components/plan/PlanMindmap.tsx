import { useEffect, useMemo, useRef, useState } from 'react'
import { Alert, Button, Input, Tooltip } from 'antd'
import {
  DeleteOutlined,
  ExpandOutlined,
  MinusSquareOutlined,
  PlusCircleOutlined,
  PlusSquareOutlined,
  SaveOutlined,
  SettingOutlined,
} from '@ant-design/icons'
import { message, modal } from '../../feedback'
import {
  api,
  ApiError,
  type Environment,
  type PlanningDoc,
  type PlanningNode,
  type ResourcePool,
} from '../../api'
import { useI18n } from '../../i18n'
import PlanNodeConfig from './PlanNodeConfig'

// Layout constants for the layered left-to-right tree.
const NODE_W = 200
const NODE_H = 52
const H_GAP = 72
const V_GAP = 16
const PAD = 32

const HINT_KEY = 'shepherd.planMindmapHint'

const uid = () => `n_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`

// Default planning tree: one category node per case kind, mirroring MeterSphere.
function defaultNodes(t: (k: string, d: string) => string): PlanningNode[] {
  return [
    { id: uid(), name: t('plan.mm.funcCases', '功能用例'), kind: 'category', children: [] },
    { id: uid(), name: t('plan.mm.apiCases', '接口用例'), kind: 'category', children: [] },
    { id: uid(), name: t('plan.mm.scenarioCases', '场景用例'), kind: 'category', children: [] },
  ]
}

/** Test-planning tab: pan/zoom mind-map editor persisted as the plan's planning doc. */
export default function PlanMindmap({ planId, projectId }: { planId: string; projectId: string }) {
  const { t } = useI18n()
  const [nodes, setNodes] = useState<PlanningNode[]>([])
  const [names, setNames] = useState<Record<string, string>>({})
  const [scenarioNames, setScenarioNames] = useState<Record<string, string>>({})
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set())
  const [selectedId, setSelectedId] = useState('')
  const [hoverId, setHoverId] = useState('')
  const [envs, setEnvs] = useState<Environment[]>([])
  const [pools, setPools] = useState<ResourcePool[]>([])
  const [view, setView] = useState({ x: 24, y: 24, z: 1 })
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [hint, setHint] = useState(() => localStorage.getItem(HINT_KEY) !== '1')
  const containerRef = useRef<HTMLDivElement>(null)
  const drag = useRef<{ sx: number; sy: number; ox: number; oy: number } | null>(null)

  useEffect(() => {
    api.planDetail(planId)
      .then((d) => {
        setNodes(d.planning?.nodes?.length ? d.planning.nodes : defaultNodes(t))
        setNames(d.planning?.caseNames || {})
        setScenarioNames(d.planning?.scenarioNames || {})
      })
      .catch(() => setNodes(defaultNodes(t)))
    api.environments(projectId).then(setEnvs).catch(() => setEnvs([]))
    api.resourcePools().then(setPools).catch(() => setPools([]))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [planId, projectId])

  // Virtual root: the fixed "测试规划" node; real nodes are its children.
  const root: PlanningNode = useMemo(
    () => ({ id: '__root__', name: t('plan.mm.root', '测试规划'), kind: 'category', children: nodes }),
    [nodes, t],
  )

  // Layered tree layout: leaves stack top-to-bottom, parents center on their children.
  const layout = useMemo(() => {
    const pos = new Map<string, { x: number; y: number }>()
    const edges: { from: string; to: string }[] = []
    let cursor = 0
    let maxDepth = 0
    const place = (n: PlanningNode, depth: number): number => {
      maxDepth = Math.max(maxDepth, depth)
      const x = depth * (NODE_W + H_GAP)
      const kids = collapsed.has(n.id) ? [] : n.children || []
      if (!kids.length) {
        const y = cursor
        cursor += NODE_H + V_GAP
        pos.set(n.id, { x, y })
        return y + NODE_H / 2
      }
      const mids = kids.map((k) => {
        edges.push({ from: n.id, to: k.id })
        return place(k, depth + 1)
      })
      const mid = (mids[0] + mids[mids.length - 1]) / 2
      pos.set(n.id, { x, y: mid - NODE_H / 2 })
      return mid
    }
    place(root, 0)
    return {
      pos,
      edges,
      height: Math.max(cursor, NODE_H),
      width: (maxDepth + 1) * (NODE_W + H_GAP),
    }
  }, [root, collapsed])

  const flat = useMemo(() => {
    const out: { node: PlanningNode; parent: PlanningNode | null }[] = []
    const walk = (n: PlanningNode, parent: PlanningNode | null) => {
      out.push({ node: n, parent })
      if (!collapsed.has(n.id)) (n.children || []).forEach((k) => walk(k, n))
    }
    walk(root, null)
    return out
  }, [root, collapsed])

  const findNode = (list: PlanningNode[], id: string): PlanningNode | null => {
    for (const n of list) {
      if (n.id === id) return n
      const hit = findNode(n.children || [], id)
      if (hit) return hit
    }
    return null
  }
  const selected = selectedId ? findNode(nodes, selectedId) : null

  const mutate = (fn: (list: PlanningNode[]) => PlanningNode[]) => {
    setNodes((prev) => fn(prev))
    setDirty(true)
  }
  const patchNode = (id: string, patch: Partial<PlanningNode>) => {
    const walk = (list: PlanningNode[]): PlanningNode[] =>
      list.map((n) => (n.id === id ? { ...n, ...patch } : { ...n, children: walk(n.children || []) }))
    mutate(walk)
  }

  const addChild = (parentId: string) => {
    let name = ''
    modal.confirm({
      title: t('plan.mm.addPoint', '添加测试点'),
      content: (
        <Input
          placeholder={t('plan.mm.nodeName', '测试点名称')}
          onChange={(e) => (name = e.target.value)}
          style={{ marginTop: 8 }}
          autoFocus
        />
      ),
      onOk: () => {
        const label = name.trim() || t('plan.mm.point', '测试点')
        const child: PlanningNode = {
          id: uid(),
          name: label,
          kind: 'point',
          children: [],
          config: { inherit: true, mode: 'serial' },
          caseIds: [],
          scenarioIds: [],
        }
        if (parentId === '__root__') mutate((list) => [...list, { ...child, kind: 'point' }])
        else {
          const walk = (list: PlanningNode[]): PlanningNode[] =>
            list.map((n) =>
              n.id === parentId
                ? { ...n, children: [...(n.children || []), child] }
                : { ...n, children: walk(n.children || []) },
            )
          mutate(walk)
        }
        setCollapsed((c) => {
          const next = new Set(c)
          next.delete(parentId)
          return next
        })
      },
    })
  }

  const removeNode = (id: string) => {
    modal.confirm({
      title: t('plan.mm.deleteNode', '删除该测试点及其子节点?'),
      okButtonProps: { danger: true },
      onOk: () => {
        const walk = (list: PlanningNode[]): PlanningNode[] =>
          list.filter((n) => n.id !== id).map((n) => ({ ...n, children: walk(n.children || []) }))
        mutate(walk)
        if (selectedId === id) setSelectedId('')
      },
    })
  }

  const toggleCollapse = (id: string) =>
    setCollapsed((c) => {
      const next = new Set(c)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  const allParentIds = useMemo(() => {
    const ids: string[] = []
    const walk = (n: PlanningNode) => {
      if (n.children?.length) {
        if (n.id !== '__root__') ids.push(n.id)
        n.children.forEach(walk)
      }
    }
    walk(root)
    return ids
  }, [root])
  const allCollapsed = allParentIds.length > 0 && allParentIds.every((id) => collapsed.has(id))
  const collapseAll = () => setCollapsed(allCollapsed ? new Set() : new Set(allParentIds))

  const save = async () => {
    setSaving(true)
    try {
      const doc: PlanningDoc = { nodes, caseNames: names, scenarioNames }
      await api.savePlanPlanning(planId, doc)
      setDirty(false)
      message.success(t('plan.mm.saved', '测试规划已保存'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.mm.saveFail', '保存失败'))
    } finally {
      setSaving(false)
    }
  }
  const saveRef = useRef(save)
  saveRef.current = save

  // Cmd/Ctrl+S saves the planning doc while this tab is mounted.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
        e.preventDefault()
        saveRef.current()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  // Pan (drag on background) + wheel zoom.
  const onMouseDown = (e: React.MouseEvent) => {
    if ((e.target as Element).closest('[data-mm-node]')) return
    drag.current = { sx: e.clientX, sy: e.clientY, ox: view.x, oy: view.y }
    const move = (ev: MouseEvent) => {
      const d = drag.current
      if (d) setView((v) => ({ ...v, x: d.ox + (ev.clientX - d.sx), y: d.oy + (ev.clientY - d.sy) }))
    }
    const up = () => {
      drag.current = null
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }
  const onWheel = (e: React.WheelEvent) => {
    const z = Math.min(2, Math.max(0.4, view.z * (e.deltaY > 0 ? 0.92 : 1.08)))
    setView((v) => ({ ...v, z }))
  }

  const envName = (id?: string) => envs.find((x) => x.id === id)?.name
  const poolName = (id?: string) => pools.find((x) => x.id === id)?.name

  const width = PAD * 2 + layout.width

  const chip = (text: string, color: string, bg: string) => (
    <span
      key={text + color}
      style={{
        fontSize: 11,
        lineHeight: '16px',
        padding: '0 6px',
        borderRadius: 8,
        color,
        background: bg,
        whiteSpace: 'nowrap',
        maxWidth: 84,
        overflow: 'hidden',
        textOverflow: 'ellipsis',
      }}
    >
      {text}
    </span>
  )

  const renderNode = (node: PlanningNode) => {
    const p = layout.pos.get(node.id)
    if (!p) return null
    const isRoot = node.id === '__root__'
    const caseCount = (node.caseIds?.length || 0) + (node.scenarioIds?.length || 0)
    const cfg = node.config
    const showMode = !!cfg && cfg.inherit === false && !!cfg.mode
    const hovered = hoverId === node.id
    const isSelected = selectedId === node.id
    return (
      <div
        key={node.id}
        data-mm-node
        onMouseEnter={() => setHoverId(node.id)}
        onMouseLeave={() => setHoverId((h) => (h === node.id ? '' : h))}
        onClick={() => !isRoot && setSelectedId(node.id)}
        style={{
          position: 'absolute',
          left: p.x,
          top: p.y,
          width: NODE_W,
          minHeight: NODE_H,
          boxSizing: 'border-box',
          padding: '6px 10px',
          borderRadius: 8,
          cursor: isRoot ? 'default' : 'pointer',
          background: isRoot ? 'var(--brand)' : 'var(--panel)',
          color: isRoot ? '#fff' : 'var(--text)',
          border: isRoot ? '1px solid var(--brand)' : `1px solid ${isSelected ? 'var(--brand)' : 'var(--border)'}`,
          boxShadow: isSelected ? '0 0 0 2px var(--brand-soft)' : '0 1px 3px rgba(0,0,0,0.06)',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <span style={{ flex: 1, fontSize: 13, fontWeight: isRoot ? 600 : 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {node.name}
          </span>
          {showMode && (
            <span
              style={{
                fontSize: 11,
                width: 18,
                height: 18,
                lineHeight: '18px',
                textAlign: 'center',
                borderRadius: '50%',
                background: 'var(--brand-soft)',
                color: 'var(--brand)',
                flexShrink: 0,
              }}
            >
              {cfg?.mode === 'parallel' ? t('plan.mm.parallel', '并') : t('plan.mm.serial', '串')}
            </span>
          )}
        </div>
        {!isRoot && (caseCount > 0 || cfg?.envId || cfg?.poolId) && (
          <div style={{ display: 'flex', gap: 4, marginTop: 4, flexWrap: 'wrap' }}>
            {caseCount > 0 && chip(`${caseCount}${t('plan.mm.casesUnit', '条')}`, 'var(--brand)', 'var(--brand-soft)')}
            {envName(cfg?.envId) && chip(envName(cfg?.envId) as string, 'var(--success)', 'rgba(0,180,42,0.12)')}
            {poolName(cfg?.poolId) && chip(poolName(cfg?.poolId) as string, 'var(--warning)', 'rgba(255,125,0,0.12)')}
          </div>
        )}
        {/* Hover toolbar: add child / configure / delete. */}
        {hovered && (
          <div
            style={{
              position: 'absolute',
              top: -26,
              right: 0,
              display: 'flex',
              gap: 2,
              background: 'var(--panel)',
              border: '1px solid var(--border-soft)',
              borderRadius: 6,
              padding: '1px 4px',
              boxShadow: '0 2px 8px rgba(0,0,0,0.1)',
            }}
          >
            <Tooltip title={t('plan.mm.addPoint', '添加测试点')}>
              <Button type="text" size="small" icon={<PlusCircleOutlined />} onClick={(e) => { e.stopPropagation(); addChild(node.id) }} />
            </Tooltip>
            {!isRoot && (
              <>
                <Tooltip title={t('plan.mm.config', '配置')}>
                  <Button type="text" size="small" icon={<SettingOutlined />} onClick={(e) => { e.stopPropagation(); setSelectedId(node.id) }} />
                </Tooltip>
                <Tooltip title={t('plan.mm.delete', '删除')}>
                  <Button type="text" size="small" danger icon={<DeleteOutlined />} onClick={(e) => { e.stopPropagation(); removeNode(node.id) }} />
                </Tooltip>
              </>
            )}
          </div>
        )}
        {/* Collapse toggle on the right edge for nodes with children. */}
        {(node.children?.length || 0) > 0 && (
          <span
            onClick={(e) => {
              e.stopPropagation()
              toggleCollapse(node.id)
            }}
            style={{
              position: 'absolute',
              right: -10,
              top: '50%',
              marginTop: -9,
              width: 18,
              height: 18,
              lineHeight: '16px',
              textAlign: 'center',
              fontSize: 11,
              borderRadius: '50%',
              background: 'var(--panel)',
              border: '1px solid var(--border)',
              color: 'var(--text-2)',
              cursor: 'pointer',
              zIndex: 2,
            }}
          >
            {collapsed.has(node.id) ? node.children?.length : '−'}
          </span>
        )}
      </div>
    )
  }

  return (
    <div ref={containerRef} style={{ position: 'relative', height: '100%', overflow: 'hidden', background: 'var(--bg-base)' }}>
      {hint && (
        <Alert
          type="success"
          showIcon
          closable
          onClose={() => setHint(false)}
          message={t('plan.mm.hint', '1.创建测试点进行业务分类测试;2.选择测试点关联用例')}
          action={
            <a
              style={{ fontSize: 12, color: 'var(--brand)' }}
              onClick={() => {
                localStorage.setItem(HINT_KEY, '1')
                setHint(false)
              }}
            >
              {t('plan.mm.dontRemind', '不再提醒')}
            </a>
          }
          style={{ position: 'absolute', top: 8, left: 8, right: 8, zIndex: 4 }}
        />
      )}
      {/* Canvas toolbar: collapse-all / fullscreen / save (⌘+S). */}
      <div style={{ position: 'absolute', top: hint ? 52 : 8, right: 8, zIndex: 4, display: 'flex', gap: 6 }}>
        <Tooltip title={allCollapsed ? t('plan.mm.expandAll', '展开全部') : t('plan.mm.collapseAll', '收起全部')}>
          <Button size="small" icon={allCollapsed ? <PlusSquareOutlined /> : <MinusSquareOutlined />} onClick={collapseAll} />
        </Tooltip>
        <Tooltip title={t('plan.mm.fullscreen', '全屏')}>
          <Button size="small" icon={<ExpandOutlined />} onClick={() => containerRef.current?.requestFullscreen?.()} />
        </Tooltip>
        <Button size="small" type="primary" icon={<SaveOutlined />} loading={saving} onClick={save}>
          {t('plan.mm.save', '保存')}
          {dirty ? ' *' : ''}
        </Button>
      </div>
      {/* Pan/zoom canvas. */}
      <div style={{ position: 'absolute', inset: 0, cursor: 'grab' }} onMouseDown={onMouseDown} onWheel={onWheel}>
        <div
          style={{
            position: 'absolute',
            left: view.x,
            top: view.y,
            transform: `scale(${view.z})`,
            transformOrigin: '0 0',
            width,
            height: layout.height,
          }}
        >
          <svg width={width} height={layout.height + PAD} style={{ position: 'absolute', left: 0, top: 0, overflow: 'visible', pointerEvents: 'none' }}>
            {layout.edges.map(({ from, to }) => {
              const a = layout.pos.get(from)
              const b = layout.pos.get(to)
              if (!a || !b) return null
              const x1 = a.x + NODE_W
              const y1 = a.y + NODE_H / 2
              const x2 = b.x
              const y2 = b.y + NODE_H / 2
              const mx = (x1 + x2) / 2
              return (
                <path
                  key={`${from}-${to}`}
                  d={`M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`}
                  fill="none"
                  stroke="var(--border)"
                  strokeWidth={1.5}
                />
              )
            })}
          </svg>
          {flat.map(({ node }) => renderNode(node))}
        </div>
      </div>
      {/* Right slide-in node config panel. */}
      {selected && (
        <PlanNodeConfig
          node={selected}
          projectId={projectId}
          envs={envs}
          pools={pools}
          onClose={() => setSelectedId('')}
          onSave={({ config, caseIds, scenarioIds, names: picked }) => {
            patchNode(selected.id, { config, caseIds, scenarioIds })
            // Split picked names into case/scenario maps for the backend link sync.
            setNames((prev) => {
              const next = { ...prev }
              caseIds.forEach((id) => {
                if (picked[id]) next[id] = picked[id]
              })
              return next
            })
            setScenarioNames((prev) => {
              const next = { ...prev }
              scenarioIds.forEach((id) => {
                if (picked[id]) next[id] = picked[id]
              })
              return next
            })
            setSelectedId('')
          }}
        />
      )}
    </div>
  )
}
