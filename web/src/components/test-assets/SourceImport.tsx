import { useEffect,useState } from 'react'
import { Alert,Button,Descriptions,Empty,Input,Popconfirm,Select,Space,Table,Tag,Spin } from 'antd'
import ResizableDrawer from '../ResizableDrawer'
import { message } from '../../feedback'
import { assetApi,type ImportBatch,type ImportItem } from './api'
const actionLabels:Record<string,string>={CREATE:'新增',ADOPT:'关联已有资产',UPDATE:'源字段更新',UNCHANGED:'无需更新',CONFLICT:'内容冲突',SOURCE_MISSING:'源记录缺失'}
export default function SourceImport({projectId,open,onClose,onChanged}:{projectId:string;open:boolean;onClose:()=>void;onChanged:()=>void}){
 const [sources,setSources]=useState<{id:string;name:string;last_success_at?:string}[]>([]),[source,setSource]=useState(''),[batch,setBatch]=useState<ImportBatch|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[filter,setFilter]=useState(''),[query,setQuery]=useState(''),[resolutions,setResolutions]=useState<Record<string,string>>({})
 useEffect(()=>{if(!open)return;let live=true;setBatch(null);setError('');assetApi.sources(projectId).then(d=>{if(live){setSources(d.items);setSource(value=>d.items.some(s=>s.id===value)?value:d.items[0]?.id||'')}}).catch(e=>{if(live)setError(e.message)});return()=>{live=false}},[open,projectId])
 const preview=async()=>{setBusy(true);setError('');setBatch(null);setResolutions({});try{setBatch(await assetApi.preview(projectId,source))}catch(e){setError(String(e))}finally{setBusy(false)}}
 const apply=async()=>{if(!batch)return;setBusy(true);setError('');try{const result=await assetApi.apply(projectId,batch.id,resolutions);setBatch({...batch,status:'APPLIED',result:{...result,sourceMissing:batch.preview.counts.SOURCE_MISSING||0}});onChanged();message.success(`同步完成：处理 ${result.imported} 条，保留 ${result.conflicts} 条待处理冲突`)}catch(e){setError(String(e))}finally{setBusy(false)}}
 const items=(batch?.preview.items||[]).filter(r=>(!filter||r.action===filter)&&(!query||`${r.title} ${r.externalId}`.toLowerCase().includes(query.toLowerCase())))
 return <ResizableDrawer title="插件资产同步与差异对账" open={open} onClose={onClose} width="min(1250px,calc(100vw - 72px))" destroyOnHidden>
  <p className="test-assets-note">同步到当前项目，按来源 ID 复用已有需求和用例。手工资产、已有需求关联和执行历史保留；源缺失项保留并标记。</p>
  {error&&<Alert showIcon type="error" title="同步未完成" description={error} style={{marginBottom:16}}/>}
  <Space wrap><Select aria-label="插件资产同步来源" value={source||undefined} onChange={v=>{setSource(v);setBatch(null)}} style={{minWidth:220}} options={sources.map(s=>({value:s.id,label:s.name}))}/><Button type="primary" onClick={preview} disabled={!source} loading={busy}>生成同步预览</Button></Space>
  {!sources.length&&!busy&&<Empty description="暂无可读取的已完成同步来源"/>}
  {batch&&<>
   <Descriptions size="small" column={2} style={{marginTop:20}} items={[{key:'source',label:'源快照版本',children:batch.source_revision},{key:'manual',label:'独立手工资产',children:`${batch.preview.manualAssets} 条，保持原样`}]} />
   <Space wrap style={{marginBlock:16}}>{Object.entries(actionLabels).map(([key,label])=><Button key={key} type={filter===key?'primary':'default'} size="small" onClick={()=>setFilter(filter===key?'':key)}>{label} {batch.preview.counts[key]||0}</Button>)}<Input.Search aria-label="搜索同步差异" placeholder="标题或来源编号" allowClear onSearch={setQuery} style={{width:230}}/></Space>
   {batch.status!=='APPLIED'&&(batch.preview.counts.CONFLICT||0)>0&&<Space wrap style={{marginBottom:12}}><Button onClick={()=>setResolutions(current=>({...current,...Object.fromEntries((batch.preview.items||[]).filter(r=>r.action==='CONFLICT').map(r=>[r.kind+':'+r.externalId,'keep-local']))}))}>所有冲突保留本地并关联来源</Button><Button onClick={()=>setResolutions({})}>清除冲突决策</Button><span className="test-assets-note">可逐条展开对比；采用来源内容需逐条确认。</span></Space>}
   <Table<ImportItem> rowKey={r=>r.kind+':'+r.externalId} size="small" dataSource={items} pagination={{pageSize:20,showTotal:n=>`${n} 条`}} scroll={{x:900}} columns={[
    {title:'类型',dataIndex:'kind',width:90,render:v=>v==='testcase'?'功能用例':'需求'},
    {title:'标题 / 来源编号',render:(_,r)=><><div>{r.title}</div><small style={{color:'var(--text-3)'}}>{r.externalId}</small></>},
    {title:'处理方式',dataIndex:'action',width:140,render:v=><Tag color={v==='CONFLICT'?'orange':v==='CREATE'?'green':undefined}>{actionLabels[v]||v}</Tag>},
    {title:'冲突决策',width:220,render:(_,r)=>r.action==='CONFLICT'?<Select aria-label={`处理冲突 ${r.externalId}`} value={resolutions[r.kind+':'+r.externalId]||''} disabled={batch.status==='APPLIED'} style={{width:205}} onChange={value=>setResolutions(v=>({...v,[r.kind+':'+r.externalId]:value}))} options={[{value:'',label:'暂不处理，保留待核对'},{value:'keep-local',label:'保留本地内容并关联来源'},...(!r.deleted&&!r.archived?[{value:'use-source',label:'确认采用源管理字段'}]:[])]}/>:null},
   ]} expandable={{expandedRowRender:r=><ImportComparison projectId={projectId} batchId={batch.id} item={r}/>,rowExpandable:r=>r.action!=='SOURCE_MISSING' }} />
   {batch.status==='APPLIED'?<Alert type="success" showIcon title="本批同步已完成" description={`处理 ${batch.result?.imported||0} 条，未变更 ${batch.result?.unchanged||0} 条，待处理冲突 ${batch.result?.conflicts||0} 条。`}/>:<Popconfirm title="按当前预览同步到此项目？" description="未作决策的冲突会保留；采用源字段的项目将更新，已有 ID、关联和执行历史保持。" onConfirm={apply} okText="确认同步"><Button type="primary" loading={busy}>确认并发布本批同步</Button></Popconfirm>}
  </>}
 </ResizableDrawer>
}

function ImportComparison({projectId,batchId,item}:{projectId:string;batchId:string;item:ImportItem}){
 const [result,setResult]=useState<ImportItem|null>(null),[error,setError]=useState('')
 useEffect(()=>{let live=true;assetApi.importItem(projectId,batchId,item.kind,item.externalId).then(d=>{if(live)setResult(d)}).catch(e=>{if(live)setError(e.message)});return()=>{live=false}},[projectId,batchId,item.kind,item.externalId])
 if(error)return <Alert type="error" title={error}/>
 if(!result)return <Spin/>
 return <div style={{display:'grid',gridTemplateColumns:'1fr 1fr',gap:16}}>{[['当前内容',result.current],['源管理字段',result.target]].map(([label,value])=><div key={String(label)}><strong>{String(label)}</strong><pre style={{whiteSpace:'pre-wrap',wordBreak:'break-word',maxHeight:360,overflow:'auto',fontSize:12}}>{value?JSON.stringify(value,null,2):'无'}</pre></div>)}</div>
}
