import { useMemo } from 'react'
import { Checkbox, Table } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { useI18n } from '../i18n'

// 权限矩阵(项目与权限 → 用户组抽屉):模块(rowSpan 分组)/ 功能 / 权限(动作 Checkbox)+ 行全选 + 表头总全选。
// 权限串格式与 Role.permissions 一致:"RESOURCE:ACT+ACT"(如 "PROJECT:READ+UPDATE")。

interface PermResource {
  res: string
  label: string
  actions: string[]
}

interface PermGroup {
  key: string
  label: string
  resources: PermResource[]
}

// 资源 → 动作全集(动作代码与后端权限串一致;RUNNER 用 EDIT,其余编辑用 UPDATE)。
const CATALOG: PermGroup[] = [
  {
    key: 'project', label: '项目',
    resources: [
      { res: 'PROJECT', label: '项目', actions: ['READ', 'ADD', 'UPDATE'] },
      { res: 'ORGANIZATION', label: '组织', actions: ['READ', 'ADD', 'UPDATE', 'DELETE'] },
      { res: 'SYSTEM_USER', label: '用户', actions: ['READ', 'ADD', 'UPDATE', 'DELETE'] },
      { res: 'USER_ROLE', label: '用户组', actions: ['READ', 'ADD', 'UPDATE', 'DELETE'] },
      { res: 'ENVIRONMENT', label: '环境', actions: ['READ', 'ADD', 'UPDATE', 'DELETE'] },
      { res: 'APIKEY', label: 'APIKEY', actions: ['READ', 'ADD', 'DELETE'] },
      { res: 'RESOURCE_POOL', label: '资源池', actions: ['READ', 'ADD', 'UPDATE', 'DELETE'] },
      { res: 'RUNNER', label: '执行器', actions: ['READ', 'ADD', 'EDIT', 'EXECUTE'] },
      { res: 'SKILL', label: '技能', actions: ['READ', 'ADD', 'UPDATE', 'DELETE'] },
    ],
  },
  {
    key: 'requirement', label: '需求',
    resources: [
      { res: 'REQUIREMENT', label: '需求', actions: ['READ', 'ADD', 'UPDATE', 'DELETE'] },
      { res: 'DELIVERY', label: '交付', actions: ['READ', 'UPDATE', 'EXECUTE'] },
      { res: 'TASK', label: '任务', actions: ['READ', 'ADD', 'UPDATE', 'EXECUTE'] },
      { res: 'VERIFICATION', label: '验收', actions: ['READ', 'ADD', 'UPDATE'] },
      { res: 'CASE_REVIEW', label: '评审', actions: ['READ', 'REVIEW'] },
      { res: 'COMMENT', label: '评论', actions: ['READ', 'ADD', 'DELETE'] },
    ],
  },
  {
    key: 'test', label: '测试',
    resources: [
      { res: 'FUNCTIONAL_CASE', label: '功能用例', actions: ['READ', 'ADD', 'UPDATE', 'DELETE'] },
      { res: 'API_DEFINITION', label: '接口定义', actions: ['READ', 'ADD', 'UPDATE', 'EXECUTE'] },
      { res: 'API_SCENARIO', label: '场景', actions: ['READ', 'ADD', 'UPDATE', 'EXECUTE'] },
      { res: 'TEST_PLAN', label: '测试计划', actions: ['READ', 'ADD', 'UPDATE', 'EXECUTE'] },
      { res: 'PERF', label: '性能', actions: ['EXECUTE'] },
      { res: 'BUG', label: '缺陷', actions: ['READ', 'ADD', 'UPDATE'] },
    ],
  },
]

const ACT_LABEL: Record<string, string> = {
  READ: '查询', ADD: '创建', UPDATE: '编辑', EDIT: '编辑', DELETE: '删除', EXECUTE: '执行', REVIEW: '评审',
}

// UPDATE/EDIT 同义(都显示「编辑」):解析时归一到目录里登记的那个代码。
const EDIT_ALIAS: Record<string, string> = { UPDATE: 'EDIT', EDIT: 'UPDATE' }

const RES_INDEX = new Map<string, PermResource>(CATALOG.flatMap((g) => g.resources).map((r) => [r.res, r]))

/** 解析 Role.permissions → 勾选集(键 "RES:ACT")+ 目录外无法呈现的原串(保存时原样带回,避免丢权限)。 */
export function parsePermissions(perms: string[] | undefined): { checked: Set<string>; extras: string[] } {
  const checked = new Set<string>()
  const extras: string[] = []
  for (const p of perms ?? []) {
    const idx = p.indexOf(':')
    const res = idx >= 0 ? p.slice(0, idx) : p
    const acts = (idx >= 0 ? p.slice(idx + 1) : '').split('+').filter(Boolean)
    const def = RES_INDEX.get(res)
    if (!def) { extras.push(p); continue }
    const unknown: string[] = []
    for (const a of acts) {
      if (def.actions.includes(a)) checked.add(`${res}:${a}`)
      else if (def.actions.includes(EDIT_ALIAS[a] ?? '')) checked.add(`${res}:${EDIT_ALIAS[a]}`)
      else unknown.push(a)
    }
    if (unknown.length) extras.push(`${res}:${unknown.join('+')}`)
  }
  return { checked, extras }
}

/** 勾选集 → Role.permissions 串(只输出勾了动作的资源;extras 原样附加)。 */
export function serializePermissions(checked: Set<string>, extras: string[] = []): string[] {
  const out: string[] = []
  for (const g of CATALOG) {
    for (const r of g.resources) {
      const acts = r.actions.filter((a) => checked.has(`${r.res}:${a}`))
      if (acts.length) out.push(`${r.res}:${acts.join('+')}`)
    }
  }
  return [...out, ...extras]
}

interface MatrixRow {
  key: string
  groupLabel: string
  groupSpan: number
  res: string
  resLabel: string
  actions: string[]
}

export default function PermissionMatrix({ checked, onChange, disabled }: {
  checked: Set<string>
  onChange: (next: Set<string>) => void
  disabled?: boolean
}) {
  const { t } = useI18n()

  const rows: MatrixRow[] = useMemo(() => CATALOG.flatMap((g) => g.resources.map((r, i) => ({
    key: r.res,
    groupLabel: t(`pa.grp.${g.key}`, g.label),
    groupSpan: i === 0 ? g.resources.length : 0,
    res: r.res,
    resLabel: t(`pa.res.${r.res}`, r.label),
    actions: r.actions,
  }))), [t])

  const allKeys = useMemo(() => rows.flatMap((r) => r.actions.map((a) => `${r.res}:${a}`)), [rows])
  const checkedCount = allKeys.filter((k) => checked.has(k)).length
  const allChecked = checkedCount === allKeys.length && allKeys.length > 0
  const someChecked = checkedCount > 0 && !allChecked

  const set = (keys: string[], on: boolean) => {
    const next = new Set(checked)
    keys.forEach((k) => (on ? next.add(k) : next.delete(k)))
    onChange(next)
  }

  const cols: ColumnsType<MatrixRow> = [
    {
      title: t('pa.colModule', '模块'), dataIndex: 'groupLabel', width: 110,
      onCell: (r) => ({ rowSpan: r.groupSpan }),
      render: (v: string) => <span style={{ fontWeight: 600, color: 'var(--text)' }}>{v}</span>,
    },
    { title: t('pa.colFeature', '功能'), dataIndex: 'resLabel', width: 150 },
    {
      title: t('pa.colPermission', '权限'),
      render: (_v, r) => (
        <div style={{ display: 'flex', flexWrap: 'wrap', columnGap: 24, rowGap: 4 }}>
          {r.actions.map((a) => (
            <Checkbox
              key={a}
              disabled={disabled}
              checked={checked.has(`${r.res}:${a}`)}
              onChange={(e) => set([`${r.res}:${a}`], e.target.checked)}
            >
              {t(`pa.act.${a}`, ACT_LABEL[a] || a)}
            </Checkbox>
          ))}
        </div>
      ),
    },
    {
      // 最右列:本行全选;表头为总开关「全选」。
      title: (
        <Checkbox
          disabled={disabled}
          checked={allChecked}
          indeterminate={someChecked}
          onChange={(e) => set(allKeys, e.target.checked)}
        >
          {t('pa.selectAll', '全选')}
        </Checkbox>
      ),
      width: 100,
      render: (_v, r) => {
        const keys = r.actions.map((a) => `${r.res}:${a}`)
        const n = keys.filter((k) => checked.has(k)).length
        return (
          <Checkbox
            disabled={disabled}
            checked={n === keys.length}
            indeterminate={n > 0 && n < keys.length}
            onChange={(e) => set(keys, e.target.checked)}
          />
        )
      },
    },
  ]

  return <Table<MatrixRow> rowKey="key" size="small" bordered pagination={false} dataSource={rows} columns={cols} />
}
