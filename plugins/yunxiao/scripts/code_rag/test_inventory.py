import importlib.util
import json
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('inventory', Path(__file__).with_name('codeup_inventory.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

class InventoryTests(unittest.TestCase):
    def setUp(self):
        guard = patch.dict('os.environ', {'YUNXIAO_ORG': 'org', 'YUNXIAO_TOKEN': 'test-only'})
        guard.start()
        self.addCleanup(guard.stop)

    def setup_config(self, root):
        env = root / 'env'
        env.write_text('YUNXIAO_ORG=org\nYUNXIAO_TOKEN=test-only\n')
        manifest = root / 'repositories.json'
        manifest.write_text(json.dumps([{'id': '3', 'name': 'existing', 'url': 'git@example:three', 'branch': 'main'}]))
        snapshot = root / 'snapshot.json'
        snapshot.write_text(json.dumps({'records': [{'kind': 'commit', 'data': {'repoId': 3}}]}))
        return {'projectId': 'project', 'repositoriesFile': str(manifest), 'codeup': {
            'sourceId': 'source', 'organizationId': 'org', 'envFile': str(env), 'devopsSnapshotFile': str(snapshot)}}

    def fake(self, path, query=None):
        if query:
            if query['page'] == 1:
                return [{'id': 1}, {'id': 2}], {'X-Total-Pages': '2'}
            return [{'id': 3}], {'x-total-pages': '2'}
        rid = path.rsplit('/', 1)[-1]
        return {'defaultBranch': '' if rid == '2' else 'main', 'pathWithNamespace': 'org/repo-' + rid,
                'sshUrlToRepo': 'git@example:' + rid}, {}

    def test_all_pages_empty_repositories_and_existing_priority(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = self.setup_config(root)
            result = m.refresh(config, self.fake)
            self.assertEqual((result['total'], result['indexable'], result['empty']), (3, 2, 1))
            self.assertEqual(result['missingDevopsRepositories'], [])
            manifest = Path(config['repositoriesFile'])
            self.assertEqual([row['id'] for row in json.loads(manifest.read_text())], ['3', '1'])
            self.assertEqual(manifest.stat().st_mode & 0o777, 0o600)
            self.assertEqual(json.loads((root / 'inventory.json').read_text())['devopsRepositories'], 1)

    def test_partial_failure_and_missing_devops_repo_keep_old_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = self.setup_config(root)
            manifest = Path(config['repositoriesFile'])
            before = manifest.read_bytes()
            def failing(path, query=None):
                if path.endswith('/3'):
                    raise RuntimeError('upstream unavailable')
                return self.fake(path, query)
            with self.assertRaises(RuntimeError):
                m.refresh(config, failing)
            self.assertEqual(manifest.read_bytes(), before)
            Path(config['codeup']['devopsSnapshotFile']).write_text(json.dumps({'records': [{'kind': 'commit', 'data': {'repoId': 99}}]}))
            with self.assertRaisesRegex(RuntimeError, 'not readable'):
                m.refresh(config, self.fake)
            self.assertEqual(manifest.read_bytes(), before)

    def test_full_source_inventory_and_previously_observed_refs_are_retained(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = self.setup_config(root)
            config['codeup']['requiredRepositoryIds'] = ['1']
            self.assertEqual(m.refresh(config, self.fake)['devopsRepositories'], 2)
            config['codeup']['requiredRepositoryIds'] = []
            Path(config['codeup']['devopsSnapshotFile']).write_text('{"records":[]}')
            self.assertEqual(m.refresh(config, self.fake)['devopsRepositories'], 2)
            config['codeup']['requiredRepositoryIds'] = ['99']
            with self.assertRaisesRegex(RuntimeError, 'not readable'):
                m.refresh(config, self.fake)

    def test_missing_pagination_header_reads_next_full_page(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = self.setup_config(root)
            Path(config['codeup']['devopsSnapshotFile']).write_text('{"records":[]}')
            def call(path, query=None):
                if query:
                    return ([{'id': n} for n in range(1,101)] if query['page'] == 1 else [{'id': 101}]), {}
                return self.fake(path)
            result = m.refresh(config, call)
            self.assertEqual(result['total'], 101)

if __name__ == '__main__':
    unittest.main()
