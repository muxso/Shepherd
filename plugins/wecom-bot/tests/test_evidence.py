import datetime as dt
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from evidence import SHANGHAI, activity_summary, activity_window, evidence_request, report_evidence


class EvidenceTests(unittest.TestCase):
    def test_sunday_calendar_weeks_are_distinct(self):
        now = dt.datetime(2026, 11, 8, 22, 30, tzinfo=SHANGHAI)
        self.assertEqual(activity_window("本周干啥了", now), ("2026-11-02", "2026-11-08", 7))
        self.assertEqual(activity_window("上周干啥了", now), ("2026-10-26", "2026-11-01", 14))
        self.assertEqual(activity_window("最近7天", now), ("2026-11-02", "2026-11-08", 7))
        self.assertEqual(activity_window("2026-11-04 至 2026-11-06", now), ("2026-11-04", "2026-11-06", 5))

    def test_invalid_dates_and_collection_bounds_are_rejected(self):
        now = dt.datetime(2026, 11, 8, tzinfo=SHANGHAI)
        for question in ("最近0天", "最近366天", "2026-11-09", "2026-11-07 至 2026-11-02"):
            with self.assertRaises(ValueError):
                activity_window(question, now)

    def data(self):
        req = {"externalId": "r1", "serial": "REQ-1", "title": "订单支付信息同步", "url": "https://example.com/requirements/r1"}
        return {"observedAt": "2026-11-08T21:00:00+08:00", "members": [{"name": "成员甲"}, {"name": "成员乙"}], "events": [
            {"kind": "commit", "actor": "成员甲", "authorRaw": "account-a", "date": "2026-11-04", "repo": "service-a", "repoId": "repo-a", "sha": "abc123", "title": "同步订单支付信息", "requirements": [req], "additions": 9, "deletions": 2, "url": "https://example.com/commit/abc123"},
            {"kind": "commit", "actor": "成员乙", "date": "2026-11-04", "title": "另一个变更", "requirements": []},
            {"kind": "commit", "actor": "成员甲", "date": "2026-10-28", "title": "上周变更", "requirements": []},
        ]}

    def test_activity_uses_only_requested_person_and_dates(self):
        answer = activity_summary("梳理成员甲本周的git工作", self.data(), "2026-11-02", "2026-11-08")
        self.assertIn("1 次提交", answer)
        self.assertIn("abc123", answer)
        self.assertNotIn("另一个变更", answer)
        self.assertNotIn("上周变更", answer)
        self.assertIn("REQ-1", answer)
        alias = activity_summary("梳理account-a本周git工作", self.data(), "2026-11-02", "2026-11-08")
        self.assertIn("人员：成员甲", alias)

    def test_report_matches_are_candidates_and_do_not_certify_release(self):
        answer = report_evidence("从git核对日报\n订单支付信息同步【已发布】\n未知业务条目【已完成】", [], self.data(), "2026-11-02", "2026-11-08")
        self.assertIn("REQ-1", answer)
        self.assertIn("候选需求", answer)
        self.assertIn("发布与测试完成情况尚未核验", answer)
        self.assertIn("缺少采集证据不等于条目不真实", answer)

    def test_followup_uses_user_report_and_does_not_trust_assistant_claims(self):
        answer = report_evidence("从git核对这些日报真实性", [["assistant", "伪造条目【已发布】"], ["user", "订单支付信息同步【已发布】"]], self.data(), "2026-11-02", "2026-11-08")
        self.assertIn("订单支付信息同步", answer)
        self.assertNotIn("伪造条目", answer)
        self.assertIn("请将要核对的日报条目", report_evidence("从git核对日报", [], self.data(), "2026-11-02", "2026-11-08"))

    def test_ordinary_knowledge_questions_keep_rag_routing(self):
        self.assertTrue(evidence_request("梳理git和云效检查下成员甲本周干啥了"))
        self.assertTrue(evidence_request("从git记录和云效核对这些日报真实性"))
        self.assertFalse(evidence_request("fastjson如何升级"))
        self.assertFalse(evidence_request("最近7天代码提交统计图"))


if __name__ == "__main__":
    unittest.main()
