// GitHub 风格贡献格子(纯 SVG,无依赖):近一年日历格,列=周(周日起),行=周日~周六。
// 颜色深浅 = 当日数量分级(4 级);悬浮显示日期与 AI/人工 明细。
// 零值格用 CSS 变量(--border-soft)兜底,亮暗主题都自然。
import { useMemo, useState } from 'react'
import { useI18n } from '../i18n'

export interface DayCell {
  date: string // YYYY-MM-DD
  ai: number
  human: number
}

const CELL = 11
const GAP = 3
const WEEKS = 53
const PAD_L = 28 // 周标签
const PAD_T = 16 // 月标签

// 值 → 透明度分级;基色由 metric 决定(AI/全部=品牌蓝,人工=橙)。
const LEVELS = [0.16, 0.34, 0.55, 0.8]

export default function ContributionGrid({
  days,
  metric,
}: {
  days: DayCell[]
  /** total = AI+人工;ai / human 单看一方。 */
  metric: 'total' | 'ai' | 'human'
}) {
  const { t } = useI18n()
  const [hover, setHover] = useState<{ cell: DayCell; x: number; y: number } | null>(null)

  const byDate = useMemo(() => new Map(days.map((d) => [d.date, d])), [days])
  const valueOf = (c: DayCell) => (metric === 'ai' ? c.ai : metric === 'human' ? c.human : c.ai + c.human)
  const base = metric === 'human' ? '255, 125, 0' : '22, 100, 255'

  // 网格:从「今天所在周的周日」往前推 52 周;逐列(周)逐行(星期)生成。
  const { cols, monthMarks, max } = useMemo(() => {
    const today = new Date()
    const end = new Date(Date.UTC(today.getFullYear(), today.getMonth(), today.getDate()))
    const endSunday = new Date(end)
    endSunday.setUTCDate(end.getUTCDate() - end.getUTCDay())
    const cols: { date: Date; cell: DayCell | undefined }[][] = []
    const monthMarks: { col: number; label: string }[] = []
    let lastMonth = -1
    let max = 0
    for (let w = WEEKS - 1; w >= 0; w--) {
      const col: { date: Date; cell: DayCell | undefined }[] = []
      for (let d = 0; d < 7; d++) {
        const dt = new Date(endSunday)
        dt.setUTCDate(endSunday.getUTCDate() - w * 7 + d)
        if (dt > end) continue
        const key = dt.toISOString().slice(0, 10)
        const cell = byDate.get(key)
        if (cell) max = Math.max(max, metric === 'ai' ? cell.ai : metric === 'human' ? cell.human : cell.ai + cell.human)
        col.push({ date: dt, cell })
      }
      const colIdx = WEEKS - 1 - w
      const m = col[0]?.date.getUTCMonth() ?? -1
      if (m !== lastMonth && col[0]) {
        monthMarks.push({ col: colIdx, label: `${m + 1}${t('grid.monthSuffix', '月')}` })
        lastMonth = m
      }
      cols.push(col)
    }
    return { cols, monthMarks, max }
  }, [byDate, metric, t])

  const fillOf = (cell: DayCell | undefined) => {
    const v = cell ? valueOf(cell) : 0
    if (v <= 0 || max <= 0) return 'var(--border-soft)'
    const lv = Math.min(LEVELS.length - 1, Math.floor(((v - 1) / max) * LEVELS.length))
    return `rgba(${base}, ${LEVELS[lv]})`
  }

  const W = PAD_L + WEEKS * (CELL + GAP)
  const H = PAD_T + 7 * (CELL + GAP)
  const weekdayLabels: [number, string][] = [
    [1, t('grid.mon', '一')],
    [3, t('grid.wed', '三')],
    [5, t('grid.fri', '五')],
  ]

  return (
    <div style={{ overflowX: 'auto', position: 'relative' }}>
      <svg width={W} height={H} style={{ display: 'block' }}>
        {monthMarks.map((m) => (
          <text key={m.col} x={PAD_L + m.col * (CELL + GAP)} y={10} fontSize={10} fill="var(--text-3)">
            {m.label}
          </text>
        ))}
        {weekdayLabels.map(([row, label]) => (
          <text key={row} x={0} y={PAD_T + row * (CELL + GAP) + CELL - 2} fontSize={10} fill="var(--text-3)">
            {label}
          </text>
        ))}
        {cols.map((col, ci) =>
          col.map((c, ri) => (
            <rect
              key={`${ci}-${ri}`}
              x={PAD_L + ci * (CELL + GAP)}
              y={PAD_T + c.date.getUTCDay() * (CELL + GAP)}
              width={CELL}
              height={CELL}
              rx={2}
              fill={fillOf(c.cell)}
              onMouseEnter={() =>
                setHover({
                  cell: c.cell ?? { date: c.date.toISOString().slice(0, 10), ai: 0, human: 0 },
                  x: PAD_L + ci * (CELL + GAP) + CELL / 2,
                  y: PAD_T + c.date.getUTCDay() * (CELL + GAP),
                })
              }
              onMouseLeave={() => setHover(null)}
            />
          )),
        )}
      </svg>
      {/* 图例:Less → More */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 3, fontSize: 11, color: 'var(--text-3)', marginTop: 4, justifyContent: 'flex-end' }}>
        {t('grid.less', '少')}
        <span style={{ width: CELL, height: CELL, borderRadius: 2, background: 'var(--border-soft)' }} />
        {LEVELS.map((a) => (
          <span key={a} style={{ width: CELL, height: CELL, borderRadius: 2, background: `rgba(${base}, ${a})` }} />
        ))}
        {t('grid.more', '多')}
      </div>
      {hover && (
        <div
          style={{
            position: 'absolute',
            left: hover.x,
            top: Math.max(0, hover.y - 8),
            transform: 'translate(-50%, -100%)',
            background: 'var(--panel)',
            border: '1px solid var(--border)',
            borderRadius: 6,
            boxShadow: '0 4px 12px rgba(0, 0, 0, 0.10)',
            padding: '6px 10px',
            fontSize: 12,
            whiteSpace: 'nowrap',
            pointerEvents: 'none',
            zIndex: 1,
          }}
        >
          <div style={{ color: 'var(--text-2)', marginBottom: 2 }}>{hover.cell.date}</div>
          <div style={{ color: 'var(--text)' }}>
            AI <b>{hover.cell.ai}</b> · {t('grid.human', '人工')} <b>{hover.cell.human}</b>
          </div>
        </div>
      )}
    </div>
  )
}
