import json
import os
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch
from yunxiao_bug_history import project_history, enrich_bug_history


def transition(identity, before, after, timestamp=1791021600000):
    return {'resourceId': 'bug', 'eventType': 'workitem.transitioned', 'eventId': identity, 'eventTime': timestamp,
            'oldValue': [{'displayValue': before}], 'newValue': [{'displayValue': after}]}


CREATED = {'resourceId': 'bug', 'eventType': 'workitem.created'}


class BugHistoryTests(unittest.TestCase):
    def test_true_reopens_are_deduplicated_and_use_event_time(self):
        event = transition(1, '已解决', '待处理')
        history = project_history([CREATED, event, event, transition(2, '修复中', '已解决'), transition(3, '已验证', '已发布'), transition(4, '已发布', '修复中'), transition(5, 'Open', 'Reopened'), transition(6, 'Reopened', 'Reopened'), transition(7, '已验证', '再次打开'), transition(8, '待确认', '待处理')], 'bug')
        self.assertTrue(history['complete'])
        self.assertEqual(history['schemaVersion'], 2)
        self.assertEqual([e['id'] for e in history['resolutions']], ['2', '3'])
        self.assertEqual(len(history['transitions']), 7)
        self.assertEqual([e['id'] for e in history['events']], ['1', '4', '5', '7'])
        self.assertTrue(all(e['at'].endswith('+00:00') for e in history['events']))
        self.assertNotIn('operator', json.dumps(history))

    def test_no_history_or_unknown_status_is_unknown_not_zero(self):
        self.assertFalse(project_history([], 'bug')['complete'])
        self.assertFalse(project_history([CREATED, transition(1, '自定义终态', '待处理')], 'bug')['complete'])
        self.assertTrue(project_history([CREATED], 'bug')['complete'])
        for rows in [{}, [CREATED, transition(1, '已解决', '待处理', 'invalid')], [CREATED, dict(transition(1, '已解决', '待处理'), resourceId='other')], [CREATED, transition(1, '已解决', '待处理'), transition(1, '已解决', '待处理', 1791108000000)]]:
            with self.assertRaises((RuntimeError, ValueError)):
                project_history(rows, 'bug')

    def test_private_cache_refreshes_modified_bugs_and_never_publishes_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'history.json'
            calls = []
            def api(endpoint):
                calls.append(endpoint)
                return [CREATED, transition(1, '已解决', '待处理')], {}
            collector = types.SimpleNamespace(cfg=lambda: {'YUNXIAO_ORG': 'org'}, api=api)
            bugs = [{'id': 'bug', 'gmtModified': 1}]
            with patch.dict(os.environ, {'SHEPHERD_BUG_HISTORY_CACHE': str(path)}), patch('yunxiao_bug_history.time.sleep'):
                enriched = enrich_bug_history(bugs, collector)
                self.assertEqual(len(enriched[0]['reopenHistory']['events']), 1)
                self.assertNotIn('reopenHistory', bugs[0])
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
                enrich_bug_history(bugs, collector)
                self.assertEqual(len(calls), 1)
                bugs[0]['gmtModified'] = 2
                enrich_bug_history(bugs, collector)
                self.assertEqual(len(calls), 2)
                before = path.read_bytes()
                collector.api = lambda _: (_ for _ in ()).throw(RuntimeError('upstream unavailable'))
                bugs[0]['gmtModified'] = 3
                with self.assertRaises(RuntimeError):
                    enrich_bug_history(bugs, collector)
                self.assertEqual(path.read_bytes(), before)
