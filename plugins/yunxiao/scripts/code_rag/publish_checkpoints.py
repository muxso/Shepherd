"""Publish completed code provenance without embedding unchanged files again."""
import argparse
import hashlib
import json
from pathlib import Path
import re
if __package__:
    from .index import Indexer, save
else:
    from index import Indexer, save


def browser_url(value):
    if value.startswith('https://'):
        return value.removesuffix('.git')
    match = re.fullmatch(r'git@([^:]+):(.+)', value)
    return 'https://' + match[1] + '/' + match[2].removesuffix('.git') if match else ''


def payload(config, spec, state):
    if state.get('status') != 'complete' or not re.fullmatch(r'[0-9a-fA-F]{40}|[0-9a-fA-F]{64}', state.get('commit', '')):
        raise RuntimeError('Repository scan is incomplete')
    identity = state.get('identity') or []
    if len(identity) < 4 or identity[:4] != [config['apiUrl'], config['projectId'], spec['url'], spec['branch']]:
        raise RuntimeError('Repository checkpoint identity differs from configuration')
    files = []
    for path, item in sorted(state.get('files', {}).items()):
        if not item.get('documentId') or not re.fullmatch(r'[0-9a-fA-F]{40}|[0-9a-fA-F]{64}', item.get('blob', '')):
            raise RuntimeError('Repository file provenance is incomplete')
        files.append({'path': path, 'documentId': item['documentId'], 'blob': item['blob'], 'contentCommit': item.get('commit', '')})
    return {'projectId': config['projectId'], 'repositoryId': str(spec['id']),
            'name': spec['name'], 'url': browser_url(spec['url']), 'branch': spec['branch'],
            'commitSha': state['commit'], 'files': files}


def publish(indexer, spec, state):
    body = payload(indexer.c, spec, state)
    result = indexer.api('POST', '/rag/code/checkpoint', body)
    if not result or not result.get('published') or result.get('files') != len(body['files']) or result.get('commitSha') != state['commit']:
        raise RuntimeError('Repository checkpoint publication was not confirmed')
    return result


def publish_activity(indexer, spec, commits):
    for start in range(0, len(commits), 200):
        records = []
        for commit in commits[start:start + 200]:
            if __package__:
                from .index import sanitize
            else:
                from index import sanitize
            message = sanitize((commit.get('message') or '').encode())
            records.append({**commit, 'message': message or '[REDACTED]', 'repositoryId': str(spec['id'])})
        result = indexer.api('POST', '/rag/code/activity', {'projectId': indexer.c['projectId'], 'repositoryId': str(spec['id']), 'commits': records})
        if not result or result.get('stored') != len(records):
            raise RuntimeError('Commit evidence upload was not confirmed')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', required=True)
    args = parser.parse_args()
    config = json.loads(Path(args.config).read_text())
    indexer = Indexer(config)
    repositories = json.loads(Path(config['repositoriesFile']).read_text())
    failures = 0
    for spec in repositories:
        key = hashlib.sha256(str(spec['id']).encode()).hexdigest()[:24]
        state_path = Path(config['stateDir']) / (key + '.json')
        try:
            state = json.loads(state_path.read_text())
            publish(indexer, spec, state)
            activity = Path(config['stateDir']) / 'activities' / (key + '.json')
            if activity.exists():
                publish_activity(indexer, spec, json.loads(activity.read_text()).get('commits', []))
            state['checkpointStatus'] = 'published'
            save(state_path, state)
            print(json.dumps({'repository': spec['name'], 'status': 'published', 'files': len(state['files'])}), flush=True)
        except Exception as error:
            failures += 1
            print(json.dumps({'repository': spec.get('name'), 'status': 'failed', 'error': type(error).__name__}), flush=True)
    if failures:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
