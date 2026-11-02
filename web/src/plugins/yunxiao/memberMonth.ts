/** Calendar months are Shanghai dates, never UTC instants or browser-local dates. */
export function shanghaiMonth(now = new Date()) {
  const parts = new Intl.DateTimeFormat('en-CA', { timeZone:'Asia/Shanghai',year:'numeric',month:'2-digit' }).formatToParts(now)
  return `${parts.find(p => p.type === 'year')!.value}-${parts.find(p => p.type === 'month')!.value}`
}
export function previousMonth(now = new Date()) {
  const [year,month] = shanghaiMonth(now).split('-').map(Number)
  return month === 1 ? `${year-1}-12` : `${year}-${String(month-1).padStart(2,'0')}`
}
export function validMonth(value:string | null, now = new Date()): value is string {
  return !!value && /^\d{4}-(0[1-9]|1[0-2])$/.test(value) && Number(value.slice(0,4)) > 0 && value <= shanghaiMonth(now)
}
