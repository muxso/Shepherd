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

export function outcomeColor(o: string): string {
  const v = o.toUpperCase()
  if (v.includes('PASS') || v === 'SUCCESS' || v === 'OK') return 'green'
  if (v.includes('FAIL') || v.includes('ERROR')) return 'red'
  return 'default'
}
