"""Read-only Bug reopen history from the Projex workitem activities API.
https://help.aliyun.com/zh/yunxiao/developer-reference/listworkitemactivities
"""
import concurrent.futures
import datetime as dt
import json
import os
from pathlib import Path
import threading
import time

RESOLVED = {'已解决', '已验证', '已发布', '已关闭', '关闭（已验证）', '已完成', 'resolved', 'fixed', 'closed', 'done'}
ACTIVE = {'待确认', '待处理', '修复中', '处理中', '待修复', '延期处理', 'open', 'new', 'active', 'in progress'}
REOPEN = {'再次打开', '重新打开', '重新激活', '重开', '已重开', '已激活', 'reopen', 'reopened'}
OTHER = {'已取消', '已拒绝', '已拒绝（不修复）', '不予修复', '非缺陷', '重复缺陷', '设计如此', '无法重现'}


def event_time(value):
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return dt.datetime.fromtimestamp(value / 1000, dt.timezone.utc).isoformat()
    if isinstance(value, str):
        parsed = dt.datetime.fromisoformat(value.replace('Z', '+00:00'))
        if parsed.tzinfo is not None:
            return parsed.isoformat()
    raise ValueError('activity timestamp requires a timezone')


def project_history(rows, item_id):
    if not isinstance(rows, list) or not all(isinstance(row, dict) for row in rows):
        raise RuntimeError('invalid Bug activity response')
    complete = any(row.get('eventType') == 'workitem.created' and str(row.get('resourceId')) == str(item_id) for row in rows)
    events, transitions, resolutions = {}, {}, {}
    for row in rows:
        if row.get('eventType') != 'workitem.transitioned':
            continue
        if str(row.get('resourceId')) != str(item_id):
            raise RuntimeError('Bug activity resource mismatch')
        old, new = row.get('oldValue'), row.get('newValue')
        if not isinstance(old, list) or not isinstance(new, list) or len(old) != 1 or len(new) != 1:
            raise RuntimeError('invalid Bug status transition')
        before, after = old[0].get('displayValue', '').strip().lower(), new[0].get('displayValue', '').strip().lower()
        known = RESOLVED | ACTIVE | REOPEN | OTHER
        if before not in known or after not in known:
            complete = False
        if before != after:
            identity = row.get('eventId')
            if identity is None or isinstance(identity, bool):
                raise RuntimeError('Bug activity has no event identifier')
            event = {'id': str(identity), 'at': event_time(row.get('eventTime'))}
            transition = {**event, 'from': old[0]['displayValue'].strip(), 'to': new[0]['displayValue'].strip()}
            if event['id'] in transitions and transitions[event['id']] != transition:
                raise RuntimeError('conflicting Bug activities')
            transitions[event['id']] = transition
            if after in REOPEN or before in RESOLVED and after in ACTIVE:
                events[event['id']] = event
            if after in RESOLVED:
                resolutions[event['id']] = event
    ordered = lambda values: sorted(values, key=lambda e: (e['at'], e['id']))
    return {'schemaVersion': 2, 'complete': complete, 'events': ordered(events.values()), 'resolutions': ordered(resolutions.values()), 'transitions': ordered(transitions.values())}


def enrich_bug_history(bugs, collector):
    path = Path(os.environ.get('SHEPHERD_BUG_HISTORY_CACHE', str(Path.home() / '.cache/shepherd-yunxiao/bug-history-v2.json')))
    try:
        cache = json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        cache = {}
    now = time.time()
    ttl = max(60, int(os.environ.get('SHEPHERD_BUG_HISTORY_CACHE_SECONDS', '86400')))
    org = collector.cfg()['YUNXIAO_ORG']
    throttle, next_call = threading.Lock(), [time.monotonic()]
    interval = 1 / max(1, min(10, float(os.environ.get('SHEPHERD_BUG_HISTORY_QPS', '4'))))

    def load(item):
        key, modified = f"{org}:{item['id']}", str(item.get('gmtModified'))
        previous = cache.get(key, {})
        if modified not in ('None', '') and previous.get('modified') == modified and previous.get('history', {}).get('schemaVersion') == 2 and previous.get('history', {}).get('complete') and now - previous.get('checkedAt', 0) < ttl:
            return key, previous
        with throttle:
            delay = max(0, next_call[0] - time.monotonic())
            next_call[0] = max(time.monotonic(), next_call[0]) + interval
        if delay:
            time.sleep(delay)
        rows, _ = collector.api(f"projex/organizations/{org}/workitems/{item['id']}/activities")
        return key, {'modified': modified, 'checkedAt': now, 'history': project_history(rows, item['id'])}

    # A failed upstream read never publishes a partial snapshot or replaces the cache.
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
        updated = dict(executor.map(load, bugs))
    cache.update(updated)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(cache, ensure_ascii=False)); temporary.chmod(0o600); temporary.replace(path)
    checked = dt.datetime.now(dt.timezone.utc).isoformat()
    return [dict(item, reopenHistory={**updated[f"{org}:{item['id']}"]['history'], 'checkedAt': checked}) for item in bugs]
