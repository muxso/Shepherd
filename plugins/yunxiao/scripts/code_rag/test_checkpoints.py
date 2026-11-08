import unittest
if __package__:
    from . import publish_checkpoints as cp
else:
    import publish_checkpoints as cp


class CheckpointTests(unittest.TestCase):
    def test_stable_file_provenance_and_browser_url(self):
        config = {'apiUrl': 'http://localhost', 'projectId': 'p'}
        spec = {'id': 1, 'name': 'service', 'url': 'git@codeup.aliyun.com:org/service.git', 'branch': 'main'}
        state = {'status': 'complete', 'identity': ['http://localhost', 'p', spec['url'], 'main', 'other'],
                 'commit': 'a' * 40, 'files': {'a.py': {'documentId': 'doc', 'blob': 'b' * 40, 'commit': 'c' * 40}}}
        result = cp.payload(config, spec, state)
        self.assertEqual(result['url'], 'https://codeup.aliyun.com/org/service')
        self.assertEqual(result['commitSha'], 'a' * 40)
        self.assertEqual(result['files'], [{'path': 'a.py', 'documentId': 'doc', 'blob': 'b' * 40, 'contentCommit': 'c' * 40}])
        state['status'] = 'indexing'
        with self.assertRaises(RuntimeError): cp.payload(config, spec, state)
        state['status'] = 'complete'; state['identity'][1] = 'other project'
        with self.assertRaises(RuntimeError): cp.payload(config, spec, state)

    def test_publication_requires_confirmed_count_and_sha(self):
        class Indexer:
            c = {'apiUrl': 'http://localhost', 'projectId': 'p'}
            def api(self, method, path, body): return {'published': True, 'files': 3, 'commitSha': 'a' * 40}
        spec = {'id': 1, 'name': 'service', 'url': 'https://example.com/service.git', 'branch': 'main'}
        state = {'status': 'complete', 'identity': ['http://localhost', 'p', spec['url'], 'main'], 'commit': 'a' * 40, 'files': {}}
        with self.assertRaises(RuntimeError): cp.publish(Indexer(), spec, state)
