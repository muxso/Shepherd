// Method/status/outcome → AntD Tag color mapping + localized status labels.

type TFn = (key: string, fallback?: string) => string

/** Lifecycle status (definition/scenario: DRAFT/DEBUGGING/COMPLETED/DEPRECATED) → localized label. */
export function statusLabel(s: string, t: TFn): string {
  switch ((s || '').toUpperCase()) {
    case 'DRAFT':
      return t('status.draft', '草稿')
    case 'DEBUGGING':
      return t('status.debugging', '调试中')
    case 'COMPLETED':
      return t('status.completed', '已完成')
    case 'DEPRECATED':
      return t('status.deprecated', '已废弃')
    default:
      return s
  }
}

/** Execution status/outcome (PENDING/RUNNING/SUCCESS/ERROR...) → localized label. */
export function execStatusLabel(s: string, t: TFn): string {
  const v = (s || '').toUpperCase()
  if (v === 'PENDING') return t('status.pending', '等待中')
  if (v === 'RUNNING') return t('status.running', '执行中')
  if (v === 'SUCCESS' || v.includes('PASS') || v === 'OK') return t('status.success', '成功')
  if (v === 'ERROR' || v.includes('FAIL')) return t('status.error', '失败')
  return s
}

/** API case status (Chinese values persisted by the backend) → localized label. */
export function caseStatusLabel(s: string, t: TFn): string {
  switch (s) {
    case '进行中':
      return t('case.statusInProgress', '进行中')
    case '已完成':
      return t('case.statusCompleted', '已完成')
    case '已废弃':
      return t('case.statusDeprecated', '已废弃')
    default:
      return s
  }
}

export function methodColor(m: string): string {
  switch (m.toUpperCase()) {
    case 'GET':
      return 'green'
    case 'POST':
      return 'blue'
    case 'PUT':
      return 'orange'
    case 'DELETE':
      return 'red'
    case 'PATCH':
      return 'purple'
    case 'SCENARIO':
      return 'geekblue'
    default:
      return 'default'
  }
}

export function statusColor(s: string): string {
  switch (s.toUpperCase()) {
    case 'DRAFT':
      return 'default'
    case 'DEBUGGING':
    case 'DEBUG':
      return 'orange'
    case 'DONE':
    case 'COMPLETED':
    case 'PASSED':
      return 'green'
    case 'DEPRECATED':
      return 'red'
    default:
      return 'blue'
  }
}

/** Priority/level → color (P0 highest = red). Shared by cases and scenarios. */
export function priorityColor(p: string): string {
  switch (p.toUpperCase()) {
    case 'P0':
      return '#ff4d4f'
    case 'P1':
      return '#fa8c16'
    case 'P2':
      return '#1677ff'
    case 'P3':
      return '#52c41a'
    case 'P4':
      return '#8a9099'
    default:
      return '#8a9099'
  }
}

export function outcomeColor(o: string): string {
  const v = o.toUpperCase()
  if (v.includes('PASS') || v === 'SUCCESS' || v === 'OK') return 'green'
  if (v.includes('FAIL') || v.includes('ERROR')) return 'red'
  return 'default'
}

/** Functional case lifecycle labels (PREPARED/REVIEWING/PASS/REJECT/PENDING). */
export const funcCaseStatusLabel = (s: string | undefined, t: (k: string, f?: string) => string): string => {
  const v = (s || 'PREPARED').toUpperCase()
  const map: Record<string, string> = {
    PREPARED: t('fcase.stPrepared', '未开始'),
    PENDING: t('fcase.stPending', '待评审'),
    REVIEWING: t('fcase.stReviewing', '评审中'),
    PASS: t('fcase.stPass', '评审通过'),
    REJECT: t('fcase.stReject', '评审不通过'),
  }
  return map[v] || (s || 'PREPARED')
}
