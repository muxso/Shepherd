"""Complete Codeup pagination without trusting advisory total-page headers."""
import datetime as dt
import threading

_lock=threading.Lock()
_pages=0
_scans=0

def list_commits(collector,repo,branch,start,end,config,max_pages=1000):
    if not branch:return []
    path=f"codeup/organizations/{config['YUNXIAO_ORG']}/repositories/{repo['id']}/commits"
    query={'refName':branch,'since':start.astimezone(dt.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),'until':end.astimezone(dt.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),'perPage':100}
    result=[];seen_pages=set();seen_commits=set()
    global _pages,_scans
    for page in range(1,max_pages+1):
        data,_=collector.api(path,query={**query,'page':page})
        rows=collector.rows(data)
        if not isinstance(rows,list):raise RuntimeError('unexpected commit page')
        with _lock:
            _pages+=1
            if _pages%100==0:print(f'Codeup commit pages read: {_pages}',flush=True)
        if not rows:
            with _lock:
                _scans+=1
                if _scans%50==0:print(f'Codeup commit scans completed: {_scans}',flush=True)
            return result
        fingerprint=tuple(str(row.get('id') or '') for row in rows)
        if not all(fingerprint) or fingerprint in seen_pages:raise RuntimeError('commit pagination repeated a page or omitted a SHA')
        seen_pages.add(fingerprint)
        for row in rows:
            if row['id'] not in seen_commits:seen_commits.add(row['id']);result.append(row)
    raise RuntimeError('commit pagination exceeded the safety limit')
