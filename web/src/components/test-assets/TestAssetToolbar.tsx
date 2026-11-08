import { useCallback,useEffect,useState } from 'react'
import { Alert,Button,Input,Modal,Segmented,Select,Space,Table } from 'antd'
import { message } from '../../feedback'
import { assetApi,type TestAssets,type CaseAsset } from './api'
import ServiceSettings from './ServiceSettings'
import SourceImport from './SourceImport'
import DefinitionApiLinks from './DefinitionApiLinks'
import './test-assets.css'
export const emptyAssets:TestAssets={services:[],cases:[],mappings:[],scenarios:[],inventories:[],imports:[],permissions:{edit:false,manage:false,import:false}}
export function useTestAssets(projectId:string){
 const [result,setResult]=useState<{project:string;data:TestAssets}|null>(null),[revision,setRevision]=useState(0),[error,setError]=useState(''),[loading,setLoading]=useState(false)
 const reload=useCallback(()=>setRevision(v=>v+1),[])
 useEffect(()=>{if(!projectId)return;let live=true;setLoading(true);setError('');assetApi.overview(projectId).then(data=>{if(!Array.isArray(data.services)||!Array.isArray(data.cases))throw Error('服务资产响应不可用');if(live)setResult({project:projectId,data})}).catch(e=>{if(live)setError(e.message)}).finally(()=>{if(live)setLoading(false)});return()=>{live=false}},[projectId,revision])
 return {data:result?.project===projectId?result.data:emptyAssets,error,loading,reload}
}
export default function TestAssetToolbar({projectId,data,error,view,onView,service,onService,onChanged}:{projectId:string;data:TestAssets;error:string;view:string;onView:(value:string)=>void;service:string;onService:(value:string)=>void;onChanged:()=>void}){
 const [apiOpen,setApiOpen]=useState(false)
 const [settings,setSettings]=useState(false),[importing,setImporting]=useState(false),[assigning,setAssigning]=useState(false),[chosen,setChosen]=useState<string[]>([]),[target,setTarget]=useState<string|null>(null),[query,setQuery]=useState(''),[busy,setBusy]=useState(false)
 const filtered=data.cases.filter(c=>(!service||(service==='UNASSIGNED'?!c.serviceId:c.serviceId===service))&&(!query||`${c.name} ${c.library} ${c.directory}`.toLowerCase().includes(query.toLowerCase())))
 const assign=async()=>{setBusy(true);try{await assetApi.assign(projectId,chosen,target);setAssigning(false);setChosen([]);onChanged();message.success(`已更新 ${chosen.length} 条用例的服务归属`)}catch(e){message.error(String(e))}finally{setBusy(false)}}
 return <><div className="test-assets-toolbar">
  <Space wrap><Segmented aria-label="功能测试资产视角" value={view} onChange={v=>onView(String(v))} options={[{value:'definitions',label:'定义管理'},{value:'scenarios',label:'场景测试'}]}/><Select aria-label="功能用例服务" value={service} style={{width:220}} onChange={onService} options={[{value:'',label:'全部服务'},{value:'UNASSIGNED',label:'待归属'},...data.services.map(s=>({value:s.id,label:s.name}))]}/><span className="test-assets-note">{data.services.length} 个服务 · {data.cases.length.toLocaleString()} 条定义 · {data.scenarios.length} 个场景</span></Space>
  <Space wrap>{data.permissions.edit&&<Button onClick={()=>setApiOpen(true)}>定义与接口关联</Button>}{data.permissions.manage&&<><Button onClick={()=>{setAssigning(true);setChosen([]);setQuery('')}}>批量服务归属</Button><Button onClick={()=>setSettings(true)}>服务与接口梳理</Button></>}{data.permissions.import&&<Button onClick={()=>setImporting(true)}>同步与差异对账</Button>}</Space>
 </div>{error&&<Alert type="warning" title="服务资产暂不可用" description={error} showIcon style={{margin:'0 12px 8px'}}/>}
 <DefinitionApiLinks key={projectId+'api'} projectId={projectId} data={data} open={apiOpen} onClose={()=>setApiOpen(false)} onChanged={onChanged}/><ServiceSettings key={projectId} projectId={projectId} data={data} open={settings} onClose={()=>setSettings(false)} onChanged={onChanged}/><SourceImport key={projectId} projectId={projectId} open={importing} onClose={()=>setImporting(false)} onChanged={onChanged}/>
 <Modal title="批量指定用例的归属服务" open={assigning} onCancel={()=>setAssigning(false)} onOk={assign} confirmLoading={busy} okButtonProps={{disabled:!chosen.length}} width={1050} destroyOnHidden>
  <Space wrap style={{marginBottom:16}}><Select aria-label="批量归属目标服务" placeholder="选择目标服务" style={{width:270}} value={target} onChange={setTarget} options={[{value:null,label:'取消单独指定，沿用来源映射'},...data.services.map(s=>({value:s.id,label:s.name}))]}/><Input.Search aria-label="搜索服务归属用例" placeholder="名称、源测试库或目录" allowClear onSearch={setQuery} style={{width:320}}/><span>已选择 {chosen.length} 条</span></Space>
  <Table<CaseAsset> rowKey="id" rowSelection={{selectedRowKeys:chosen,onChange:ids=>setChosen(ids.map(String)),preserveSelectedRowKeys:true}} dataSource={filtered} size="small" pagination={{pageSize:15,showTotal:n=>`${n} 条`}} columns={[{title:'用例',dataIndex:'name'},{title:'当前服务',dataIndex:'serviceId',render:id=>data.services.find(s=>s.id===id)?.name||'待归属'},{title:'源测试库',dataIndex:'library'},{title:'源目录',dataIndex:'directory'}]}/>
 </Modal></>
}
