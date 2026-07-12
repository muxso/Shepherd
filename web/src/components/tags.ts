// Method/status/outcome → AntD Tag color mapping.

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
