#!/usr/bin/env python3
"""Synchronize Yunxiao facts into Shepherd. No writes to Yunxiao or messages.
Reuses the tested read-only collector installed for the n8n report; credentials
remain in ~/.config/devops-weekly/env. See docs/YUNXIAO.zh-CN.md in the Shepherd repository.
"""
import collections
import concurrent.futures
import datetime as dt
import fcntl
import importlib
import json
import os
from pathlib import Path
import sys
import urllib.error
import urllib.request

KINDS = ('requirement', 'bug', 'sprint', 'commit')
from yunxiao_testhub import enrich_requirements, collect_testcases

def load_env(path):
    for line in Path(path).read_text().splitlines():
        if line.strip() and not line.lstrip().startswith('#') and '=' in line:
            k, v = line.split('=', 1)
            os.environ.setdefault(k.strip(), v.strip().strip('"').strip("'"))

def request(method, path, body=None):
    raw = None if body is None else json.dumps(body, ensure_ascii=False).encode()
    req = urllib.request.Request(os.environ['SHEPHERD_URL'].rstrip('/') + path, data=raw, method=method,
        headers={'Authorization': 'Bearer ' + os.environ['SHEPHERD_SYNC_KEY'], 'Content-Type': 'application/json'})
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        with opener.open(req, timeout=600 if path.endswith("/complete") else 120) as r:
            return json.load(r)
    except urllib.error.HTTPError as e:
        # Do not log keys or source payloads.
        raise RuntimeError(f'Shepherd HTTP {e.code} on {method} {path}: {e.read(250).decode(errors="replace")}') from None

def batches(records, max_bytes=1_400_000):
    batch = []; size = 20
    for r in records:
        n = len(json.dumps(r, ensure_ascii=False).encode()) + 2
        if n > max_bytes:
            raise ValueError('source record exceeds ingest batch size')
        if batch and (len(batch) >= 100 or size + n > max_bytes):
            yield batch; batch = []; size = 20
        batch.append(r); size += n
    if batch:
        yield batch

def deduplicate(records):
    """Cloud pagination can repeat an entity while work items are being updated."""
    unique = {}
    def updated(record):
        value = record['data'].get('updatedAt') or record['data'].get('authoredAt')
        try: return dt.datetime.fromisoformat(value.replace('Z','+00:00')).timestamp()
        except (TypeError, ValueError, AttributeError): return 0
    for record in records:
        key = (record['kind'], record['externalId'])
        if key not in unique or updated(record) >= updated(unique[key]):
            unique[key] = record
    return list(unique.values())

def normalize_workitem(item, kind, collector, groups, sprint_map):
    name = collector.pname
    def iso(value):
        x = collector.dateval(value)
        return x.isoformat() if x else ''
    fields = {f.get('fieldName'): f for f in item.get('customFieldValues') or []}
    due = []
    sprint = item.get('sprint') or {}
    if not isinstance(sprint, dict): sprint = {}
    sd = sprint_map.get(str(sprint.get('id') or ''), {})
    value = collector.dateval(sd.get('endDate'))
    if value: due.append((value, '迭代结束日期'))
    for field_name in ('计划完成时间','计划完成日期','截止日期','期望完成时间','产品完成时间'):
        for v in fields.get(field_name, {}).get('values') or []:
            value = collector.dateval(v.get('identifier') or v.get('displayValue')) if isinstance(v,dict) else collector.dateval(v)
            if value: due.append((value, field_name))
    end, due_source = min(due, key=lambda x:x[0]) if due else (None, '')
    owner = name(item.get('assignedTo'))
    status = item.get('status') or {}
    data = {'serial':item.get('serialNumber') or str(item['id']), 'title':item.get('subject') or '',
        'status':status.get('displayName') or status.get('name') or '',
        'priority':groups.priority_name(item), 'owner':owner, 'group':groups.group_for(owner),
        'sprintId':str(sprint.get('id') or ''), 'sprint':sd.get('name') or name(sprint),
        'parentId':str(item.get('parentId') or ''), 'due':end.strftime('%Y-%m-%d') if end else '',
        'dueSource':due_source, 'createdAt':iso(item.get('gmtCreate')), 'updatedAt':iso(item.get('gmtModified')),
        'statusChangedAt':iso(item.get('updateStatusAt')), 'raw':item}
    return {'kind':kind, 'externalId':str(item['id']), 'data':data}

def collect(collector, groups, days):
    c = collector.cfg()
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as ex:
        fs = [ex.submit(collector.workitems,c,'Req'), ex.submit(collector.workitems,c,'Bug'), ex.submit(collector.sprints,c)]
        items, bugs, sprints = [f.result() for f in fs]
    items = enrich_requirements(items, collector)
    repo_ids = [x.strip() for x in os.environ.get('SHEPHERD_TEST_REPO_IDS','').split(',') if x.strip()]
    testcases = collect_testcases(collector, repo_ids)
    # All source requests must succeed before a complete snapshot may be published.
    records = [normalize_workitem(x,'requirement',collector,groups,sprints) for x in items]
    records += [normalize_workitem(x,'bug',collector,groups,sprints) for x in bugs]
    serials = {str(x['id']):x.get('serialNumber') or str(x['id']) for x in items}
    for r in records:
        r['data']['parentSerial'] = serials.get(r['data'].get('parentId'), '')
    for sid, sprint in sprints.items():
        records.append({'kind':'sprint','externalId':sid,'data':{'title':sprint.get('name') or '', 'status':str(sprint.get('status') or ''), 'startAt':(collector.dateval(sprint.get('startDate')).isoformat() if collector.dateval(sprint.get('startDate')) else ''), 'endAt':(collector.dateval(sprint.get('endDate')).isoformat() if collector.dateval(sprint.get('endDate')) else ''), 'raw':sprint}})
    until = dt.datetime.now(collector.TZ).replace(second=0,microsecond=0)
    since = until-dt.timedelta(days=days)
    # Cache the already-completed source snapshot for the collector's associations.
    old_workitems, old_sprints, old_ci = collector.workitems, collector.sprints, collector.flow_ci
    collector.workitems = lambda c,category='Req': items if category=='Req' else bugs
    collector.sprints = lambda c:sprints
    collector.flow_ci = lambda c,a,b:{'totals':{'passRate':None,'runs':0,'running':0,'passed':0,'failed':0,'pipelineErrors':0},'pipelines':[]}
    try: report = collector.report(since,until,[])
    finally: collector.workitems,collector.sprints,collector.flow_ci = old_workitems,old_sprints,old_ci
    if report['totals'].get('branchScanErrors'):
        raise RuntimeError('Codeup branch scan incomplete; snapshot will not be published')
    for cm in report['commitsDetail']:
        if 'requirementIds' not in cm:
            raise RuntimeError('collector requires requirementIds extension; see deployment guide')
        stats = cm.get('stats') or {}
        records.append({'kind':'commit','externalId':str(cm['repo_id'])+':'+cm['sha'], 'data':{
            'title':cm.get('title') or '', 'author':groups.canonical_name(cm['author']),
            'authorRaw':cm['author'],'email':cm.get('email') or '', 'group':groups.group_for(cm['author']),
            'repoId':cm['repo_id'],'repo':cm['repo'],'sha':cm['sha'], 'authoredAt':cm['date'],
            'additions':stats.get('additions'),'deletions':stats.get('deletions'),
            'requirementIds':cm['requirementIds'],'isMerge':False,
            'parentIds':cm.get('parentIds') or [],'url':cm.get('url') or ''}})
    records.extend(testcases)
    unique = deduplicate(records)
    if len(unique) != len(records): print(f"Cloud pagination duplicates deduplicated: {len(records)-len(unique)}", flush=True)
    return unique,since.isoformat(),until.isoformat()

def main():
    if len(sys.argv)>1:load_env(sys.argv[1])
    collector_path=os.environ.get('SHEPHERD_COLLECTOR_PATH',str(Path.home() / 'requirement-code-metrics'))
    sys.path.insert(0,collector_path)
    collector=importlib.import_module('service');groups=importlib.import_module('report_groups')
    days=max(1,min(366,int(os.environ.get('SHEPHERD_SYNC_DAYS','7'))))
    source=os.environ['SHEPHERD_SOURCE_ID']
    sources=request('GET','/api/devops/sources')['items']
    s=next(x for x in sources if x['id']==source)
    c=collector.cfg()
    if (s['cloud_org_id'],s['cloud_project_id'])!=(c['YUNXIAO_ORG'],c['YUNXIAO_PROJECT']):
        raise RuntimeError('collector project differs from configured source')
    run=request('POST',f'/api/devops/sources/{source}/runs',{})['id']
    try:
        records,since,until=collect(collector,groups,days)
        # Keep a private replay snapshot so the exact same run can be verified for idempotency.
        snapshot={'records':records,'codeSince':since,'codeUntil':until}
        cache=Path(os.environ.get('SHEPHERD_SYNC_CACHE',str(Path.home() / '.cache/shepherd-yunxiao/snapshot.json')))
        cache.parent.mkdir(mode=0o700,parents=True,exist_ok=True)
        temp=cache.with_suffix('.tmp');temp.write_text(json.dumps(snapshot,ensure_ascii=False));temp.chmod(0o600);temp.replace(cache)
        counts=dict.fromkeys(KINDS,0)
        if os.environ.get("SHEPHERD_TEST_REPO_IDS"): counts["testcase"]=0
        for r in records: counts[r['kind']]+=1
        for batch in batches(records):request('PUT',f'/api/devops/runs/{run}/records',{'records':batch})
        result=request('POST',f'/api/devops/runs/{run}/complete',{'counts':counts,'codeSince':since,'codeUntil':until})
        print(json.dumps(result,ensure_ascii=False),flush=True)
    except Exception as e:
        # Avoid propagating upstream bodies which may contain identifiers or credentials.
        try:request('POST',f'/api/devops/runs/{run}/fail',{'error':f'{type(e).__name__}: sync failed; inspect private service logs'})
        except Exception:pass
        raise

if __name__=='__main__':
    lock=(Path.home() / '.cache/shepherd-yunxiao/sync.lock');lock.parent.mkdir(mode=0o700,parents=True,exist_ok=True)
    with lock.open('w') as f:
        try:fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:raise SystemExit('sync already running')
        main()
