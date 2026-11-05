import { useMemo, useState } from 'react'
import { Button, Drawer, Empty, Input, Tooltip } from 'antd'
import type { CodeCommit } from './api'
import { dateKey, dateTime, groupBy, latest, monthGrid } from './codeActivity'
import './code-calendar.css'
const instantMotion={motionName:'',motionAppear:false,motionEnter:false,motionLeave:false}
const personKey=(c:CodeCommit)=>JSON.stringify([c.author,c.group])
function people(commits:CodeCommit[]) {
  return groupBy(commits,personKey).map(p=>({...p,name:p.commits[0].author || '待归属',group:p.commits[0].group || '待归属',last:latest(p.commits)})).sort((a,b)=>a.name.localeCompare(b.name,'zh-CN') || a.group.localeCompare(b.group,'zh-CN'))
}
export default function AuthorCommitCalendar({month,commits,canRequirements,openRequirement}:{month:string;commits:CodeCommit[];canRequirements:boolean;openRequirement:(id:string)=>void}) {
  const [showWholeMonth,setShowWholeMonth]=useState(false)
  const [selection,setSelection]=useState<{day:string;person?:string}|null>(null)
  const [search,setSearch]=useState('')
  const select=(value:{day:string;person?:string})=>{setSearch('');setSelection(value)}
  const days=useMemo(()=>new Map(groupBy(commits.filter(c=>dateKey(c.authoredAt).startsWith(month+'-')),c=>dateKey(c.authoredAt)).map(d=>[d.key,d.commits])),[commits,month])
  const grid=useMemo(()=>monthGrid(month),[month])
  const today=dateKey(new Date().toISOString())
  const lastRelevantWeek=Math.max(grid.findIndex(w=>w.includes(today)),...grid.map((week,i)=>week.some(day=>days.has(day))?i:-1))
  const canCollapse=month===today.slice(0,7) && lastRelevantWeek>=0 && lastRelevantWeek<grid.length-1
  const shownGrid=canCollapse && !showWholeMonth ? grid.slice(0,lastRelevantWeek+1) : grid
  const selected=selection ? people(days.get(selection.day) || []).filter(p=>!selection.person || p.key===selection.person) : []
  const visible=selected.filter(p=>(p.name+' '+p.group).toLowerCase().includes(search.trim().toLowerCase()))
  const totalCommits=selected.reduce((n,p)=>n+p.commits.length,0)
  const totalRepos=new Set(selected.flatMap(p=>p.commits.map(c=>c.repoId))).size
  return <section className="yx-commit-calendar" aria-label="个人每日最后提交日历">
    <div className="yx-calendar-guide"><span><i /> 已采集提交 · 点击成员查看当天详情</span><span>最后提交时间 · 上海时区</span></div>
    {!days.size && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="所选范围暂无已采集提交" />}
    <table className="yx-calendar-grid" aria-label={`${month} 每人每日最后提交时间`}>
      <thead><tr>{['一','二','三','四','五','六','日'].map((d,i)=><th key={d} scope="col" className={i>4?'is-weekend':''}>周{d}</th>)}</tr></thead>
      <tbody>{shownGrid.map((week,w)=><tr key={w}>{week.map((day,i)=>{
        if(!day)return <td key={i} className="is-outside" aria-hidden="true" />
        const records=days.get(day) || [],members=people(records),future=day>today && !records.length
        return <td key={day} data-date={day} className={`${records.length?'has-activity':''} ${records.length>=10?'many-activity':''} ${day===today?'is-today':''} ${future?'is-future':''}`} aria-label={day}>
          <div className="yx-calendar-day-head"><button disabled={!records.length} onClick={()=>select({day})} aria-label={`查看 ${day} 全部 ${members.length} 位成员的提交`}><time dateTime={day}>{Number(day.slice(-2))}</time></button><span>{members.length ? `${members.length}人` : ''}</span></div>
          <div className="yx-calendar-day-people">{members.slice(0,3).map(p=><Tooltip key={p.key} title={`${p.name} · ${p.group} · 最后提交 ${dateTime(p.last).slice(11)}`}><button className="yx-calendar-person" onClick={()=>select({day,person:p.key})} aria-label={`${day} ${p.name} 最后提交 ${dateTime(p.last).slice(11)}，${p.commits.length} 次提交`}><span>{p.name}</span><time dateTime={p.last}>{dateTime(p.last).slice(11,16)}</time></button></Tooltip>)}</div>
          {members.length>3 && <button className="yx-calendar-more" onClick={()=>select({day})} aria-label={`查看 ${day} 全部 ${members.length} 位成员`}><span className="yx-calendar-more-full">另 {members.length-3} 人</span><span className="yx-calendar-more-short">+{members.length-3}</span></button>}
          {!members.length && <span className="yx-calendar-empty">{future?'—':'暂无记录'}</span>}
        </td>
      })}</tr>)}</tbody>
    </table>
    {canCollapse && <div className="yx-calendar-expand"><Button type="link" size="small" onClick={()=>setShowWholeMonth(v=>!v)}>{showWholeMonth ? '仅看截至本周' : '展开整月'}</Button></div>}
    <div className="yx-calendar-footnote">时间取当天已采集提交中最后一次 Git 作者时间；空白不代表未工作。筛选成员、仓库或语言后，按筛选范围计算。</div>
    <Drawer title={selection ? `${selection.day} · 每日最后提交明细` : '每日最后提交明细'} width={780} styles={{wrapper:{maxWidth:'96vw'}}} motion={instantMotion} maskMotion={instantMotion} open={!!selection} onClose={()=>setSelection(null)} rootClassName="yx-calendar-drawer" destroyOnHidden>
      <div className="yx-calendar-overview"><div><span>活跃成员</span><strong>{selected.length}<small>人</small></strong></div><div><span>代码提交</span><strong>{totalCommits}<small>次</small></strong></div><div><span>涉及仓库</span><strong>{totalRepos}<small>个</small></strong></div></div>
      <div className="yx-calendar-detail-toolbar"><span>成员提交明细 <small>上海时区</small></span>{selected.length>1 && <Input allowClear aria-label="搜索当天成员" placeholder="搜索成员或分组" value={search} onChange={e=>setSearch(e.target.value)} />}</div>
      <div className="yx-calendar-list-head"><span>成员 / 分组</span><span>最后提交</span><span>提交 / 仓库</span><span /></div>
      <div key={`${selection?.day}-${selection?.person || 'all'}`} className="yx-calendar-members">
      {!visible.length && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="没有匹配的成员" />}
      {visible.map(p=>{
        const repos=groupBy(p.commits,c=>c.repoId)
        const requirements=canRequirements ? [...new Map(p.commits.flatMap(c=>c.requirements).map(r=>[r.externalId,r])).values()] : []
        return <details className="yx-calendar-detail" key={p.key} open={!!selection?.person}>
          <summary className="yx-calendar-member-row"><span className="yx-calendar-member-name"><strong>{p.name}</strong><small>{p.group}</small></span><span className="yx-calendar-detail-time"><time dateTime={p.last}>{dateTime(p.last).slice(11)}</time></span><span className="yx-calendar-member-count">{p.commits.length} 次 <i>·</i> {repos.length} 库</span><span className="yx-calendar-member-arrow" aria-hidden="true">›</span></summary>
          <div className="yx-calendar-member-body">
          <h4>代码仓库 <small>{repos.length}</small></h4>{repos.map(r=><div className="yx-calendar-detail-repo" key={r.key}><span>{r.commits[0].repo || r.key}</span><small>{r.commits.length} 次 · 最后 {dateTime(latest(r.commits)).slice(11)}</small></div>)}
          <h4>关联需求 <small>{canRequirements ? requirements.length : '—'}</small></h4>{!canRequirements ? <span className="yx-muted">无需求读取权限</span> : requirements.length ? requirements.map(r=>r.title ? <div key={r.externalId}><Button type="link" size="small" className="yx-calendar-requirement" onClick={()=>openRequirement(r.externalId)}>{r.serial || r.externalId} {r.title}</Button></div> : <div key={r.externalId}>{r.externalId}（名称未同步）</div>) : <span className="yx-muted">未关联需求</span>}
          <details className="yx-calendar-commits"><summary>查看当天提交记录（最近 {Math.min(20,p.commits.length)} 条）</summary><ol>{[...p.commits].sort((a,b)=>Date.parse(b.authoredAt)-Date.parse(a.authoredAt)).slice(0,20).map(c=><li key={c.id}><time>{dateTime(c.authoredAt).slice(11)}</time> <code title={c.sha}>{c.sha.slice(0,10)}</code><span>{c.repo || c.repoId}</span></li>)}</ol></details>
          </div>
        </details>
      })}
      </div>
      <p className="yx-calendar-footnote">仅表示已观测的 Git 提交记录，不推断工时或绩效。</p>
    </Drawer>
  </section>
}
