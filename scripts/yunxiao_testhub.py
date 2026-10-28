"""Read-only Testhub collector and cached Projex requirement details."""
import concurrent.futures
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import re

class TextParser(HTMLParser):
    def __init__(self):super().__init__(convert_charrefs=True);self.parts=[]
    def handle_starttag(self,tag,attrs):
        if tag in ('p','div','li','br','tr','article','h1','h2','h3'):self.parts.append('\n')
        if tag=='img':
            src=dict(attrs).get('src','')
            if src:self.parts.append('[图片] '+src)
    def handle_endtag(self,tag):
        if tag in ('p','div','li','tr','article'):self.parts.append('\n')
    def handle_data(self,data):self.parts.append(data)

def text(value):
    value=str(value or '')
    if not re.search(r'</?(?:article|p|div|span|br|table|tr|td|ul|ol|li|img|a|strong|em|b|i)(?:\s|/?>)',value,re.I):return value.strip()
    p=TextParser();p.feed(value)
    return re.sub(r'\n{3,}','\n\n',''.join(p.parts)).strip()

def enrich_requirements(items,collector):
    path=Path(os.environ.get('SHEPHERD_REQUIREMENT_CACHE','/home/liber/.cache/shepherd-yunxiao/requirement-details.json'))
    try:cache=json.loads(path.read_text())
    except FileNotFoundError:cache={}
    org=collector.cfg()['YUNXIAO_ORG']
    def detail(item):
        key=str(item['id']);old=cache.get(key,{})
        if item.get('gmtModified') is not None and old.get('modified')==str(item.get('gmtModified')) and old.get('data'):return key,old['data']
        d,_=collector.api(f'projex/organizations/{org}/workitems/{key}')
        if not isinstance(d,dict) or str(d.get('id'))!=key:raise RuntimeError('invalid requirement detail response')
        return key,dict(item,**d)
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as ex:results=list(ex.map(detail,items))
    for key,d in results:cache[key]={'modified':str(d.get('gmtModified')),'data':d}
    path.parent.mkdir(mode=0o700,parents=True,exist_ok=True)
    tmp=path.with_suffix('.tmp');tmp.write_text(json.dumps(cache,ensure_ascii=False));tmp.chmod(0o600);tmp.replace(path)
    return [d for _,d in results]

def case_body(item):
    body=item.get('testSteps') or {};content=body.get('content') or []
    if body.get('contentType')=='TEXT' and (body.get('stepContent') or body.get('expectedResult')):
        steps=[{'step':text(body.get('stepContent')),'expected':text(body.get('expectedResult'))}]
    else:steps=[{'step':text(s.get('step')),'expected':text(s.get('expected'))} for s in content if isinstance(s,dict)]
    return {'steps':steps,'precondition':text(item.get('preCondition'))}

def enrich_testcases(records,collector):
    path=Path(os.environ.get('SHEPHERD_TESTCASE_CACHE','/home/liber/.cache/shepherd-yunxiao/testcase-details.json'))
    try:cache=json.loads(path.read_text())
    except FileNotFoundError:cache={}
    org=collector.cfg()['YUNXIAO_ORG']
    def detail(r):
        key=r['externalId'];item=r['data']['raw'];old=cache.get(key,{})
        if item.get('gmtModified') is not None and old.get('modified')==str(item.get('gmtModified')) and old.get('data'):return key,old['data']
        rid=r['data']['repoId'];tid=item['id']
        d,_=collector.api(f'testhub/organizations/{org}/testRepos/{rid}/testcases/{tid}')
        if not isinstance(d,dict) or str(d.get('id'))!=str(tid):raise RuntimeError('invalid testcase detail response')
        return key,dict(item,**d)
    def save():
        path.parent.mkdir(mode=0o700,parents=True,exist_ok=True)
        temp=path.with_suffix('.tmp');temp.write_text(json.dumps(cache,ensure_ascii=False));temp.chmod(0o600);temp.replace(path)
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=6) as ex:
            futures=[ex.submit(detail,r) for r in records]
            for i,f in enumerate(concurrent.futures.as_completed(futures),1):
                key,d=f.result();cache[key]={'modified':str(d.get('gmtModified')),'data':d}
                if i%500==0:save();print(f'Testhub content details checked: {i}/{len(records)}',flush=True)
    finally:save()
    for r in records:
        item=cache[r['externalId']]['data']
        original=r['data']
        fresh=normalize_case(item,{'testRepoIdentifier':original['repoId'],'name':original.get('repo') or original['repoId']},{},collector)['data']
        fresh['modulePath']=original.get('modulePath') or fresh['modulePath']
        fresh['directory']=original.get('directory') or fresh['directory']
        r['data']=fresh
    return records

def repo_list(collector):
    root='testhub/organizations/'+collector.cfg()['YUNXIAO_ORG'];out=[]
    for pg in range(1,1001):
        d,h=collector.api(root+'/testRepo/list',query={'page':pg,'perPage':100});batch=collector.rows(d);out+=batch
        total=collector.total_pages(h)
        if not batch or (total and pg>=total) or (total is None and len(batch)<100):return out
    raise RuntimeError('Testhub repository pagination limit exceeded')

def normalize_case(item,repo,directories,collector):
    directory=item.get('directory') or {};did=str(directory.get('id') or '')
    path=[];seen=set()
    while did and did not in seen:
        seen.add(did);d=directories.get(did) or (directory if did==str(directory.get('id') or '') else {})
        if d.get('name'):path.insert(0,str(d['name']))
        did=str(d.get('parentId') or '')
    steps=case_body(item)['steps']
    priority='P2'
    for f in item.get('customFieldValues') or []:
        if f.get('fieldName')=='优先级':
            for v in f.get('values') or []:
                p=v.get('displayValue') if isinstance(v,dict) else str(v)
                priority={'紧急':'P0','高':'P1','中':'P2','低':'P3'}.get(p,p if p in ('P0','P1','P2','P3') else 'P2')
    def iso(v):
        d=collector.dateval(v);return d.isoformat() if d else ''
    rid=repo['testRepoIdentifier'];name=repo.get('name') or rid
    return {'kind':'testcase','externalId':rid+':'+str(item['id']),'data':{
        'serial':item.get('customCode') or str(item['id']),'title':item.get('subject') or '',
        'repo':name,'repoId':rid,'directory':' / '.join(path),'modulePath':['云效测试库',name]+path,
        'owner':collector.pname(item.get('assignedTo')),'creator':collector.pname(item.get('creator')),
        'priority':priority,'status':'PREPARED','precondition':text(item.get('preCondition')),
        'steps':steps,'labels':[str(x.get('name')) for x in item.get('labels') or [] if isinstance(x,dict) and x.get('name')],
        'createdAt':iso(item.get('gmtCreate')),'updatedAt':iso(item.get('gmtModified')),'raw':item}}

def collect_testcases(collector,repo_ids):
    if not repo_ids:return []
    repos={r['testRepoIdentifier']:r for r in repo_list(collector)}
    if set(repo_ids)-set(repos):raise RuntimeError('configured Testhub repositories are not readable; preserving existing snapshot')
    root='testhub/organizations/'+collector.cfg()['YUNXIAO_ORG']
    def collect(rid):
        repo=repos[rid];d,_=collector.api(f'{root}/testRepos/{rid}/directories');dirs={str(x['id']):x for x in collector.rows(d)}
        out=[]
        for pg in range(1,1001):
            d,h=collector.api(f'{root}/testRepos/{rid}/testcases:search','POST',{'page':pg,'perPage':200,'orderBy':'gmtCreate','sort':'asc'})
            batch=collector.rows(d);out.extend(normalize_case(x,repo,dirs,collector) for x in batch)
            total=collector.total_pages(h)
            if not batch or (total and pg>=total) or (total is None and len(batch)<200):return out
        raise RuntimeError('Testhub testcase pagination limit exceeded')
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as ex:groups=list(ex.map(collect,repo_ids))
    return enrich_testcases([r for group in groups for r in group],collector)
