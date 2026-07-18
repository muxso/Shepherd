import { useMemo, useRef, useState } from 'react'
import { Button, Input, Tooltip } from 'antd'
import { CaretRightOutlined, DeleteOutlined, HolderOutlined, SearchOutlined } from '@ant-design/icons'
import ResizableDrawer from './ResizableDrawer'
import { useI18n } from '../i18n'

export type CardSize = 'half' | 'full'
export interface CardLayoutItem {
  key: string
  size: CardSize
}

/** Default card sizes: dense chart cards take a full row, stat/entry cards half. */
export const CARD_DEFAULT_SIZE: Record<string, CardSize> = {
  assets: 'full',
  collab: 'full',
  projectBars: 'full',
  execTrend: 'full',
  apiStats: 'half',
  caseStats: 'half',
  quality: 'half',
  shortcuts: 'half',
  planOverview: 'full',
  apiChanges: 'full',
}

type ThumbKind = 'donut' | 'bars' | 'trend' | 'grid' | 'stat' | 'list'

/** Library-item thumbnail: 96x60 sketch, colored entirely via CSS vars so it adapts to dark/light. */
function Thumb({ kind }: { kind: ThumbKind }) {
  const body = () => {
    switch (kind) {
      case 'donut':
        return (
          <>
            <circle cx={26} cy={30} r={13} fill="none" stroke="var(--border)" strokeWidth={7} />
            <circle cx={26} cy={30} r={13} fill="none" stroke="var(--brand)" strokeWidth={7} strokeDasharray="49 82" transform="rotate(-90 26 30)" />
            <rect x={50} y={17} width={30} height={5} rx={2.5} fill="var(--text-3)" opacity={0.55} />
            <rect x={50} y={27} width={24} height={5} rx={2.5} fill="var(--text-3)" opacity={0.35} />
            <rect x={50} y={37} width={27} height={5} rx={2.5} fill="var(--text-3)" opacity={0.35} />
          </>
        )
      case 'bars':
        return (
          <>
            <line x1={12} y1={48} x2={84} y2={48} stroke="var(--border)" />
            <rect x={16} y={26} width={7} height={22} rx={1.5} fill="var(--brand)" opacity={0.85} />
            <rect x={25} y={34} width={7} height={14} rx={1.5} fill="var(--accent)" opacity={0.85} />
            <rect x={42} y={18} width={7} height={30} rx={1.5} fill="var(--brand)" opacity={0.85} />
            <rect x={51} y={28} width={7} height={20} rx={1.5} fill="var(--accent)" opacity={0.85} />
            <rect x={68} y={32} width={7} height={16} rx={1.5} fill="var(--brand)" opacity={0.85} />
            <rect x={77} y={22} width={7} height={26} rx={1.5} fill="var(--accent)" opacity={0.85} />
          </>
        )
      case 'trend':
        return (
          <>
            <line x1={12} y1={48} x2={84} y2={48} stroke="var(--border)" />
            {[8, 14, 12, 20, 24, 30].map((h, i) => (
              <rect key={i} x={16 + i * 12} y={48 - h} width={8} height={h} rx={1.5} fill="var(--brand)" opacity={0.3 + i * 0.11} />
            ))}
          </>
        )
      case 'grid':
        return (
          <>
            {Array.from({ length: 4 }, (_, r) =>
              Array.from({ length: 6 }, (_, c) => (
                <rect key={`${r}-${c}`} x={14 + c * 12} y={8 + r * 12} width={9} height={9} rx={2} fill="var(--brand)" opacity={[0.12, 0.3, 0.55, 0.85][(r * 7 + c * 3) % 4]} />
              )),
            )}
          </>
        )
      case 'stat':
        return (
          <>
            <rect x={12} y={13} width={26} height={10} rx={2} fill="var(--text-2)" opacity={0.7} />
            <rect x={12} y={29} width={20} height={4} rx={2} fill="var(--text-3)" opacity={0.4} />
            <rect x={54} y={13} width={26} height={10} rx={2} fill="var(--brand)" opacity={0.75} />
            <rect x={54} y={29} width={20} height={4} rx={2} fill="var(--text-3)" opacity={0.4} />
            <line x1={12} y1={42} x2={84} y2={42} stroke="var(--border)" />
            <rect x={12} y={47} width={72} height={4} rx={2} fill="var(--border)" />
          </>
        )
      case 'list':
        return (
          <>
            {[12, 26, 40].map((y, i) => (
              <g key={y}>
                <circle cx={18} cy={y + 4} r={4} fill="var(--brand)" opacity={0.55} />
                <rect x={28} y={y + 1.5} width={[46, 36, 52][i]} height={5} rx={2.5} fill="var(--text-3)" opacity={0.35} />
              </g>
            ))}
          </>
        )
    }
  }
  return (
    <svg width={96} height={60} viewBox="0 0 96 60" style={{ display: 'block', flex: '0 0 auto', borderRadius: 6, background: 'var(--panel-2)', border: '1px solid var(--border-soft)' }}>
      {body()}
    </svg>
  )
}

interface CardMeta {
  key: string
  title: string
  desc: string
  thumb: ThumbKind
}

type DragSrc = { from: 'lib'; key: string } | { from: 'layout'; index: number }

interface Props {
  layout: CardLayoutItem[]
  onSave: (next: CardLayoutItem[]) => void
  onExit: () => void
}

/**
 * Card layout editor in a right drawer: card library (groups + search) on the left, current layout on the right.
 * Native HTML5 DnD: drag left→right to add, drag between blocks to reorder. Changes stay in a draft;
 * only "Save" persists via callback.
 */
export default function CardSettings({ layout, onSave, onExit }: Props) {
  const { t } = useI18n()
  const [draft, setDraft] = useState<CardLayoutItem[]>(() => layout.map((x) => ({ ...x })))
  const [q, setQ] = useState('')
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})
  const dragSrc = useRef<DragSrc | null>(null)
  const [dragging, setDragging] = useState<string | null>(null)
  const [insertAt, setInsertAt] = useState<number | null>(null)

  const groups = useMemo(
    () => [
      {
        key: 'overview',
        title: t('home.cs.group.overview', '概览'),
        cards: [
          { key: 'assets', title: t('home.assetDist', '测试资产分布'), desc: t('home.cs.desc.assets', '各类测试资产的数量与占比'), thumb: 'donut' },
          { key: 'shortcuts', title: t('home.shortcuts', '快捷入口'), desc: t('home.cs.desc.shortcuts', '常用功能模块的快速入口'), thumb: 'list' },
        ] as CardMeta[],
      },
      {
        key: 'project',
        title: t('home.cs.group.project', '项目'),
        cards: [{ key: 'projectBars', title: t('home.projectCompare', '项目资产对比'), desc: t('home.cs.desc.projectBars', '各项目四类测试资产的数量对比'), thumb: 'bars' }] as CardMeta[],
      },
      {
        key: 'api',
        title: t('home.cs.group.api', '接口测试'),
        cards: [
          { key: 'apiStats', title: t('home.apiStats', '接口数'), desc: t('home.cs.desc.apiStats', '接口协议分布与用例覆盖率'), thumb: 'donut' },
          { key: 'caseStats', title: t('home.caseStats', '接口用例数'), desc: t('home.cs.desc.caseStats', '接口用例的执行率与通过率'), thumb: 'stat' },
          { key: 'execTrend', title: t('home.execTrend', '执行趋势'), desc: t('home.cs.desc.execTrend', '近 7 天用例执行与通过情况'), thumb: 'trend' },
          { key: 'apiChanges', title: t('home.apiChanges', '接口变更'), desc: t('home.cs.desc.apiChanges', '最近更新的接口及其关联用例与场景'), thumb: 'list' },
        ] as CardMeta[],
      },
      {
        key: 'quality',
        title: t('home.cs.group.quality', '质量'),
        cards: [
          { key: 'quality', title: t('home.quality', '质量概览'), desc: t('home.cs.desc.quality', '需求、缺陷与测试计划概况'), thumb: 'stat' },
          { key: 'planOverview', title: t('home.planOverview', '测试计划概览'), desc: t('home.cs.desc.planOverview', '单个测试计划的执行进度与关联资产'), thumb: 'bars' },
        ] as CardMeta[],
      },
      {
        key: 'collab',
        title: t('home.cs.group.collab', '人机协同'),
        cards: [{ key: 'collab', title: t('home.collab', '人机协同人效'), desc: t('home.cs.desc.collab', 'AI 与人工交付的任务量和工作量拆分'), thumb: 'grid' }] as CardMeta[],
      },
    ],
    [t],
  )
  const metaByKey = useMemo(() => {
    const m = new Map<string, CardMeta>()
    groups.forEach((g) => g.cards.forEach((c) => m.set(c.key, c)))
    return m
  }, [groups])

  const cleanup = () => {
    dragSrc.current = null
    setDragging(null)
    setInsertAt(null)
  }

  /** Drop: library items insert at the target position (default size), layout blocks move there. */
  const commit = () => {
    const src = dragSrc.current
    const pos = insertAt
    cleanup()
    if (!src || pos == null) return
    setDraft((prev) => {
      const next = prev.map((x) => ({ ...x }))
      if (src.from === 'lib') {
        if (next.some((x) => x.key === src.key)) return prev
        next.splice(pos, 0, { key: src.key, size: CARD_DEFAULT_SIZE[src.key] ?? 'half' })
        return next
      }
      const [it] = next.splice(src.index, 1)
      next.splice(pos > src.index ? pos - 1 : pos, 0, it)
      return next
    })
  }

  const resize = (i: number, size: CardSize) => setDraft((prev) => prev.map((x, j) => (j === i ? { ...x, size } : x)))
  const removeAt = (i: number) => setDraft((prev) => prev.filter((_, j) => j !== i))

  const kw = q.trim().toLowerCase()

  return (
    <ResizableDrawer
      open
      onClose={onExit}
      width="72%"
      title={t('home.cs.title', '卡片设置')}
      extra={
        <div style={{ display: 'flex', gap: 8 }}>
          <Button onClick={onExit}>{t('home.cs.exit', '退出编辑')}</Button>
          <Button type="primary" onClick={() => onSave(draft)}>{t('home.cs.save', '保存')}</Button>
        </div>
      }
      styles={{ body: { padding: 0, display: 'flex', minHeight: 0 } }}
    >
      {/* Left card library: search + collapsible groups */}
      <aside style={{ flex: '0 0 300px', width: 300, borderRight: '1px solid var(--border)', background: 'var(--panel)', overflow: 'auto', padding: 12 }}>
        <Input
          allowClear
          prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          placeholder={t('home.cs.search', '卡片搜索')}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          style={{ marginBottom: 12 }}
        />
        {groups.map((g) => {
          const cards = kw ? g.cards.filter((c) => c.title.toLowerCase().includes(kw)) : g.cards
          if (kw && cards.length === 0) return null
          const open = kw ? true : !collapsed[g.key]
          return (
            <div key={g.key} style={{ marginBottom: 4 }}>
              <div
                onClick={() => setCollapsed((s) => ({ ...s, [g.key]: !s[g.key] }))}
                style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer', padding: '6px 4px', fontSize: 12, fontWeight: 600, color: 'var(--text-2)', userSelect: 'none' }}
              >
                <CaretRightOutlined style={{ fontSize: 10, color: 'var(--text-3)', transform: open ? 'rotate(90deg)' : undefined, transition: 'transform .15s' }} />
                {g.title}
              </div>
              {open &&
                cards.map((c) => {
                  const used = draft.some((d) => d.key === c.key)
                  return (
                    <div
                      key={c.key}
                      className={used ? undefined : 'ms-hover-card'}
                      draggable={!used}
                      onDragStart={
                        used
                          ? undefined
                          : (e) => {
                              dragSrc.current = { from: 'lib', key: c.key }
                              setDragging(c.key)
                              e.dataTransfer.setData('text/plain', c.key)
                              e.dataTransfer.effectAllowed = 'copyMove'
                            }
                      }
                      onDragEnd={cleanup}
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: 10,
                        padding: 8,
                        marginBottom: 8,
                        borderRadius: 8,
                        border: '1px solid var(--border-soft)',
                        background: 'var(--panel)',
                        cursor: used ? 'not-allowed' : 'grab',
                        opacity: used ? 0.45 : dragging === c.key ? 0.4 : 1,
                      }}
                    >
                      <Thumb kind={c.thumb} />
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>
                          <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{c.title}</span>
                          {used && (
                            <span style={{ flex: '0 0 auto', fontSize: 11, fontWeight: 400, color: 'var(--text-3)', border: '1px solid var(--border)', borderRadius: 4, padding: '0 4px' }}>
                              {t('home.cs.added', '已添加')}
                            </span>
                          )}
                        </div>
                        <div style={{ fontSize: 12, color: 'var(--text-3)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{c.desc}</div>
                      </div>
                    </div>
                  )
                })}
            </div>
          )
        })}
      </aside>

      {/* Right layout area: placeholder blocks, half = two per row, full = whole row; drag to reorder/add */}
      <main
        onDragOver={(e) => {
          e.preventDefault()
          if (dragSrc.current) setInsertAt(draft.length)
        }}
        onDrop={(e) => {
          e.preventDefault()
          commit()
        }}
        style={{ flex: 1, minWidth: 0, overflow: 'auto', padding: 20 }}
      >
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 12, alignContent: 'flex-start', minHeight: '100%' }}>
          {draft.map((item, i) => {
            const meta = metaByKey.get(item.key)
            const showBefore = dragging != null && insertAt === i
            const showAfter = dragging != null && insertAt === i + 1 && i === draft.length - 1
            const indicator =
              item.size === 'half'
                ? showBefore
                  ? '-2px 0 0 var(--brand)'
                  : showAfter
                    ? '2px 0 0 var(--brand)'
                    : undefined
                : showBefore
                  ? '0 -2px 0 var(--brand)'
                  : showAfter
                    ? '0 2px 0 var(--brand)'
                    : undefined
            return (
              <div
                key={item.key}
                draggable
                onDragStart={(e) => {
                  dragSrc.current = { from: 'layout', index: i }
                  setDragging(item.key)
                  e.dataTransfer.setData('text/plain', item.key)
                  e.dataTransfer.effectAllowed = 'move'
                }}
                onDragEnd={cleanup}
                onDragOver={(e) => {
                  e.preventDefault()
                  e.stopPropagation()
                  if (!dragSrc.current) return
                  const r = e.currentTarget.getBoundingClientRect()
                  const before = item.size === 'half' ? e.clientX < r.left + r.width / 2 : e.clientY < r.top + r.height / 2
                  setInsertAt(before ? i : i + 1)
                }}
                onDrop={(e) => {
                  e.preventDefault()
                  e.stopPropagation()
                  commit()
                }}
                style={{
                  width: item.size === 'half' ? 'calc(50% - 6px)' : '100%',
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  padding: '12px 14px',
                  borderRadius: 8,
                  border: '1px solid var(--border)',
                  background: 'var(--panel)',
                  cursor: 'grab',
                  opacity: dragging === item.key ? 0.4 : 1,
                  boxShadow: indicator,
                }}
              >
                <HolderOutlined style={{ color: 'var(--text-3)' }} />
                <span style={{ flex: 1, minWidth: 0, fontSize: 13, fontWeight: 600, color: 'var(--text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {meta?.title ?? item.key}
                </span>
                {/* half/full pill toggle */}
                <span style={{ flex: '0 0 auto', display: 'inline-flex', border: '1px solid var(--border)', borderRadius: 999, overflow: 'hidden' }}>
                  {(['half', 'full'] as CardSize[]).map((s) => (
                    <span
                      key={s}
                      onClick={() => resize(i, s)}
                      style={{
                        padding: '2px 10px',
                        fontSize: 12,
                        cursor: 'pointer',
                        userSelect: 'none',
                        background: item.size === s ? 'var(--brand-soft)' : 'transparent',
                        color: item.size === s ? 'var(--brand)' : 'var(--text-3)',
                      }}
                    >
                      {s === 'half' ? t('home.cs.half', '半屏') : t('home.cs.full', '全屏')}
                    </span>
                  ))}
                </span>
                <Tooltip title={t('home.cs.remove', '移除')}>
                  <Button type="text" size="small" icon={<DeleteOutlined />} onClick={() => removeAt(i)} />
                </Tooltip>
              </div>
            )
          })}
          {draft.length === 0 && (
            <div
              style={{
                width: '100%',
                padding: '48px 0',
                textAlign: 'center',
                fontSize: 13,
                color: 'var(--text-3)',
                border: '1px dashed var(--border)',
                borderRadius: 8,
                background: dragging != null && insertAt != null ? 'var(--brand-soft)' : undefined,
              }}
            >
              {t('home.cs.emptyLayout', '从左侧拖入卡片')}
            </div>
          )}
        </div>
      </main>
    </ResizableDrawer>
  )
}
