#!/usr/bin/env python3
"""Recollect code while retaining the exact successful non-code snapshot."""
import collections
import datetime as dt
import importlib
import json
import os
from pathlib import Path
import shutil
import sys
import yunxiao_sync as sync

def main():
    os.umask(0o077)
    if len(sys.argv)>1:sync.load_env(sys.argv[1])
    sys.path.insert(0,os.environ.get('SHEPHERD_COLLECTOR_PATH',str(Path.home()/'requirement-code-metrics')))
    collector=importlib.import_module('service');groups=importlib.import_module('report_groups')
    source=os.environ['SHEPHERD_SOURCE_ID'];sources=sync.request('GET','/api/devops/sources')['items'];info=next(s for s in sources if s['id']==source);config=collector.cfg()
    if (info['cloud_org_id'],info['cloud_project_id'])!=(config['YUNXIAO_ORG'],config['YUNXIAO_PROJECT']):raise RuntimeError('source organization/project mismatch')
    cache=Path(os.environ.get('SHEPHERD_SYNC_CACHE',str(Path.home()/'.cache/shepherd-yunxiao/snapshot.json')))
    snapshot=json.loads(cache.read_text());since=dt.datetime.fromisoformat(snapshot['codeSince']);until=dt.datetime.fromisoformat(snapshot['codeUntil'])
    if until-since!=dt.timedelta(days=90):raise RuntimeError('repair requires the successful 90-day snapshot')
    preserved=[r for r in snapshot['records'] if r['kind']!='commit']
    def raw(kind):return [r['data']['raw'] for r in preserved if r['kind']==kind]
    items=raw('requirement');bugs=raw('bug');sprints={r['externalId']:r['data']['raw'] for r in preserved if r['kind']=='sprint'}
    backup=cache.with_name('snapshot-before-pagination-repair.json')
    if not backup.exists():shutil.copy2(cache,backup)
    run=sync.request('POST',f'/api/devops/sources/{source}/runs',{})['id']
    try:
        print('Recollecting all code pages for the successful 90-day window',flush=True)
        commits=sync.collect_code(collector,groups,since,until,items,bugs,sprints)
        records=sync.deduplicate(preserved+commits);counts=dict.fromkeys((*sync.KINDS,'testcase'),0)
        for record in records:counts[record['kind']]+=1
        print('Complete code collection: '+str(len(commits))+' commits',flush=True)
        for batch in sync.batches(records):sync.request('PUT',f'/api/devops/runs/{run}/records',{'records':batch})
        result=sync.request('POST',f'/api/devops/runs/{run}/complete',{'counts':counts,'codeSince':snapshot['codeSince'],'codeUntil':snapshot['codeUntil']})
        updated={**snapshot,'records':records};temp=cache.with_suffix('.repair.tmp');temp.write_text(json.dumps(updated,ensure_ascii=False));temp.chmod(0o600);temp.replace(cache)
        print(json.dumps({'status':'complete','runId':run,'counts':counts,'result':result},ensure_ascii=False),flush=True)
    except Exception:
        sync.request('POST',f'/api/devops/runs/{run}/fail',{'error':'Code pagination repair failed; previous successful snapshot retained.'})
        raise
if __name__=='__main__':main()
