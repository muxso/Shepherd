import { useEffect, useMemo, useRef, useState } from 'react'
import { Alert, Button, Input, Modal, Popover, Select, Tag, Tooltip } from 'antd'
import {
  AimOutlined,
  DeleteOutlined,
  ExpandOutlined,
  MacCommandOutlined,
  MinusSquareOutlined,
  PlusCircleOutlined,
  PlusSquareOutlined,
  SaveOutlined,
  SettingOutlined,
  SwapOutlined,
  UnorderedListOutlined,
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
import PlanCategoryConfig from './PlanCategoryConfig'
import PlanCasePicker, { type PlanCatType } from './PlanCasePicker'

// Layout constants for the layered left-to-right tree.
const NODE_W = 200
const NODE_H = 52
const LEAF_H = 30
const H_GAP = 72
const V_GAP = 16
const PAD = 32

const HINT_KEY = 'shepherd.planMindmapHint'

const uid = () => `n_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`

// Planning node + additive catType marker on category nodes (doc stays backward compatible;
// the backend stores the doc verbatim, so the extra field round-trips).
type MmNode = PlanningNode & { catType?: PlanCatType }

// Default planning tree: one category per case kind, each with one default test point
// (mirrors MeterSphere). API/scenario categories default to serial execution.
function defaultNodes(t: (k: string, d: string) => string): MmNode[] {
  const point = (name: string): PlanningNode => ({
    id: uid(),
    name,
    kind: 'point',
    children: [],
    config: { inherit: true, mode: 'serial' },
    caseIds: [],
    scenarioIds: [],
  })
  return [
    {
      id: uid(),
      name: t('plan.mm.funcCases', '功能用例'),
      kind: 'category',
      catType: 'func',
      children: [point(t('plan.mm.defaultFuncPoint', '基本功能点'))],
    },
    {
      id: uid(),
      name: t('plan.mm.apiCases', '接口用例'),
      kind: 'category',
      catType: 'api',
      config: { mode: 'serial' },
      children: [point(t('plan.mm.defaultApiPoint', '单接口验证'))],
    },
    {
      id: uid(),
      name: t('plan.mm.scenarioCases', '场景用例'),
      kind: 'category',
      catType: 'scenario',
      config: { mode: 'serial' },
      children: [point(t('plan.mm.defaultScenarioPoint', '业务流程验证'))],
    },
  ]
}

// Display tree node: real planning node or a virtual config leaf (case count / env / pool).
type LeafKind = 'cases' | 'env' | 'pool'
interface DispNode {
  id: string
  node?: PlanningNode
  leaf?: { kind: LeafKind; text: string; ownerId: string }
  children: DispNode[]
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
  const [menuOpen, setMenuOpen] = useState(false)
  const [catCfgId, setCatCfgId] = useState('')
  const [pickerId, setPickerId] = useState('')
  const [envEditId, setEnvEditId] = useState('')
  const [poolEditId, setPoolEditId] = useState('')
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

  // Fallback to a raw id slice when the env/pool list has no match.
  const envName = (id?: string) => (id ? envs.find((x) => x.id === id)?.name || id.slice(0, 8) : '')
  const poolName = (id?: string) => (id ? pools.find((x) => x.id === id)?.name || id.slice(0, 8) : '')

  // Node id -> owning category kind. New docs carry catType on the category node;
  // older docs fall back to name matching. Nodes outside a known category act as api.
  const catTypes = useMemo(() => {
    const byName = (n: MmNode): PlanCatType => {
      if (n.catType) return n.catType
      if (/功能|functional/i.test(n.name)) return 'func'
      if (/场景|scenario/i.test(n.name)) return 'scenario'
      return 'api'
    }
    const map = new Map<string, PlanCatType>()
    const walk = (n: PlanningNode, ct: PlanCatType) => {
      map.set(n.id, ct)
      ;(n.children || []).forEach((c) => walk(c, ct))
    }
    nodes.forEach((top) => walk(top, byName(top as MmNode)))
    return map
  }, [nodes])
  const catTypeOf = (id: string): PlanCatType => catTypes.get(id) || 'api'

  // Display tree: 测试点 nodes grow virtual leaves for case count / env / pool.
  // API/scenario test points always show env + pool leaves (default text when unset).
  const dispRoot = useMemo(() => {
    const build = (n: PlanningNode): DispNode => {
      const children = (n.children || []).map(build)
      if (n.kind === 'point') {
        const ct = catTypeOf(n.id)
        const count = (n.caseIds?.length || 0) + (n.scenarioIds?.length || 0)
        children.push({
          id: `${n.id}::cases`,
          leaf: { kind: 'cases', text: `${count}${t('plan.mm.casesUnit', '条')}`, ownerId: n.id },
          children: [],
        })
        if (ct !== 'func' || n.config?.envId)
          children.push({
            id: `${n.id}::env`,
            leaf: { kind: 'env', text: n.config?.envId ? envName(n.config.envId) : t('plan.mm.defaultEnv', '默认环境'), ownerId: n.id },
            children: [],
          })
        if (ct !== 'func' || n.config?.poolId)
          children.push({
            id: `${n.id}::pool`,
            leaf: { kind: 'pool', text: n.config?.poolId ? poolName(n.config.poolId) : t('plan.mm.defaultPool', '默认资源池'), ownerId: n.id },
            children: [],
          })
      }
      return { id: n.id, node: n, children }
    }
    return build(root)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, envs, pools, catTypes, t])

  // Layered tree layout: leaves stack top-to-bottom, parents center on their children.
  const layout = useMemo(() => {
    const pos = new Map<string, { x: number; y: number; h: number }>()
    const edges: { from: string; to: string }[] = []
    let cursor = 0
    let maxDepth = 0
    const place = (n: DispNode, depth: number): number => {
      maxDepth = Math.max(maxDepth, depth)
      const x = depth * (NODE_W + H_GAP)
      const h = n.leaf ? LEAF_H : NODE_H
      const kids = collapsed.has(n.id) ? [] : n.children
      if (!kids.length) {
        const y = cursor
        cursor += h + V_GAP
        pos.set(n.id, { x, y, h })
        return y + h / 2
      }
      const mids = kids.map((k) => {
        edges.push({ from: n.id, to: k.id })
        return place(k, depth + 1)
      })
      const mid = (mids[0] + mids[mids.length - 1]) / 2
      pos.set(n.id, { x, y: mid - h / 2, h })
      return mid
    }
    place(dispRoot, 0)
    return {
      pos,
      edges,
      height: Math.max(cursor, NODE_H),
      width: (maxDepth + 1) * (NODE_W + H_GAP),
    }
  }, [dispRoot, collapsed])

  const flat = useMemo(() => {
    const out: DispNode[] = []
    const walk = (n: DispNode) => {
      out.push(n)
      if (!collapsed.has(n.id)) n.children.forEach(walk)
    }
    walk(dispRoot)
    return out
  }, [dispRoot, collapsed])

  const findNode = (list: PlanningNode[], id: string): PlanningNode | null => {
    for (const n of list) {
      if (n.id === id) return n
      const hit = findNode(n.children || [], id)
      if (hit) return hit
    }
    return null
  }
  const selected = selectedId ? findNode(nodes, selectedId) : null
  const catCfgNode = catCfgId ? findNode(nodes, catCfgId) : null
  const pickerNode = pickerId ? findNode(nodes, pickerId) : null
  const envEditNode = envEditId ? findNode(nodes, envEditId) : null
  const poolEditNode = poolEditId ? findNode(nodes, poolEditId) : null

  const mutate = (fn: (list: PlanningNode[]) => PlanningNode[]) => {
    setNodes((prev) => fn(prev))
    setDirty(true)
  }
  const patchNode = (id: string, patch: Partial<PlanningNode>) => {
    const walk = (list: PlanningNode[]): PlanningNode[] =>
      list.map((n) => (n.id === id ? { ...n, ...patch } : { ...n, children: walk(n.children || []) }))
    mutate(walk)
  }
  const patchConfig = (node: PlanningNode, patch: PlanningNode['config']) =>
    patchNode(node.id, { config: { ...node.config, ...patch } })
  const toggleMode = (node: PlanningNode) =>
    patchConfig(node, { mode: node.config?.mode === 'parallel' ? 'serial' : 'parallel' })

  // Apply a picker result to a node and record display names for the link sync.
  const applyPicked = (id: string, caseIds: string[], scenarioIds: string[], picked: Record<string, string>) => {
    patchNode(id, { caseIds, scenarioIds })
    setNames((prev) => {
      const next = { ...prev }
      caseIds.forEach((cid) => {
        if (picked[cid]) next[cid] = picked[cid]
      })
      return next
    })
    setScenarioNames((prev) => {
      const next = { ...prev }
      scenarioIds.forEach((sid) => {
        if (picked[sid]) next[sid] = picked[sid]
      })
      return next
    })
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

  const renameNode = (node: PlanningNode) => {
    let name = node.name
    modal.confirm({
      title: t('plan.mm.rename', '重命名'),
      content: (
        <Input
          defaultValue={node.name}
          onChange={(e) => (name = e.target.value)}
          style={{ marginTop: 8 }}
          autoFocus
        />
      ),
      onOk: () => {
        const v = name.trim()
        if (v && v !== node.name) patchNode(node.id, { name: v })
      },
    })
  }

  const removeNode = (id: string) => {
    // Category nodes are fixed (no delete, from toolbar or keyboard).
    if (findNode(nodes, id)?.kind === 'category') return
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
    const walk = (n: DispNode) => {
      if (n.children.length) {
        if (n.id !== '__root__') ids.push(n.id)
        n.children.forEach(walk)
      }
    }
    walk(dispRoot)
    return ids
  }, [dispRoot])
  const allCollapsed = allParentIds.length > 0 && allParentIds.every((id) => collapsed.has(id))
  const collapseAll = () => setCollapsed(allCollapsed ? new Set() : new Set(allParentIds))

  // Reset pan/zoom so the root node sits vertically centered at the left edge.
  const centerView = () => {
    const el = containerRef.current
    const rootPos = layout.pos.get('__root__')
    if (!el || !rootPos) return
    setView({ x: 24, y: Math.max(24, el.clientHeight / 2 - (rootPos.y + NODE_H / 2)), z: 1 })
  }

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
  const removeRef = useRef(removeNode)
  removeRef.current = removeNode
  const selectedIdRef = useRef(selectedId)
  selectedIdRef.current = selectedId

  // Keyboard shortcuts while this tab is mounted: ⌘/Ctrl+S save, Delete removes selected node.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
        e.preventDefault()
        saveRef.current()
        return
      }
      if (e.key === 'Delete' && selectedIdRef.current) {
        const el = e.target as HTMLElement | null
        if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)) return
        removeRef.current(selectedIdRef.current)
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

  const width = PAD * 2 + layout.width

  // Small brand-colored "#" chip shown before category / test-point labels.
  const hashChip = (
    <span
      style={{
        width: 16,
        height: 16,
        lineHeight: '16px',
        textAlign: 'center',
        borderRadius: 4,
        background: 'var(--brand)',
        color: '#fff',
        fontSize: 11,
        fontWeight: 600,
        flexShrink: 0,
      }}
    >
      #
    </span>
  )

  const leafTagMeta: Record<LeafKind, { color: string; label: string }> = {
    cases: { color: 'gold', label: t('plan.mm.caseCountTag', '用例数') },
    env: { color: 'magenta', label: t('plan.mm.envTag', '环境') },
    pool: { color: 'gold', label: t('plan.mm.poolTag', '资源池') },
  }

  // Virtual config leaf: value text + a small colored category tag.
  // Click opens the matching editor: cases -> link dialog, env/pool -> select modal.
  const renderLeaf = (d: DispNode) => {
    const p = layout.pos.get(d.id)
    if (!p || !d.leaf) return null
    const meta = leafTagMeta[d.leaf.kind]
    const { kind, ownerId } = d.leaf
    return (
      <div
        key={d.id}
        data-mm-node
        onClick={() => {
          if (kind === 'cases') setPickerId(ownerId)
          else if (kind === 'env') setEnvEditId(ownerId)
          else setPoolEditId(ownerId)
        }}
        style={{
          cursor: 'pointer',
          position: 'absolute',
          left: p.x,
          top: p.y,
          height: LEAF_H,
          maxWidth: NODE_W,
          boxSizing: 'border-box',
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          padding: '0 8px',
          borderRadius: 6,
          background: 'var(--panel)',
          border: '1px solid var(--border)',
          boxShadow: '0 1px 3px rgba(0,0,0,0.06)',
        }}
      >
        <span
          style={{
            fontSize: 12,
            color: 'var(--text)',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {d.leaf.text}
        </span>
        <Tag color={meta.color} style={{ margin: 0, fontSize: 11, lineHeight: '16px', padding: '0 4px' }}>
          {meta.label}
        </Tag>
      </div>
    )
  }

  const renderNode = (d: DispNode) => {
    if (d.leaf) return renderLeaf(d)
    const node = d.node!
    const p = layout.pos.get(d.id)
    if (!p) return null
    const isRoot = node.id === '__root__'
    const cfg = node.config
    const isCat = node.kind === 'category' && !isRoot
    const ct = catTypeOf(node.id)
    // Serial/parallel badge: api/scenario subtrees only (功能用例 has no exec mode).
    const showMode = !isRoot && ct !== 'func' && !!cfg?.mode
    const hovered = hoverId === node.id
    const isSelected = selectedId === node.id
    return (
      <div
        key={node.id}
        data-mm-node
        onMouseEnter={() => setHoverId(node.id)}
        onMouseLeave={() => setHoverId((h) => (h === node.id ? '' : h))}
        onClick={() => !isRoot && setSelectedId(node.id)}
        onDoubleClick={() => !isRoot && renameNode(node)}
        style={{
          position: 'absolute',
          left: p.x,
          top: p.y,
          width: NODE_W,
          minHeight: NODE_H,
          boxSizing: 'border-box',
          padding: '6px 10px',
          display: 'flex',
          alignItems: 'center',
          borderRadius: 8,
          cursor: isRoot ? 'default' : 'pointer',
          background: isRoot ? 'var(--brand)' : 'var(--panel)',
          color: isRoot ? '#fff' : 'var(--text)',
          border: isRoot ? '1px solid var(--brand)' : `1px solid ${isSelected ? 'var(--brand)' : 'var(--border)'}`,
          boxShadow: isSelected ? '0 0 0 2px var(--brand-soft)' : '0 1px 3px rgba(0,0,0,0.06)',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, width: '100%' }}>
          {!isRoot && hashChip}
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
            {/* API/scenario categories: serial-parallel toggle + category config. 功能用例 only adds points. */}
            {isCat && ct !== 'func' && (
              <>
                <Tooltip title={t('plan.mm.toggleMode', '串行/并行')}>
                  <Button type="text" size="small" icon={<SwapOutlined />} onClick={(e) => { e.stopPropagation(); toggleMode(node) }} />
                </Tooltip>
                <Tooltip title={t('plan.mm.config', '配置')}>
                  <Button type="text" size="small" icon={<SettingOutlined />} onClick={(e) => { e.stopPropagation(); setCatCfgId(node.id) }} />
                </Tooltip>
              </>
            )}
            {!isRoot && !isCat && (
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
        {/* Collapse toggle on the right edge for nodes with children (incl. config leaves). */}
        {d.children.length > 0 && (
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
            {collapsed.has(node.id) ? d.children.length : '−'}
          </span>
        )}
      </div>
    )
  }

  const shortcutRows: [string, string][] = [
    ['⌘+S', t('plan.mm.save', '保存')],
    [t('plan.mm.dblclick', '双击'), t('plan.mm.rename', '重命名')],
    ['Delete', t('plan.mm.deleteNodeShort', '删除节点')],
  ]
  const shortcutContent = (
    <div style={{ minWidth: 150 }}>
      {shortcutRows.map(([key, desc]) => (
        <div key={key} style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 16, padding: '3px 0' }}>
          <span style={{ fontSize: 12, color: 'var(--text-2)' }}>{desc}</span>
          <span
            style={{
              fontSize: 11,
              padding: '0 6px',
              lineHeight: '18px',
              borderRadius: 4,
              border: '1px solid var(--border)',
              background: 'var(--bg-base)',
              color: 'var(--text)',
            }}
          >
            {key}
          </span>
        </div>
      ))}
    </div>
  )

  // Vertical canvas-tool strip inside the toolbar dropdown.
  const canvasMenu = (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
      <Tooltip placement="left" title={allCollapsed ? t('plan.mm.expandAll', '展开全部') : t('plan.mm.collapseAll', '收起全部')}>
        <Button
          type="text"
          size="small"
          icon={allCollapsed ? <PlusSquareOutlined /> : <MinusSquareOutlined />}
          onClick={collapseAll}
        />
      </Tooltip>
      <Popover placement="left" title={t('plan.mm.shortcuts', '快捷键')} content={shortcutContent}>
        <Button type="text" size="small" icon={<MacCommandOutlined />} />
      </Popover>
      <Tooltip placement="left" title={t('plan.mm.recenter', '回到中心')}>
        <Button
          type="text"
          size="small"
          icon={<AimOutlined />}
          onClick={() => {
            centerView()
            setMenuOpen(false)
          }}
        />
      </Tooltip>
    </div>
  )

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
      {/* Canvas toolbar: tool dropdown / fullscreen / save (⌘+S). */}
      <div style={{ position: 'absolute', top: hint ? 52 : 8, right: 8, zIndex: 4, display: 'flex', gap: 6 }}>
        <Popover
          open={menuOpen}
          onOpenChange={setMenuOpen}
          trigger="click"
          placement="bottomRight"
          arrow={false}
          styles={{ body: { padding: 4 } }}
          content={canvasMenu}
        >
          <Tooltip title={t('plan.mm.canvasMenu', '画布工具')}>
            <Button size="small" icon={<UnorderedListOutlined />} />
          </Tooltip>
        </Popover>
        <Tooltip title={t('plan.mm.fullscreen', '全屏')}>
          <Button size="small" icon={<ExpandOutlined />} onClick={() => containerRef.current?.requestFullscreen?.()} />
        </Tooltip>
        <Button size="small" type="primary" icon={<SaveOutlined />} loading={saving} onClick={save}>
          {t('plan.mm.save', '保存')} (⌘+S){dirty ? ' *' : ''}
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
              const y1 = a.y + a.h / 2
              const x2 = b.x
              const y2 = b.y + b.h / 2
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
          {flat.map((d) => renderNode(d))}
        </div>
      </div>
      {/* Right slide-in test-point config panel (categories use the category panel below). */}
      {selected && selected.kind !== 'category' && (
        <PlanNodeConfig
          node={selected}
          projectId={projectId}
          catType={catTypeOf(selected.id)}
          envs={envs}
          pools={pools}
          onClose={() => setSelectedId('')}
          onSave={({ config, caseIds, scenarioIds, names: picked }) => {
            patchNode(selected.id, { config })
            applyPicked(selected.id, caseIds, scenarioIds, picked)
            setSelectedId('')
          }}
        />
      )}
      {/* Category config panel (接口用例 / 场景用例). */}
      {catCfgNode && (
        <PlanCategoryConfig
          node={catCfgNode}
          envs={envs}
          pools={pools}
          onClose={() => setCatCfgId('')}
          onSave={(config) => {
            patchNode(catCfgNode.id, { config })
            setCatCfgId('')
          }}
        />
      )}
      {/* Link-cases dialog opened from a 用例数 leaf. */}
      {pickerNode && (
        <PlanCasePicker
          open
          projectId={projectId}
          catType={catTypeOf(pickerNode.id)}
          caseIds={pickerNode.caseIds || []}
          scenarioIds={pickerNode.scenarioIds || []}
          onClose={() => setPickerId('')}
          onOk={(c, s, picked) => {
            applyPicked(pickerNode.id, c, s, picked)
            setPickerId('')
          }}
        />
      )}
      {/* Env / pool leaf editors. */}
      {envEditNode && (
        <LeafSelectModal
          title={t('plan.mm.setEnv', '设置环境')}
          placeholder={t('plan.mm.defaultEnv', '默认环境')}
          options={envs.map((e) => ({ value: e.id, label: e.name }))}
          value={envEditNode.config?.envId}
          onCancel={() => setEnvEditId('')}
          onOk={(v) => {
            patchConfig(envEditNode, { envId: v })
            setEnvEditId('')
          }}
        />
      )}
      {poolEditNode && (
        <LeafSelectModal
          title={t('plan.mm.setPool', '设置资源池')}
          placeholder={t('plan.mm.defaultPool', '默认资源池')}
          options={pools.map((p) => ({ value: p.id, label: p.name }))}
          value={poolEditNode.config?.poolId}
          onCancel={() => setPoolEditId('')}
          onOk={(v) => {
            patchConfig(poolEditNode, { poolId: v })
            setPoolEditId('')
          }}
        />
      )}
    </div>
  )
}

/** Small select modal for the env / pool leaves. */
function LeafSelectModal({
  title,
  placeholder,
  options,
  value,
  onOk,
  onCancel,
}: {
  title: string
  placeholder: string
  options: { value: string; label: string }[]
  value?: string
  onOk: (v?: string) => void
  onCancel: () => void
}) {
  const [v, setV] = useState<string | undefined>(value)
  return (
    <Modal open title={title} width={360} onCancel={onCancel} onOk={() => onOk(v)}>
      <Select
        style={{ width: '100%', margin: '8px 0' }}
        allowClear
        placeholder={placeholder}
        value={v}
        onChange={setV}
        options={options}
      />
    </Modal>
  )
}
