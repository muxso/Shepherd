#!/usr/bin/env python3
"""Read-only Git -> Shepherd RAG. Private JSON config and checkpoint; stdlib only."""
import argparse
import concurrent.futures
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import time
import threading
import urllib.error
import urllib.request
import uuid

EXTENSIONS = set('rs java kt kts py js jsx ts tsx vue svelte go c h cpp hpp cs rb php swift m mm scala sh bash sql proto graphql groovy xml html css scss less md rst txt toml yaml yml json properties gradle'.split())
EXCLUDED = set('.git node_modules target dist build vendor .idea .vscode coverage __pycache__ .next .nuxt .gradle .mvn test-output generated'.split())
SECRET_NAME = re.compile(r'(^|[._-])(secret|credential|password|keystore|id_rsa|id_ed25519)([._-]|$)', re.I)
SECRET_LINE = re.compile(r'(?i)(password|passwd|secret|api[_-]?key|access[_-]?key|private[_-]?key|token|authorization)["\']?\s*[=:]\s*["\']?[^\s"\']+')
SECRET_XML = re.compile(r'(?i)<(password|passwd|secret|token|privateKey|apiKey)>')
URL_CREDENTIAL = re.compile(r'://[^\s/:]+:[^\s@]+@')


def eligible(path, size):
    p = PurePosixPath(path)
    return (0 < size <= 262144 and not any(x.lower() in EXCLUDED for x in p.parts)
            and not p.name.startswith('.env') and not SECRET_NAME.search(p.name)
            and p.name not in ('package-lock.json', 'yarn.lock', 'Cargo.lock', 'pnpm-lock.yaml')
            and '.min.' not in p.name and p.suffix.lstrip('.').lower() in EXTENSIONS)


def sanitize(raw):
    if b'\0' in raw:
        return None
    try:
        text = raw.decode('utf-8')
    except UnicodeDecodeError:
        return None
    if '-----BEGIN ' in text and 'PRIVATE KEY-----' in text:
        return None
    # Preserve line count so citations still locate the original source accurately.
    return '\n'.join('[REDACTED credential line]' if any(p.search(l) for p in
                     (SECRET_LINE, SECRET_XML, URL_CREDENTIAL)) else l for l in text.splitlines())


def save(path, data):
    tmp = path.with_suffix('.tmp')
    tmp.write_text(json.dumps(data, ensure_ascii=False))
    tmp.chmod(0o600)
    tmp.replace(path)


def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args], stderr=subprocess.PIPE,
                                   timeout=900, env={**os.environ, 'GIT_TERMINAL_PROMPT': '0',
                                   'GIT_SSH_COMMAND': 'ssh -o BatchMode=yes -o ConnectTimeout=10'})


class Indexer:
    def __init__(self, config):
        self.c = config
        self.root = Path(config['stateDir'])
        self.root.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.root.chmod(0o700)
        self.auth_lock = threading.Lock()
        self.auth_at = 0

    def token(self):
        with self.auth_lock:
            if self.c.get('credentialsFile') and time.monotonic() - self.auth_at > 1800:
                credentials = json.loads(Path(self.c['credentialsFile']).read_text())
                req = urllib.request.Request(self.c['apiUrl'].rstrip('/') + '/auth/login',
                    data=json.dumps({k: credentials[k] for k in ('username', 'password')}).encode(),
                    headers={'Content-Type': 'application/json'})
                try:
                    with urllib.request.urlopen(req, timeout=30) as response:
                        fresh = json.load(response)['token']
                except Exception:
                    raise RuntimeError('Indexer login failed') from None
                path = Path(self.c['tokenFile'])
                temp = path.with_suffix('.tmp')
                temp.write_text(fresh)
                temp.chmod(0o600)
                temp.replace(path)
                self.auth_at = time.monotonic()
            return Path(self.c['tokenFile']).read_text().strip()

    def api(self, method, path, body=None):
        for attempt in range(4):
            token = self.token()
            req = urllib.request.Request(self.c['apiUrl'].rstrip('/') + path,
                    data=None if body is None else json.dumps(body).encode(), method=method,
                    headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
            try:
                with urllib.request.urlopen(req, timeout=600) as res:
                    raw = res.read()
                    return json.loads(raw) if raw else None
            except urllib.error.HTTPError as e:
                if e.code not in (429, 500, 502, 503, 504) or attempt == 3:
                    raise RuntimeError(f'Shepherd HTTP {e.code}') from None
            except (urllib.error.URLError, TimeoutError):
                if attempt == 3:
                    raise RuntimeError('Shepherd unavailable') from None
            time.sleep(2 ** attempt)

    def repo(self, spec):
        rid = str(spec['id'])
        # Never allow repository names/branches to become filesystem paths.
        key = hashlib.sha256(rid.encode()).hexdigest()[:24]
        state_path = self.root / (key + '.json')
        with (self.root / (key + '.lock')).open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            identity = [self.c['apiUrl'], self.c['projectId'], spec['url'], spec['branch'],
                        self.c.get('visibilityGroups', []), self.c['embeddingRevision'], 2]
            old = json.loads(state_path.read_text()) if state_path.exists() else {}
            if old and old.get('identity') != identity:
                previous = old.get('identity', [])
                if previous[:-1] == identity[:-1] and previous[-1:] == [1]:
                    # Format-only upgrade: retain stable ids for deletion tracking, re-embed all files.
                    for file in old.get('files', {}).values():
                        file['blob'] = ''
                    old['identity'] = identity
                    old['status'] = 'reindexing'
                    save(state_path, old)
                else:
                    raise RuntimeError('Index identity changed; use a fresh stateDir and explicitly remove old documents')
            repo = self.root / 'repos' / key
            repo.parent.mkdir(exist_ok=True)
            if not repo.exists():
                subprocess.run(['git', 'clone', '--bare', '--depth=1', '--single-branch', '--branch',
                    spec['branch'], '--', spec['url'], str(repo)], check=True,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=900,
                    env={**os.environ, 'GIT_TERMINAL_PROMPT': '0', 'GIT_SSH_COMMAND': 'ssh -o BatchMode=yes -o ConnectTimeout=10'})
                sha = git(repo, 'rev-parse', 'HEAD').decode().strip()
            else:
                git(repo, 'fetch', '--depth=1', 'origin', spec['branch'])
                sha = git(repo, 'rev-parse', 'FETCH_HEAD').decode().strip()
            state = {**old, 'identity': identity, 'status': 'indexing', 'targetCommit': sha,
                     'startedAt': int(time.time()), 'files': old.get('files', {}), 'repository': spec['name']}
            save(state_path, state)
            live = set()
            changed = skipped = 0
            # Read only committed regular blobs. No checkout, symlinks, submodules or repository hooks.
            for entry in git(repo, 'ls-tree', '-r', '-l', '-z', sha).split(b'\0'):
                if not entry:
                    continue
                meta, raw_path = entry.split(b'\t', 1)
                mode, kind, oid, size = meta.split()
                path = raw_path.decode('utf-8', errors='strict')
                if kind != b'blob' or mode not in (b'100644', b'100755') or not eligible(path, int(size)):
                    continue
                live.add(path)
                previous = state['files'].get(path)
                if previous and previous['blob'] == oid.decode():
                    skipped += 1
                    continue
                text = sanitize(git(repo, 'cat-file', 'blob', oid.decode()))
                if not text or not text.strip():
                    live.discard(path)
                    continue
                doc_id = str(uuid.uuid5(uuid.NAMESPACE_URL,
                    json.dumps([self.c['projectId'], rid, spec['branch'], path])))
                result = self.api('POST', '/rag/document', {
                    'projectId': self.c['projectId'], 'documentId': doc_id,
                    'title': f"{spec['name']} [{spec['branch']}] / {path} @{sha}",
                    'text': text, 'sourceType': 'code',
                    'sourceId': json.dumps({'repositoryId': rid, 'branch': spec['branch'], 'path': path, 'commit': sha}),
                    'visibilityGroups': self.c.get('visibilityGroups', [])})
                if not result.get('embedded'):
                    raise RuntimeError('Embedding unavailable; checkpoint not advanced')
                state['files'][path] = {'blob': oid.decode(), 'documentId': doc_id, 'chunks': result['chunks'], 'commit': sha}
                changed += 1
                state['indexedFiles'] = len(state['files'])
                state['updatedAt'] = int(time.time())
                save(state_path, state)
            # Only delete after a complete successful source traversal/ingest. Failures keep old evidence.
            for path in list(state['files']):
                if path not in live:
                    self.api('DELETE', '/rag/document/' + state['files'][path]['documentId'])
                    del state['files'][path]
                    save(state_path, state)
            state.update(status='complete', commit=sha, completedAt=int(time.time()),
                         indexedFiles=len(state['files']), changed=changed, unchanged=skipped)
            save(state_path, state)
            print(json.dumps({k: v for k, v in state.items() if k not in ('files', 'identity')}, ensure_ascii=False), flush=True)

    def run(self):
        if self.c.get('codeup'):
            if __package__:
                from .codeup_inventory import refresh
            else:
                from codeup_inventory import refresh
            print(json.dumps({'inventory': refresh(self.c)}), flush=True)
        repos = json.loads(Path(self.c['repositoriesFile']).read_text())
        def work(spec):
            try:
                self.repo(spec)
                return True
            except Exception as e:
                # No raw subprocess stderr, tokens or API response bodies in logs.
                print(json.dumps({'repository': spec.get('name'), 'status': 'failed',
                                  'error': type(e).__name__ + (': ' + str(e)[:160] if isinstance(e, RuntimeError) else '')}), flush=True)
                return False
        with concurrent.futures.ThreadPoolExecutor(max_workers=self.c.get('workers', 2)) as pool:
            results = list(pool.map(work, repos))
        summary = {'repositories': len(results), 'complete': sum(results), 'failed': len(results)-sum(results),
                   'updatedAt': int(time.time())}
        save(self.root / 'summary.json', summary)
        return 1 if summary['failed'] else 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', required=True)
    args = parser.parse_args()
    raise SystemExit(Indexer(json.loads(Path(args.config).read_text())).run())
