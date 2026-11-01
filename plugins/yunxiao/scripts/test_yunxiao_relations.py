import json, os, tempfile, types, unittest
from pathlib import Path
from unittest.mock import patch
from yunxiao_relations import enrich_bug_relations

class RelationTests(unittest.TestCase):
    def test_explicit_associations_parent_deduplication_cache_and_refresh(self):
        with tempfile.TemporaryDirectory() as directory:
            calls=[]
            def api(path):
                calls.append(path)
                return ([{'resourceId':'req-2','resourceType':'WORKITEM'}, {'resourceId':'req-1','resourceType':'WORKITEM'}, {'resourceId':'another-bug','resourceType':'WORKITEM'}, {'resourceId':'file','resourceType':'FILE'}],{})
            collector=types.SimpleNamespace(cfg=lambda:{'YUNXIAO_ORG':'org'},api=api)
            bugs=[{'id':'bug','parentId':'req-1','gmtModified':1}]
            requirements=[{'id':'req-1'},{'id':'req-2'}]
            path=str(Path(directory)/'relations.json')
            with patch.dict(os.environ,{'SHEPHERD_BUG_RELATION_CACHE':path}),patch('yunxiao_relations.time.sleep'):
                first=enrich_bug_relations(bugs,requirements,collector)
                self.assertEqual(first[0]['requirementIds'],['req-1','req-2'])
                self.assertNotIn('requirementIds',bugs[0])
                enrich_bug_relations(bugs,requirements,collector)
                self.assertEqual(len(calls),1)
                bugs[0]['gmtModified']=2
                enrich_bug_relations(bugs,requirements,collector)
                self.assertEqual(len(calls),2)
                self.assertEqual(Path(path).stat().st_mode & 0o777,0o600)
    def test_failed_fetch_does_not_publish_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'relations.json'
            def fail(_): raise RuntimeError('upstream unavailable')
            collector=types.SimpleNamespace(cfg=lambda:{'YUNXIAO_ORG':'org'},api=fail)
            with patch.dict(os.environ,{'SHEPHERD_BUG_RELATION_CACHE':str(path)}):
                with self.assertRaises(RuntimeError):enrich_bug_relations([{'id':'b'}],[],collector)
            self.assertFalse(path.exists())
    def test_malformed_relation_response_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            collector=types.SimpleNamespace(cfg=lambda:{'YUNXIAO_ORG':'org'},api=lambda _:({'items':[]},{}))
            with patch.dict(os.environ,{'SHEPHERD_BUG_RELATION_CACHE':str(Path(directory)/'relations.json')}):
                with self.assertRaises(RuntimeError):enrich_bug_relations([{'id':'b'}],[],collector)
