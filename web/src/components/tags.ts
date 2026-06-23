// 方法/状态/结果 → AntD Tag 颜色,统一视觉。

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

/** 优先级/等级 → 颜色(P0 最高=红,依次 橙/蓝/灰)。用例与场景共用。 */
export function priorityColor(p: string): string {
  switch (p.toUpperCase()) {
    case 'P0':
      return '#ff4d4f' // 红:最高
    case 'P1':
      return '#fa8c16' // 橙
    case 'P2':
      return '#1677ff' // 蓝
    case 'P3':
      return '#52c41a' // 绿
    case 'P4':
      return '#8a9099' // 灰
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
