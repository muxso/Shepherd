"""Read-only, cached bug-to-requirement associations from Projex.
API: https://help.aliyun.com/en/yunxiao/developer-reference/listworkitemrelationrecords
"""
import concurrent.futures
import json
import os
from pathlib import Path
import time
import threading


def enrich_bug_relations(bugs, requirements, collector):
    path = Path(os.environ.get('SHEPHERD_BUG_RELATION_CACHE', str(Path.home() / '.cache/shepherd-yunxiao/bug-relations.json')))
    try:
        cache = json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        cache = {}
    now = time.time()
    ttl = max(60, int(os.environ.get('SHEPHERD_BUG_RELATION_CACHE_SECONDS', '3600')))
    org = collector.cfg()['YUNXIAO_ORG']
    known = {str(item['id']) for item in requirements}
    throttle = threading.Lock()
    next_call = [time.monotonic()]
    interval = 1 / max(1, min(10, float(os.environ.get('SHEPHERD_BUG_RELATION_QPS', '4'))))

    def load(item):
        key = f"{org}:{item['id']}"
        modified = str(item.get('gmtModified'))
        previous = cache.get(key, {})
        if previous.get('modified') == modified and now - previous.get('checkedAt', 0) < ttl:
            return key, previous
        with throttle:
            delay = max(0, next_call[0] - time.monotonic())
            next_call[0] = max(time.monotonic(), next_call[0]) + interval
        if delay: time.sleep(delay)
        rows, _ = collector.api(f"projex/organizations/{org}/workitems/{item['id']}/relationRecords?relationType=ASSOCIATED")
        if not isinstance(rows, list) or not all(isinstance(row, dict) and isinstance(row.get('resourceId'), str) for row in rows):
            raise RuntimeError('invalid bug relation response')
        references = sorted({row['resourceId'] for row in rows if row.get('resourceType') == 'WORKITEM'})
        return key, {'modified': modified, 'checkedAt': now, 'references': references}

    # No cache or snapshot publication unless all upstream relation reads succeed.
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
        updated = dict(executor.map(load, bugs))
    cache.update(updated)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(cache, ensure_ascii=False)); temporary.chmod(0o600); temporary.replace(path)
    output = []
    for bug in bugs:
        cached = updated[f"{org}:{bug['id']}"]
        references = set(cached['references']) | {str(bug.get('parentId') or '')}
        output.append(dict(bug, requirementIds=sorted(references & known), relationsCheckedAt=cached['checkedAt']))
    return output
