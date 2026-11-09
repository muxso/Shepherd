import asyncio
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from gateway import Gateway


class Client:
    def __init__(self):
        self.replies = []

    async def reply_stream(self, frame, stream, text, finished, **kwargs):
        self.replies.append((text, finished, kwargs))


class UnavailableBackend:
    def get(self, *args, **kwargs):
        raise AssertionError("Unauthorized messages must not query the backend")

    def post(self, *args, **kwargs):
        raise AssertionError("Unauthorized messages must not query the backend")


def frame(message="m1", room="group-a", user="user-a", question="说明接口"):
    return {"body": {"msgid": message, "chatid": room, "from": {"userid": user}, "text": {"content": question}}}


class GatewayTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.policy_mode = patch.dict(os.environ, {"WECOM_REMOTE_POLICY": "0"})
        self.policy_mode.start()
        self.addCleanup(self.policy_mode.stop)
        self.directory = tempfile.TemporaryDirectory()
        path = Path(self.directory.name)
        (path / "config.json").write_text(json.dumps({"projectId": "project-a", "allowedChatIds": []}))
        self.client = Client()
        self.gateway = Gateway(self.client, UnavailableBackend(), str(path / "config.json"), str(path / "state.sqlite"))

    async def asyncTearDown(self):
        self.gateway.state.db.close()
        self.directory.cleanup()

    async def test_mixed_text_reaches_the_query_handler_once(self):
        self.gateway.state.bind("group-a", "secret", "secret")
        calls = []
        async def ask(project, question, history, received, stream):
            calls.append(question)
            return "回答"
        self.gateway.ask = ask
        message = frame()
        message["body"].pop("text")
        message["body"].update(msgtype="mixed", mixed={"msg_item": [
            {"msgtype": "text", "text": {"content": "@机器人 解释一下"}},
            {"msgtype": "text", "text": {"content": "这个接口"}},
        ]})
        await self.gateway.on_message(message)
        await self.gateway.on_message(message)
        self.assertEqual(calls, ["解释一下\n这个接口"])
        self.assertIsNotNone(self.gateway.last_message_at)
        self.assertTrue(self.client.replies[-1][1])

    async def test_unsupported_attachment_gets_a_clear_reply_without_query(self):
        self.gateway.state.bind("group-a", "secret", "secret")
        message = frame()
        message["body"].pop("text")
        message["body"]["msgtype"] = "image"
        await self.gateway.on_message(message)
        self.assertIn("暂时无法读取", self.client.replies[-1][0])
        self.assertEqual(self.gateway.state.history("group-a", "user-a"), [])

    async def test_activity_questions_use_the_same_backend_as_document_questions(self):
        self.gateway.state.bind("group-a", "secret", "secret")
        calls = []
        async def ask(project, question, history, received, stream):
            calls.append((project, question))
            return "统计回答", [{"msgtype": "image", "image": {"base64": "png", "md5": "digest"}}]
        self.gateway.ask = ask
        await self.gateway.on_message(frame(question="每个组本周的代码热力图"))
        self.assertEqual(calls, [("project-a", "每个组本周的代码热力图")])
        self.assertEqual(self.client.replies[-1][2]["msg_item"][0]["msgtype"], "image")

    async def test_unbound_room_is_rejected_without_backend_or_history(self):
        await self.gateway.on_text(frame())
        self.assertIn("尚未授权", self.client.replies[0][0])
        self.assertEqual(self.gateway.state.history("group-a", "user-a"), [])
        await self.gateway.on_text(frame())
        self.assertEqual(len(self.client.replies), 1)

    async def test_one_use_binding_does_not_store_secret_in_history(self):
        with patch.dict(os.environ, {"WECOM_SETUP_CODE": "setup-secret"}):
            await self.gateway.on_text(frame(question="绑定 setup-secret"))
            self.assertIn("绑定成功", self.client.replies[-1][0])
            await self.gateway.on_text(frame(message="m2", room="group-b", question="绑定 setup-secret"))
            self.assertIn("无效或已使用", self.client.replies[-1][0])
        self.assertEqual(self.gateway.state.history("group-a", "user-a"), [])

    async def test_authorized_question_keeps_fixed_project_and_isolated_history(self):
        self.gateway.state.bind("group-a", "secret", "secret")
        calls = []
        async def ask(project, question, history, received, stream):
            calls.append((project, question, history))
            return "回答"
        self.gateway.ask = ask
        await self.gateway.on_text(frame(question="解释实现"))
        await self.gateway.on_text(frame(message="m2", user="user-b", question="解释流程"))
        self.assertEqual(calls, [("project-a", "解释实现", []), ("project-a", "解释流程", [])])
        self.assertTrue(self.client.replies[-1][1])
        self.assertEqual(self.gateway.locks, {})

    async def test_remote_authorization_is_project_scoped_and_revocation_wins_over_local_binding(self):
        self.gateway.state.bind("group-a", "secret", "secret")
        policy = {"enabled": True, "conversations": [{"kind": "group", "externalId": "group-a", "projectId": "project-b", "enabled": True}]}
        async def api(path):
            self.assertEqual(path, "/rag/channel/runtime")
            return policy
        async def report(receipt=None):
            self.assertNotIn("question", receipt or {})
        calls = []
        async def ask(project, question, history, received, stream):
            calls.append((project, history))
            return "Scoped answer"
        self.gateway.api, self.gateway.report_status, self.gateway.ask = api, report, ask
        with patch.dict(os.environ, {"WECOM_REMOTE_POLICY": "1"}):
            await self.gateway.on_text(frame())
            policy["enabled"] = False
            await self.gateway.on_text(frame(message="m2"))
        self.assertEqual(calls, [("project-b", [])])
        self.assertIn("尚未授权", self.client.replies[-1][0])

    async def test_direct_chat_requires_an_explicit_user_authorization(self):
        policy = {"enabled": True, "conversations": [{"kind": "direct", "externalId": "user-a", "projectId": "project-a", "enabled": True}]}
        async def api(path): return policy
        async def report(receipt=None): pass
        calls = []
        async def ask(project, question, history, received, stream):
            calls.append(project)
            return "Direct answer"
        self.gateway.api, self.gateway.report_status, self.gateway.ask = api, report, ask
        with patch.dict(os.environ, {"WECOM_REMOTE_POLICY": "1"}):
            await self.gateway.on_text(frame(room=""))
            await self.gateway.on_text(frame(message="m2", room="", user="user-b"))
        self.assertEqual(calls, ["project-a"])
        self.assertIn("尚未授权", self.client.replies[-1][0])

    async def test_identity_query_only_returns_current_sender_without_granting_access(self):
        receipts = []
        async def report(receipt=None): receipts.append(receipt)
        self.gateway.report_status = report
        with patch.dict(os.environ, {"WECOM_REMOTE_POLICY": "1"}):
            await self.gateway.on_text(frame(room="", question="我的用户 ID 是什么？"))
            self.assertIn("user-a", self.client.replies[-1][0])
            self.assertIn("不会自动绑定", self.client.replies[-1][0])
            await self.gateway.on_text(frame(message="m2", question="绑定我的用户 ID"))
            self.assertIn("group-a", self.client.replies[-1][0])
        self.assertEqual(receipts[0], {"kind": "direct", "externalId": "user-a", "senderId": "user-a"})
        self.assertFalse(self.gateway.state.allowed("group-a", []))
        self.assertEqual(self.gateway.state.history("group-a", "user-a"), [])

    async def test_remote_binding_never_uses_the_local_setup_code(self):
        async def bind(room, code):
            return False
        self.gateway.bind_remote = bind
        with patch.dict(os.environ, {"WECOM_REMOTE_POLICY": "1", "WECOM_SETUP_CODE": "secret"}):
            await self.gateway.on_text(frame(question="绑定 secret"))
        self.assertIn("后台开启", self.client.replies[-1][0])
        self.assertFalse(self.gateway.state.allowed("group-a", []))

    async def test_backend_authorization_is_required_when_no_mode_is_configured(self):
        self.gateway.state.bind("group-a", "secret", "secret")
        calls = []
        async def api(path):
            calls.append(path)
            return {"enabled": False, "conversations": []}
        async def report(receipt=None): pass
        self.gateway.api, self.gateway.report_status = api, report
        with patch.dict(os.environ, {}, clear=True):
            await self.gateway.on_text(frame())
        self.assertEqual(calls, ["/rag/channel/runtime"])
        self.assertIn("尚未授权", self.client.replies[-1][0])


if __name__ == "__main__":
    unittest.main()
