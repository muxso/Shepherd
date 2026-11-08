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
        self.directory = tempfile.TemporaryDirectory()
        path = Path(self.directory.name)
        (path / "config.json").write_text(json.dumps({"projectId": "project-a", "allowedChatIds": []}))
        self.client = Client()
        self.gateway = Gateway(self.client, UnavailableBackend(), str(path / "config.json"), str(path / "state.sqlite"))

    async def asyncTearDown(self):
        self.gateway.state.db.close()
        self.directory.cleanup()

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


if __name__ == "__main__":
    unittest.main()
