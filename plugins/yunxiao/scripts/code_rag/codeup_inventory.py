"""Refresh all Codeup repositories bound to a Yunxiao source; no upstream writes."""
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request

BASE = 'https://openapi-rdc.aliyuncs.com/oapi/v1/'


def save(path, value):
    path = Path(path)
    temp = path.with_suffix('.tmp')
    temp.write_text(json.dumps(value, ensure_ascii=False))
    temp.chmod(0o600)
    temp.replace(path)


def environment(path):
    values = dict(os.environ)
    for line in Path(path).read_text().splitlines():
        if line.strip() and not line.lstrip().startswith('#') and '=' in line:
            key, value = line.split('=', 1)
            values.setdefault(key.strip(), value.strip().strip('"').strip("'"))
    return values


def cloud_call(token):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    def call(path, query=None):
        url = BASE + path
        if query:
            url += '?' + urllib.parse.urlencode(query)
        for attempt in range(5):
            request = urllib.request.Request(url, headers={'x-yunxiao-token': token, 'Accept': 'application/json'})
            try:
                with opener.open(request, timeout=60) as response:
                    return json.load(response), dict(response.headers)
            except urllib.error.HTTPError as error:
                if error.code not in (429, 500, 502, 503, 504) or attempt == 4:
                    raise RuntimeError(f'Codeup inventory HTTP {error.code}; previous manifest preserved') from None
            except (urllib.error.URLError, TimeoutError):
                if attempt == 4:
                    raise RuntimeError('Codeup inventory unavailable; previous manifest preserved') from None
            time.sleep(2 ** attempt)
    return call


def items(data):
    if isinstance(data, list):
        return data
    if isinstance(data, dict):
        for key in ('data', 'items', 'repositories'):
            if isinstance(data.get(key), list):
                return data[key]
    raise RuntimeError('Unexpected Codeup repository list; previous manifest preserved')


def refresh(config, call=None):
    settings = config['codeup']
    env = environment(settings['envFile'])
    org = settings['organizationId']
    if env.get('YUNXIAO_ORG') != org or not env.get('YUNXIAO_TOKEN'):
        raise RuntimeError('Codeup credentials differ from the bound Yunxiao organization')
    if call is None:
        call = cloud_call(env['YUNXIAO_TOKEN'])
    manifest_path = Path(config['repositoriesFile'])
    old = json.loads(manifest_path.read_text()) if manifest_path.exists() else []
    path = f'codeup/organizations/{urllib.parse.quote(org, safe="")}/repositories'
    repositories = {}
    seen_pages = set()
    for page in range(1, 1001):
        data, headers = call(path, {'page': page, 'perPage': 100})
        rows = items(data)
        if any(not isinstance(row, dict) or not row.get('id') for row in rows):
            raise RuntimeError('Codeup repository missing id; previous manifest preserved')
        signature = tuple(str(row['id']) for row in rows)
        if signature and signature in seen_pages:
            raise RuntimeError('Repeated Codeup page; previous manifest preserved')
        seen_pages.add(signature)
        for row in rows:
            repositories[str(row['id'])] = row
        headers = {str(k).lower(): v for k, v in headers.items()}
        total = headers.get('x-total-pages')
        if total is not None:
            if page >= max(1, int(total)):
                break
        elif len(rows) < 100:
            break
    else:
        raise RuntimeError('Codeup pagination limit exceeded; previous manifest preserved')
    if not repositories and old:
        raise RuntimeError('Empty Codeup inventory; previous manifest preserved')

    def detail(repository_id):
        data, _ = call(path + '/' + urllib.parse.quote(repository_id, safe=''))
        if not isinstance(data, dict) or 'defaultBranch' not in data or not data.get('pathWithNamespace'):
            raise RuntimeError('Incomplete Codeup detail; previous manifest preserved')
        branch = data['defaultBranch']
        if branch is not None and not isinstance(branch, str):
            raise RuntimeError('Invalid Codeup branch; previous manifest preserved')
        if branch and not data.get('sshUrlToRepo'):
            raise RuntimeError('Codeup clone URL unavailable; previous manifest preserved')
        return {'id': repository_id, 'name': data['pathWithNamespace'],
                'branch': branch or '', 'url': data.get('sshUrlToRepo', ''),
                'state': 'ready' if branch else 'empty'}

    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        inventory = list(pool.map(detail, repositories))
    required = set()
    snapshot_path = settings.get('devopsSnapshotFile')
    if snapshot_path:
        snapshot = json.loads(Path(snapshot_path).read_text())
        if not isinstance(snapshot.get('records'), list):
            raise RuntimeError('Invalid Devops snapshot; previous manifest preserved')
        for record in snapshot['records']:
            if record.get('kind') == 'commit':
                repository_id = record.get('data', {}).get('repoId')
                if repository_id is None:
                    raise RuntimeError('Devops commit missing repository id; previous manifest preserved')
                required.add(str(repository_id))
    missing = required - repositories.keys()
    if missing:
        raise RuntimeError(f'{len(missing)} Devops repositories are not readable in Codeup; previous manifest preserved')
    order = {str(row['id']): index for index, row in enumerate(old)}
    ready = [{k: row[k] for k in ('id', 'name', 'url', 'branch')} for row in inventory if row['state'] == 'ready']
    ready.sort(key=lambda row: (order.get(row['id'], len(order)), row['name']))
    summary = {'sourceId': settings['sourceId'], 'projectId': config['projectId'], 'organizationId': org,
               'checkedAt': int(time.time()), 'repositories': inventory,
               'total': len(inventory), 'indexable': len(ready),
               'empty': sum(row['state'] == 'empty' for row in inventory),
               'devopsRepositories': len(required), 'missingDevopsRepositories': []}
    # Publish only after every page/detail and the Devops coverage audit succeeded.
    save(manifest_path, ready)
    summary['manifestSha256'] = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    save(manifest_path.with_name('inventory.json'), summary)
    return {key: value for key, value in summary.items() if key != 'repositories'}
