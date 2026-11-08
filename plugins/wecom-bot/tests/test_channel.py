import base64
import hashlib
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from channel import Conversations, chart_data, chart_request, clean_question, render_chart, utf8_limit


class ChannelTests(unittest.TestCase):
    def test_binding_is_single_room_and_persists(self):
        with tempfile.TemporaryDirectory() as directory:
            filename = str(Path(directory) / "state.sqlite")
            state = Conversations(filename)
            self.assertFalse(state.bind("room-a", "wrong", "secret"))
            self.assertFalse(state.bind("room-a", "错误绑定码", "secret"))
            self.assertFalse(state.bind("", "secret", "secret"))
            self.assertTrue(state.bind("room-a", "secret", "secret"))
            self.assertTrue(state.bind("room-a", "secret", "secret"))
            self.assertFalse(state.bind("room-b", "secret", "secret"))
            reopened = Conversations(filename)
            self.assertTrue(reopened.allowed("room-a", []))
            self.assertFalse(reopened.allowed("room-b", []))
            self.assertTrue(reopened.allowed("room-b", ["room-b"]))

    def test_deduplication_and_sender_history_isolation(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Conversations(str(Path(directory) / "state.sqlite"))
            self.assertTrue(state.claim("message"))
            self.assertFalse(state.claim("message"))
            self.assertFalse(state.claim(""))
            state.append("room-a", "sender-a", "question", "answer")
            self.assertEqual(state.history("room-a", "sender-a"), [["user", "question"], ["assistant", "answer"]])
            self.assertEqual(state.history("room-b", "sender-a"), [])
            self.assertEqual(state.history("room-a", "sender-b"), [])

    def test_only_supported_chart_ranges_are_accepted(self):
        self.assertEqual(chart_request("最近 90 天代码量统计图"), (90, "code", "lines"))
        self.assertEqual(chart_request("今天测试缺陷统计图"), (1, "test", "bugs"))
        self.assertIsNone(chart_request("这个接口如何实现"))
        for question in ("最近0天提交统计图", "最近366天提交统计图", "上周提交统计图", "2026/11/01提交统计图"):
            with self.assertRaises(ValueError):
                chart_request(question)

    def test_chart_counts_unique_repositories_requirements_and_bugs(self):
        data = {"role": "code", "events": [
            {"actor": "成员甲", "repoId": "repo-a", "additions": 3, "deletions": 2, "requirements": [{"externalId": "r1"}]},
            {"actor": "成员甲", "repoId": "repo-a", "additions": None, "requirements": [{"externalId": "r1"}, {"externalId": "r2"}]},
        ], "bugs": [{"id": "b1", "responsibles": ["成员甲"]}, {"id": "b1", "responsibles": ["成员甲"]}]}
        for metric, expected in [("events", 2), ("lines", 5), ("repos", 1), ("requirements", 2), ("bugs", 1)]:
            self.assertEqual(chart_data(data, metric), [("成员甲", expected)])
        reported = {"role": "test", "events": [{"actor": "测试甲", "kind": "bugReported", "externalId": "b1"}], "bugs": data["bugs"]}
        self.assertEqual(chart_data(reported, "bugs"), [("测试甲", 1)])

    def test_mentions_and_utf8_limits(self):
        self.assertEqual(clean_question("<@bot> 说明接口"), "说明接口")
        self.assertEqual(clean_question("@机器人 说明接口"), "说明接口")
        self.assertLessEqual(len(utf8_limit("测" * 10000).encode()), 19003)


if __name__ == "__main__":
    unittest.main()
