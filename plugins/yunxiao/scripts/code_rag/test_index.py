import importlib.util
import unittest
import tempfile
import subprocess
import json
from pathlib import Path

spec = importlib.util.spec_from_file_location('index', Path(__file__).with_name('index.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

class PolicyTests(unittest.TestCase):
    def test_excludes_secrets_builds_binary_and_large_files(self):
        for path in ('.env', '.env.production', 'src/password.json', 'node_modules/a.js',
                     'target/a.rs', 'src/id_rsa.txt', 'package-lock.json', 'a.min.js', 'a.png'):
            self.assertFalse(m.eligible(path, 50), path)
        self.assertFalse(m.eligible('a.rs', 300000))
        self.assertTrue(m.eligible('src/service/OrderService.java', 500))

    def test_redaction_keeps_line_numbers_and_rejects_private_keys(self):
        self.assertEqual(m.sanitize(b'a\npassword=secret1234\nb').splitlines(),
                         ['a', '[REDACTED credential line]', 'b'])
        self.assertIsNone(m.sanitize(b'-----BEGIN RSA PRIVATE KEY-----\nabc'))
        self.assertIsNone(m.sanitize(b'\x00binary'))
        self.assertIsNone(m.sanitize(b'\xff'))

class IncrementalTests(unittest.TestCase):
    def test_retry_idempotence_and_deleted_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'source'
            subprocess.run(['git', 'init', '-q', '-b', 'main', str(source)], check=True)
            def commit():
                subprocess.run(['git', '-C', str(source), 'add', '-A'], check=True)
                subprocess.run(['git', '-C', str(source), '-c', 'user.name=Test', '-c',
                                'user.email=test@example.invalid', 'commit', '-qm', 'test'], check=True)
            (source / 'a.rs').write_text('fn main() {}')
            commit()
            calls = []
            class Fake(m.Indexer):
                fail = False
                def api(self, method, path, body=None):
                    calls.append((method, path, body))
                    if self.fail:
                        raise RuntimeError('provider unavailable')
                    return {'embedded': True, 'chunks': 1}
            index = Fake({'stateDir': str(root / 'state'), 'apiUrl': 'http://test',
                          'projectId': 'p', 'embeddingRevision': 'model-v1'})
            spec = {'id': 'r', 'name': 'repo', 'url': str(source), 'branch': 'main'}
            index.repo(spec)
            self.assertEqual(len(calls), 1)
            index.repo(spec)
            self.assertEqual(len(calls), 1)  # unchanged files do not re-embed
            (source / 'a.rs').unlink()
            (source / 'b.rs').write_text('fn b() {}')
            commit()
            index.fail = True
            with self.assertRaises(RuntimeError):
                index.repo(spec)
            self.assertFalse(any(c[0] == 'DELETE' for c in calls))
            index.fail = False
            index.repo(spec)
            self.assertEqual(sum(c[0] == 'DELETE' for c in calls), 1)
            checkpoint = next((root / 'state').glob('*.json'))
            self.assertEqual(list(json.loads(checkpoint.read_text())['files']), ['b.rs'])
            index.c['projectId'] = 'other'
            with self.assertRaises(RuntimeError):
                index.repo(spec)

if __name__ == '__main__':
    unittest.main()
