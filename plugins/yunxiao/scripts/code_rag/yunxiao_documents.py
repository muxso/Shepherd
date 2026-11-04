"""Completed Yunxiao source snapshots -> private, incremental RAG documents."""
import argparse
import concurrent.futures
import fcntl
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import threading
import time
import urllib.parse
import uuid
if __package__:
    from .index import Indexer, save, sanitize
else:
    from index import Indexer, save, sanitize

class PlainText(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True); self.parts=[]; self.hidden=0
    def handle_starttag(self, tag, attrs):
        if tag in ('script','style'): self.hidden+=1
        if not self.hidden and tag in ('p','div','li','br','tr','h1','h2','h3'): self.parts.append('\n')
        if not self.hidden and tag=='img': self.parts.append('[图片；文字索引不包含图片内容]')
    def handle_endtag(self, tag):
        if tag in ('script','style'): self.hidden=max(0,self.hidden-1)
        if not self.hidden and tag in ('p','div','li','tr'): self.parts.append('\n')
    def handle_data(self, value):
        if not self.hidden: self.parts.append(value)

def rich_text(value):
    if isinstance(value,dict):
        return rich_text(value.get('content') or value.get('jsonML') or value.get('text') or '')
    if isinstance(value,list):
        if value and isinstance(value[0],str):
            if value[0].lower() in ('script','style'): return ''
            return '\n'+' '.join(rich_text(v) for v in value[1:] if not isinstance(v,dict))+'\n'
        return '\n'.join(rich_text(v) for v in value)
    if not isinstance(value,str): return ''
    try:
        decoded=json.loads(value)
        if isinstance(decoded,(dict,list)): return rich_text(decoded)
    except (ValueError,TypeError): pass
    parser=PlainText(); parser.feed(value)
    return ''.join(parser.parts).strip()

def document(config, kind, row):
    source=config['yunxiao']['sourceId']; external=row['externalId']
    prefix='云效需求' if kind=='requirement' else '云效测试用例'
    title=f"{prefix} {row.get('serial') or external} {row.get('title') or ''}"
    lines=[f"来源：{prefix}；编号：{row.get('serial') or external}",f"标题：{row.get('title') or ''}",
           f"负责人：{row.get('owner') or '未提供'}；创建人：{row.get('creator') or '未提供'}",
           f"状态：{row.get('status') or '未提供'}；更新时间：{row.get('updatedAt') or '未提供'}"]
    if kind=='requirement':
        body=rich_text(row.get('description'))
        lines+=['# 需求正文',body or '源需求未提供可索引文字正文。']
    else:
        lines += [f"测试库：{row.get('repo') or ''}；目录：{row.get('directory') or ''}",
                  '# 前置条件',rich_text(row.get('precondition')) or '未提供', '# 测试步骤与预期结果']
        steps=row.get('steps') or []
        for i,step in enumerate(steps,1):
            lines += [f"## 步骤 {i}",rich_text(step.get('step')), '预期结果：'+rich_text(step.get('expected'))]
        if not steps: lines.append('源用例未提供步骤，不能推断测试覆盖。')
    lines += ['# 来源定位',f"/devops?source={source}&reference={external}" if kind=='requirement' else f"/devops?source={source}&kind=testcase&search={urllib.parse.quote(str(row.get('serial') or external))}"]
    title=sanitize(title.encode())
    if not title: raise RuntimeError('Source title rejected by credential policy')
    title=title[:300]
    text=sanitize('\n\n'.join(lines).encode())
    if not text: raise RuntimeError('Source document rejected by credential policy')
    return {'projectId':config['projectId'],'documentId':str(uuid.uuid5(uuid.NAMESPACE_URL,json.dumps([config['projectId'],source,kind,external]))),
            'title':title,'text':text,'sourceType':'yunxiao_'+kind,
            'sourceId':json.dumps({'sourceId':source,'externalId':external,'serial':row.get('serial'),'updatedAt':row.get('updatedAt')},ensure_ascii=False),
            'visibilityGroups':config.get('visibilityGroups',[])}

class YunxiaoDocuments(Indexer):
    def source(self):
        items=self.api('GET','/api/devops/sources')['items']
        source=next((s for s in items if s['id']==self.c['yunxiao']['sourceId']),None)
        if not source or source['project_id']!=self.c['projectId'] or not source.get('last_success_at'):
            raise RuntimeError('Completed source/project binding required')
        return source
    def snapshot(self):
        source=self.source(); result=[]
        scan_path=self.root/'yunxiao-source-scan.json'
        for kind in ('requirement','testcase'):
            expected=None; seen=set(); page=1
            while True:
                q=urllib.parse.urlencode({'kind':kind,'page':page,'pageSize':100})
                data=self.api('GET',f"/api/devops/sources/{source['id']}/records?{q}")
                save(scan_path,{'status':'collecting','kind':kind,'page':page,'total':data['total'],'updatedAt':int(time.time())})
                if expected is None: expected=data['total']
                if data['total']!=expected: raise RuntimeError('Source changed during pagination')
                for row in data['items']:
                    if row['externalId'] in seen: raise RuntimeError('Repeated source item during pagination')
                    seen.add(row['externalId']); result.append(document(self.c,kind,row))
                if len(seen)==expected: break
                if not data['items'] or len(seen)>expected: raise RuntimeError('Incomplete source pagination')
                page+=1
        if self.source()['last_success_at']!=source['last_success_at']:
            raise RuntimeError('Source revision changed; preserve prior knowledge')
        save(scan_path,{'status':'complete','documents':len(result),'updatedAt':int(time.time())})
        return source,result
    def run(self):
        state_path=self.root/'yunxiao-documents.json'
        with (self.root/'yunxiao-documents.lock').open('w') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            identity=[self.c['apiUrl'],self.c['projectId'],self.c['yunxiao']['sourceId'],self.c.get('visibilityGroups',[]),self.c['embeddingRevision'],1]
            state=json.loads(state_path.read_text()) if state_path.exists() else {}
            if state and state.get('identity')!=identity: raise RuntimeError('Document identity changed; use fresh state directory')
            source,docs=self.snapshot()
            state={**state,'identity':identity,'status':'indexing','sourceRevision':source['last_success_at'],'total':len(docs),'documents':state.get('documents',{}),'startedAt':int(time.time())}
            save(state_path,state); mutex=threading.Lock()
            def ingest(doc):
                digest=hashlib.sha256(json.dumps(doc,ensure_ascii=False,sort_keys=True).encode()).hexdigest()
                if state['documents'].get(doc['documentId'],{}).get('digest')==digest: return
                result=self.api('POST','/rag/document',doc)
                if not result.get('embedded'): raise RuntimeError('Embedding not available; no checkpoint advance')
                with mutex:
                    state['documents'][doc['documentId']]={'digest':digest,'chunks':result['chunks'],'sourceType':doc['sourceType']}
                    state['indexedDocuments']=len(state['documents']);state['updatedAt']=int(time.time());save(state_path,state)
            try:
                with concurrent.futures.ThreadPoolExecutor(max_workers=self.c.get('workers',2)) as pool:
                    list(pool.map(ingest,docs))
                # Never delete after a failed/updating source traversal or partial embedding run.
                if self.source()['last_success_at']!=source['last_success_at']: raise RuntimeError('Source changed while indexing; retirement deferred')
                live={d['documentId'] for d in docs}
                for doc_id in list(state['documents']):
                    if doc_id not in live:
                        self.api('DELETE','/rag/document/'+doc_id);del state['documents'][doc_id];save(state_path,state)
                state.update(status='complete',completedAt=int(time.time()),indexedDocuments=len(state['documents']))
                save(state_path,state)
                print(json.dumps({k:v for k,v in state.items() if k not in ('identity','documents')}),flush=True)
                return 0
            except Exception:
                state.update(status='failed',updatedAt=int(time.time()));save(state_path,state)
                raise

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--config',required=True);a=p.parse_args()
    raise SystemExit(YunxiaoDocuments(json.loads(Path(a.config).read_text())).run())
