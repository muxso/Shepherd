"""Read-only language inventory from cached default-branch source trees."""
import collections
import hashlib
import os
from pathlib import Path, PurePosixPath
import subprocess

LANGUAGES = {'java':'Java','kt':'Kotlin','kts':'Kotlin','js':'JavaScript','jsx':'JavaScript',
 'ts':'TypeScript','tsx':'TypeScript','vue':'Vue','svelte':'Svelte','rs':'Rust','py':'Python',
 'go':'Go','c':'C','h':'C/C++','cc':'C++','cpp':'C++','hpp':'C++','cs':'C#','swift':'Swift',
 'm':'Objective-C','mm':'Objective-C++','php':'PHP','rb':'Ruby','scala':'Scala','dart':'Dart',
 'sh':'Shell','bash':'Shell','sql':'SQL','html':'HTML','htm':'HTML','css':'CSS','scss':'SCSS',
 'less':'Less','groovy':'Groovy'}
EXCLUDE = set('.git node_modules vendor target dist build coverage generated .next .nuxt .gradle __pycache__'.split())


def tree_languages(raw):
    sizes = collections.Counter()
    for entry in raw.split(b'\0'):
        if not entry:
            continue
        meta, raw_path = entry.split(b'\t', 1)
        mode, kind, oid, size = meta.split()
        if mode not in (b'100644', b'100755') or kind != b'blob' or not size.isdigit():
            continue
        path = PurePosixPath(raw_path.decode('utf-8', errors='replace'))
        if any(p.lower() in EXCLUDE for p in path.parts[:-1]) or '.min.' in path.name:
            continue
        language = LANGUAGES.get(path.suffix.lstrip('.').lower())
        if language:
            sizes[language] += int(size)
    return [{'name': name, 'bytes': size} for name, size in sorted(sizes.items(), key=lambda item: (-item[1], item[0])) if size]


def repository_language(repository_id, root=None):
    root = Path(root or os.environ.get('SHEPHERD_CODE_REPO_CACHE', str(Path.home() / 'shepherd-code-rag')))
    rid = str(repository_id)
    candidates = [root / 'state/repos' / hashlib.sha256(rid.encode()).hexdigest()[:24]]
    if rid.isdigit():
        candidates.append(root / 'repos' / rid)
    repo = next((path for path in candidates if path.is_dir()), None)
    if repo is None:
        return {'repositoryLanguages': [], 'repositoryLanguageSource': 'unavailable'}
    def git(*args):
        return subprocess.check_output(['git', '-C', str(repo), *args], stderr=subprocess.PIPE,
             timeout=30, env={**os.environ, 'GIT_OPTIONAL_LOCKS': '0'})
    try:
        try:
            sha = git('rev-parse', '--verify', 'FETCH_HEAD').decode().strip()
        except subprocess.CalledProcessError:
            sha = git('rev-parse', '--verify', 'HEAD').decode().strip()
        values = tree_languages(git('ls-tree', '-r', '-l', '-z', sha))
        return {'repositoryLanguages': [v['name'] for v in values],
                'repositoryLanguageBytes': values, 'repositoryLanguageSource': 'git-default-branch-extensions',
                'repositoryLanguageCommit': sha}
    except (subprocess.SubprocessError, OSError, ValueError):
        return {'repositoryLanguages': [], 'repositoryLanguageSource': 'unavailable'}
