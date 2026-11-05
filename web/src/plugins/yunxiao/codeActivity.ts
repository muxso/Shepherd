import type { CodeCommit } from './api'
const formatter = new Intl.DateTimeFormat('en-GB', { timeZone:'Asia/Shanghai',year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',second:'2-digit',hourCycle:'h23' })
export function dateTime(value:string) {
  const p=Object.fromEntries(formatter.formatToParts(new Date(value)).map(p=>[p.type,p.value]))
  return `${p.year}-${p.month}-${p.day} ${p.hour}:${p.minute}:${p.second}`
}
export const dateKey=(value:string)=>dateTime(value).slice(0,10)
export const latest=(items:CodeCommit[])=>items.reduce((a,c)=>!a || Date.parse(c.authoredAt)>Date.parse(a) ? c.authoredAt : a,'')
export function groupBy(items:CodeCommit[],key:(c:CodeCommit)=>string) {
  const map=new Map<string,CodeCommit[]>()
  for(const c of items){const k=key(c);const found=map.get(k);if(found)found.push(c);else map.set(k,[c])}
  return Array.from(map,([key,commits])=>({key,commits}))
}
export function monthGrid(month:string) {
  const [year,value]=month.split('-').map(Number)
  const start=new Date(0);start.setUTCFullYear(year,value-1,1);start.setUTCHours(0,0,0,0)
  const end=new Date(start);end.setUTCMonth(end.getUTCMonth()+1)
  const count=Math.round((+end-+start)/86_400_000),offset=(start.getUTCDay()+6)%7
  return Array.from({length:Math.ceil((offset+count)/7)},(_,week)=>Array.from({length:7},(_,day)=>{
    const n=week*7+day-offset+1
    return n>=1 && n<=count ? `${month}-${String(n).padStart(2,'0')}` : ''
  }))
}
