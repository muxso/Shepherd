"""Read-only multi-branch Git evidence and per-author Shanghai daily RAG documents."""
import collections
import datetime as dt
import hashlib
import json
from pathlib import Path
import subprocess
import time
import uuid
TZ=dt.timezone(dt.timedelta(hours=8))

def cutoff(config):
    return (dt.datetime.now(TZ)-dt.timedelta(days=max(1,min(366,int(config.get('gitHistoryDays',90)))))).replace(hour=0,minute=0,second=0,microsecond=0)

def fetch(repo,spec,config,git):
    start=cutoff(config);prefix='refs/remotes/origin/'
    heads={}
    for line in git(repo,'ls-remote','--heads','origin').decode().splitlines():
        sha,ref=line.split()
        if not ref.startswith('refs/heads/'):raise RuntimeError('Invalid remote branch reference')
        heads[prefix+ref[len('refs/heads/'):]]=sha
    default=prefix+spec['branch']
    if default not in heads:raise RuntimeError('Configured default branch is not advertised')
    marker=Path(repo)/'shepherd-history.json'
    old=json.loads(marker.read_text()) if marker.exists() else {}
    covered=old.get('since') and dt.datetime.fromisoformat(old['since'])<=start
    unknown=[]
    for ref,sha in heads.items():
        try:git(repo,'cat-file','-e',sha+'^{commit}')
        except subprocess.CalledProcessError:unknown.append('+refs/heads/'+ref[len(prefix):]+':'+ref)
    # Bound first contact with a branch to its tip. Dormant Codeup repositories can hang when
    # deepen-since is requested before checking whether any branch has commits in that window.
    if unknown:git(repo,'fetch','--no-tags','--depth=1','origin',*unknown)
    current=git(repo,'for-each-ref','--format=%(refname)','refs/remotes/origin/').decode().splitlines()
    for ref in current:
        if ref not in heads:git(repo,'update-ref','--no-deref','-d',ref)
    for ref,sha in heads.items():git(repo,'update-ref',ref,sha)
    active=[]
    for line in git(repo,'for-each-ref','--format=%(refname)%00%(objectname)%00%(committerdate:iso-strict)','refs/remotes/origin/').decode().splitlines():
        ref,sha,date=line.split('\0',2)
        if dt.datetime.fromisoformat(date)>=start and not (covered and old.get('heads',{}).get(ref)==sha):
            active.append('+refs/heads/'+ref[len(prefix):]+':'+ref)
    if active:git(repo,'fetch','--no-tags','--shallow-since='+start.isoformat(),'origin',*active)
    temp=marker.with_suffix('.tmp');temp.write_text(json.dumps({'since':start.isoformat(),'heads':heads}));temp.chmod(0o600);temp.replace(marker)
    return heads[default]


def collect(repo,spec,config,git,eligible,sanitize):
    refs=git(repo,'for-each-ref','--format=%(refname)','refs/remotes/origin/').decode().splitlines()
    if not refs: raise RuntimeError('No remote branches fetched for activity history')
    start=cutoff(config)
    shas=set(git(repo,'log','--no-merges','--since='+start.isoformat(),'--format=%H',*refs,'--').decode().splitlines())
    result=[]
    for sha in sorted(shas):
        raw=git(repo,'show','-s','--format=%H%x00%aI%x00%cI%x00%an%x00%ae%x00%B',sha,'--').decode('utf-8',errors='replace').strip()
        oid,authored,committed,author,email,message=raw.split('\0',5)
        at=dt.datetime.fromisoformat(authored)
        if at<start or at>dt.datetime.now(TZ)+dt.timedelta(days=1): continue
        message=sanitize(message.encode()) or '[提交说明因凭据过滤未索引]'
        files=[]; additions=deletions=unknown=0
        for entry in git(repo,'show','--format=','--numstat','--no-renames','-z',sha,'--').split(b'\0'):
            if not entry: continue
            added,deleted,path=entry.split(b'\t',2);path=path.decode('utf-8',errors='replace')
            if not eligible(path,1): continue
            files.append(path)
            if added.isdigit() and deleted.isdigit(): additions+=int(added);deletions+=int(deleted)
            else: unknown+=1
        result.append({'sha':oid,'authoredAt':authored,'committedAt':committed,'author':author,'email':email,
                       'message':message,'files':files,'additions':additions,'deletions':deletions,'unknownStats':unknown,
                       'repositoryId':str(spec['id']),'repository':spec['name']})
    return result

def daily_documents(config,commits):
    aliases=json.loads(Path(config['authorAliasesFile']).read_text()) if config.get('authorAliasesFile') else {}
    groups=collections.defaultdict(list);seen=set()
    for commit in commits:
        key=(commit['repositoryId'],commit['sha'])
        if key in seen: continue
        seen.add(key)
        author=aliases.get(commit['email']) or aliases.get(commit['author']) or commit['author']
        date=dt.datetime.fromisoformat(commit['authoredAt']).astimezone(TZ).date().isoformat()
        groups[(author,date)].append(commit)
    result=[]
    scope=config.get('codeup',{}).get('sourceId') or config.get('activityScope','configured-repositories')
    for (author,date),items in sorted(groups.items()):
        items.sort(key=lambda c:(c['authoredAt'],c['repositoryId'],c['sha']))
        repos=collections.Counter(c['repository'] for c in items)
        overview=f"作者：{author}；日期：{date}（上海时区，Git 作者时间）；去重非合并提交 {len(items)} 次，涉及 {len(repos)} 个仓库。\n"
        overview+='；'.join(f'{name}：{n} 次' for name,n in repos.items())
        if len(overview)>650:overview=overview[:600]+'…完整仓库列表见提交明细。'
        themes=list(dict.fromkeys(c['message'].splitlines()[0][:65] for c in items if c['message'].splitlines()))
        overview+='\n提交主题：'+'；'.join(themes[:5])+(f'；另有 {len(themes)-5} 个主题见明细' if len(themes)>5 else '')
        overview+='\n仅表示已观测到的 Git 提交活动，不等于全部工作或工时；没有提交记录不能推断没有工作。'
        lines=[overview]
        for c in items:
            lines += [f"# {c['repository']} · {c['sha']}",f"实际作者：{c['author']}；作者时间：{c['authoredAt']}；提交者时间：{c['committedAt']}",c['message'],
                      f"可索引文件新增 {c['additions']} / 删除 {c['deletions']} 行，{c['unknownStats']} 个文件行数未知。",'变更文件：\n'+'\n'.join(c['files'])]
        doc_id=str(uuid.uuid5(uuid.NAMESPACE_URL,json.dumps([config['projectId'],scope,'git_activity',author,date])))
        meta={'scope':scope,'author':author,'date':date,'aliases':sorted({author,*(c['author'] for c in items),*(c['email'] for c in items)}),'commits':len(items),'repositories':dict(repos)}
        evidence='\n\n'.join(lines[1:]);segments=[]
        # Small uploads keep embedding/storage within the normal authenticated API deadline.
        # Preserve every source character, preferring line boundaries so paths remain searchable.
        while evidence:
            edge=min(12000,len(evidence))
            if edge<len(evidence):
                newline=evidence.rfind('\n',0,edge)
                if newline>=edge//2:edge=newline+1
            segments.append(evidence[:edge]);evidence=evidence[edge:]
        for part,segment in enumerate(segments or ['']):
            multipart=len(segments)>1
            part_id=doc_id if part==0 else str(uuid.uuid5(uuid.UUID(doc_id),'part:'+str(part+1)))
            part_meta={**meta,**({'part':part+1,'parts':len(segments)} if multipart else {})}
            note=f'\n\n本作者当天记录分为 {len(segments)} 段；当前第 {part+1} 段。上方总量为当天去重总量，不能按分段累加。' if multipart else ''
            result.append({'projectId':config['projectId'],'documentId':part_id,'title':f'Git 每日活动 {author} {date}'+(f'（第{part+1}/{len(segments)}段）' if multipart else ''),
                           'text':overview+note+'\n\n'+segment,'sourceType':'git_activity','sourceId':json.dumps(part_meta,ensure_ascii=False),
                           'visibilityGroups':config.get('visibilityGroups',[])})
    return result

def publish(indexer,commits,save):
    path=indexer.root/'git-activity.json';c=indexer.c
    identity=[c['apiUrl'],c['projectId'],c.get('visibilityGroups',[]),c['embeddingRevision'],c.get('codeup',{}).get('sourceId') or c.get('activityScope','configured-repositories')]
    state=json.loads(path.read_text()) if path.exists() else {}
    if state and state.get('identity')!=identity:raise RuntimeError('Git activity audience/project identity changed; explicit migration required')
    state={**state,'identity':identity,'status':'indexing','documents':state.get('documents',{}),'startedAt':int(time.time())};save(path,state)
    documents=daily_documents(c,commits);live=set()
    try:
        for doc in documents:
            key=doc['documentId'];live.add(key)
            digest=hashlib.sha256(json.dumps(doc,ensure_ascii=False,sort_keys=True).encode()).hexdigest()
            if state['documents'].get(key,{}).get('digest')==digest:continue
            response=indexer.api('POST','/rag/document',doc)
            if not response.get('embedded'):raise RuntimeError('Git activity embedding unavailable')
            date=json.loads(doc['sourceId'])['date']
            state['documents'][key]={'digest':digest,'date':date,'chunks':response['chunks']};state['updatedAt']=int(time.time());save(path,state)
        since=cutoff(c).date().isoformat()
        # Retain older successfully observed daily facts; reconcile only the current scan window.
        for key,old in list(state['documents'].items()):
            if key not in live and old['date']>=since:
                indexer.api('DELETE','/rag/document/'+key);del state['documents'][key];save(path,state)
        state.update(status='complete',indexedDays=len(state['documents']),windowDays=len(documents),completedAt=int(time.time()));save(path,state)
    except Exception:
        state.update(status='failed',updatedAt=int(time.time()));save(path,state);raise
