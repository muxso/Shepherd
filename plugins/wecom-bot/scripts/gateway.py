"""WeCom long-connection adapter. Credentials and chat contents never enter logs."""
import asyncio
import contextlib
import json
import os
import re
import signal
import time
import uuid
from pathlib import Path
from urllib.parse import quote
from channel import Conversations, incoming_text, utf8_limit, citation_footer, render_grounded_chart, absolute_source_links



class SafeLogger:
    def __init__(self):
        self.last_heartbeat_at = None

    def debug(self, message, *args):
        if message == "Received heartbeat ack":
            self.last_heartbeat_at = time.monotonic()
    def info(self, *args): pass
    def warn(self, *args): print("sdk_warning", flush=True)
    def error(self, *args): print("sdk_error", flush=True)


class Gateway:
    def __init__(self, client, http, config_path, state_path):
        self.client, self.http = client, http
        self.config_path = config_path
        self.state = Conversations(state_path)
        self.config()
        self.authenticated = False
        self.tasks = asyncio.Semaphore(3)
        self.locks = {}
        self.last_message_at = None

    def config(self):
        if os.environ.get("WECOM_REMOTE_POLICY", "1") == "1":
            return {}
        config = json.loads(Path(self.config_path).read_text())
        if not isinstance(config.get("projectId"), str) or not config["projectId"].strip():
            raise ValueError("project_required")
        if not isinstance(config.get("allowedChatIds", []), list):
            raise ValueError("invalid_chat_allowlist")
        return config

    async def api(self, path):
        headers = {"Authorization": "Bearer "+os.environ["SHEPHERD_API_KEY"]}
        async with self.http.get(os.environ["SHEPHERD_URL"].rstrip("/")+path, headers=headers) as response:
            if response.status != 200:
                raise RuntimeError("backend_unavailable")
            return await response.json()

    async def bind_remote(self, room, code):
        headers = {"Authorization": "Bearer "+os.environ["SHEPHERD_API_KEY"]}
        async with self.http.post(os.environ["SHEPHERD_URL"].rstrip("/")+"/rag/channel/bind", json={"chatId": room, "code": code}, headers=headers) as response:
            return response.status == 200

    async def report_status(self, receipt=None):
        if os.environ.get("WECOM_REMOTE_POLICY", "1") != "1":
            return
        age = None if not getattr(self, "heartbeat_at", None) else time.monotonic()-self.heartbeat_at
        body = {"authenticated": self.authenticated, "heartbeatAgeSeconds": age,
                "activeQueries": len(self.locks), "lastMessageAt": int(self.last_message_at*1000) if self.last_message_at else None,
                "receipt": receipt}
        headers = {"Authorization": "Bearer "+os.environ["SHEPHERD_API_KEY"]}
        try:
            async with self.http.post(os.environ["SHEPHERD_URL"].rstrip("/")+"/rag/channel/runtime", json=body, headers=headers) as response:
                if response.status != 204:
                    print("channel_status_unavailable", flush=True)
        except Exception:
            print("channel_status_unavailable", flush=True)

    async def remote_project(self, body, room, sender):
        kind = "direct" if body.get("chattype") == "single" or not room else "group"
        external = sender if kind == "direct" else room
        await self.report_status({"kind": kind, "externalId": external, "senderId": sender})
        policy = await self.api("/rag/channel/runtime")
        if not policy.get("enabled"):
            return None
        return next((p.get("projectId") for p in policy.get("conversations", [])
                     if p.get("enabled") and p.get("kind") == kind and p.get("externalId") == external), None)

    async def ask(self, project, question, history, frame, stream):
        headers = {"Authorization": "Bearer "+os.environ["SHEPHERD_API_KEY"]}
        body = {"projectId": project, "question": question, "history": history, "topK": 8, "rerank": True, "trace": False}
        answer, citations, event, last_update = "", [], "", 0.0
        completed = False
        charts = []
        async with self.http.post(os.environ["SHEPHERD_URL"].rstrip("/")+"/rag/ask/stream", json=body, headers=headers, read_bufsize=4*1024*1024) as response:
            if response.status != 200:
                raise RuntimeError("backend_unavailable")
            async for raw in response.content:
                line = raw.decode().strip()
                if line.startswith("event:"):
                    event = line[6:].strip()
                elif line.startswith("data:"):
                    data = json.loads(line[5:].strip())
                    if event == "error":
                        raise RuntimeError("answer_unavailable")
                    if event == "chart" and len(charts) < 3:
                        charts.append(data)
                    if event == "done":
                        completed = True
                    if event == "sources":
                        citations = data.get("sources", [])
                    if event == "chunk":
                        answer += data.get("delta", "")
                        if time.monotonic()-last_update >= 1:
                            await self.client.reply_stream(frame, stream, utf8_limit(answer), False)
                            last_update = time.monotonic()
        if not completed:
            raise RuntimeError("incomplete_answer")
        answer = absolute_source_links(answer, os.environ.get("SHEPHERD_PUBLIC_URL", ""))
        footer = citation_footer(citations, os.environ.get("SHEPHERD_PUBLIC_URL", ""))
        if footer:
            answer = utf8_limit(answer, max(1000, 18900-len(footer.encode())))+footer
        images = []
        for chart in charts:
            try:
                images.append(await asyncio.to_thread(render_grounded_chart, chart, os.environ["WECOM_CHART_FONT"]))
            except Exception as error:
                print("chart_failed "+type(error).__name__, flush=True)
                answer += "\n\n图表生成失败，以上文字仍是本次查询结果，可在网页查看图表。"
        return answer or "当前没有足够资料回答，请补充问题背景。", images or None

    async def on_message(self, frame):
        body = frame.get("body") or {}
        self.last_message_at = time.time()
        kind = body.get("msgtype", "unknown")
        safe_kind = kind if kind in ("text", "mixed", "image", "voice", "file") else "unknown"
        print("message_received type="+safe_kind, flush=True)
        await self.on_text(frame)

    async def on_text(self, frame):
        body = frame.get("body", {})
        room = str(body.get("chatid") or "")
        sender = str((body.get("from") or {}).get("userid") or "")
        question = incoming_text(body)
        message = str(body.get("msgid") or frame.get("headers", {}).get("req_id") or "")
        if not sender or not message:
            print("message_skipped missing_identity", flush=True)
            return
        if not self.state.claim(message):
            print("message_skipped duplicate", flush=True)
            return
        stream = "shepherd_"+uuid.uuid4().hex
        identity = re.sub(r"[\s？?！!。]", "", question).lower()
        if identity in ("我的用户id是什么", "我的用户id", "查看我的用户id", "绑定我的用户id", "我的userid", "当前会话id", "群id", "查看群id"):
            direct = body.get("chattype") == "single" or not room
            with contextlib.suppress(Exception):
                await asyncio.wait_for(self.report_status({"kind": "direct" if direct else "group", "externalId": sender if direct else room, "senderId": sender}), timeout=3)
            text = "你的企业微信用户 ID：`"+sender+"`"
            if not direct:
                text += "\n当前群 ID：`"+room+"`"
            text += "\n请管理员在 RAG 后台展开现有机器人渠道，在待授权会话中选择项目并授权。身份查询不会自动绑定或授予项目访问权限。"
            await self.client.reply_stream(frame, stream, text, True)
            return
        config = self.config()
        binding = re.fullmatch(r"(?:绑定|bind)\s+(\S+)", question, flags=re.I)
        remote = os.environ.get("WECOM_REMOTE_POLICY", "1") == "1"
        if binding and remote:
            if not room or body.get("chattype") == "single":
                success = False
            else:
                try:
                    success = await asyncio.wait_for(self.bind_remote(room, binding.group(1)), timeout=12)
                except Exception:
                    success = False
            await self.client.reply_stream(frame, stream, "绑定成功。本群已按后台配置获授权访问指定项目的机器人共享资料。" if success else "绑定未完成。请管理员在 RAG 后台开启渠道与群绑定，生成指定项目的有效一次性绑定码。", True)
            return
        if binding and not remote:
            success = self.state.bind(room, binding.group(1), os.environ.get("WECOM_SETUP_CODE", ""))
            await self.client.reply_stream(frame, stream, "绑定成功。本群已获授权访问项目的机器人共享资料，可直接 @ 我提问或请求统计图。" if success else "绑定码无效或已使用，请联系管理员。", True)
            return
        project = config.get("projectId", "")
        if remote:
            try:
                project = await asyncio.wait_for(self.remote_project(body, room, sender), timeout=12)
            except Exception:
                await self.client.reply_stream(frame, stream, "暂时无法核验会话授权，本次不会读取项目资料，请稍后重试。", True)
                return
            authorized = bool(project)
        else:
            authorized = bool(room) and self.state.allowed(room, config.get("allowedChatIds", []))
        if not authorized:
            await self.client.reply_stream(frame, stream, "此会话尚未授权。请管理员在 Shepherd 的 RAG 后台展开现有机器人渠道，在待授权会话中选择项目并授权；授权前不会读取或发送项目资料。发送“我的用户 ID”可查看当前身份。", True)
            return
        if not question:
            print("message_skipped unsupported_content", flush=True)
            await self.client.reply_stream(frame, stream, "当前支持文字提问；图片、文件和语音暂时无法读取，请 @ 我用文字描述问题。", True)
            return
        room = f"{project}|{room or 'direct:'+sender}" if remote else room
        try:
            await asyncio.wait_for(self.tasks.acquire(), timeout=0.1)
        except asyncio.TimeoutError:
            await self.client.reply_stream(frame, stream, "当前查询较多，请稍后重试。", True)
            return
        try:
            lock = self.locks.setdefault((room, sender), asyncio.Lock())
            async with lock:
                await self.client.reply_stream(frame, stream, "正在查询…", False)
                print("message_acknowledged", flush=True)
                print("query_started", flush=True)
                result = await self.ask(project, question, self.state.history(room, sender), frame, stream)
                answer, images = (result, None) if isinstance(result, str) else result
                if any(item.get("msgtype") != "text" for item in (body.get("mixed") or {}).get("msg_item", [])):
                    answer = "本次仅根据消息中的文字查询，未读取附件。\n\n"+answer
                await self.client.reply_stream(frame, stream, utf8_limit(answer), True, msg_item=images)
                self.state.append(room, sender, question, answer)
                print("message_completed", flush=True)
        except ValueError:
            await self.client.reply_stream(frame, stream, "统计图支持今天或最近 1～365 天的团队汇总，例如：最近 7 天代码提交统计图。其他问题可直接用文字提问。", True)
        except Exception as error:
            print("message_failed "+type(error).__name__, flush=True)
            await self.client.reply_stream(frame, stream, "本次查询未能完成，请稍后重试或在 Shepherd 网页查看。", True)
        finally:
            self.tasks.release()
            if not lock.locked() and not getattr(lock, "_waiters", None):
                self.locks.pop((room, sender), None)


async def main():
    os.umask(0o077)
    import aiohttp
    from aiohttp import web
    from aibot import WSClient, WSClientOptions
    logger = SafeLogger()
    client = WSClient(WSClientOptions(bot_id=os.environ["WECOM_BOT_ID"], secret=os.environ["WECOM_BOT_SECRET"], max_reconnect_attempts=-1, logger=logger))
    async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=240)) as http:
        gateway = Gateway(client, http, os.environ.get("WECOM_CONFIG", "/config/bindings.json"), "/data/private/conversations.sqlite")
        @client.on("authenticated")
        def authenticated():
            gateway.authenticated = True
            logger.last_heartbeat_at = time.monotonic()
            print("websocket_authenticated", flush=True)
        @client.on("disconnected")
        def disconnected(*args):
            gateway.authenticated = False
            print("websocket_disconnected", flush=True)
        @client.on("event")
        def event(frame):
            kind = ((frame.get("body") or {}).get("event") or {}).get("eventtype", "unknown")
            safe_kind = kind if re.fullmatch(r"[a-z_]{1,64}", str(kind)) else "unknown"
            print("event_received type="+safe_kind, flush=True)
            if kind in ("disconnected_event", "disconnected"):
                gateway.authenticated = False
                stop.set()
        @client.on("error")
        def error(*args):
            print("websocket_error", flush=True)
        client.on("message", gateway.on_message)
        async def health(request):
            age = None if logger.last_heartbeat_at is None else time.monotonic()-logger.last_heartbeat_at
            fresh = age is None or age < 90
            ready = gateway.authenticated and fresh
            status = 200 if (request.path == "/healthz" and fresh) or ready else 503
            return web.json_response({"authenticated": gateway.authenticated, "heartbeatAgeSeconds": None if age is None else round(age, 1), "lastMessageAt": gateway.last_message_at, "activeQueries": len(gateway.locks)}, status=status)
        app = web.Application(); app.router.add_get("/healthz", health); app.router.add_get("/readyz", health)
        runner = web.AppRunner(app, access_log=None); await runner.setup(); await web.TCPSite(runner, "0.0.0.0", 8080).start()
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        async def report_loop():
            while not stop.is_set():
                gateway.heartbeat_at = logger.last_heartbeat_at
                try:
                    await asyncio.wait_for(gateway.report_status(), timeout=10)
                except asyncio.TimeoutError:
                    print("channel_status_timeout", flush=True)
                try:
                    await asyncio.wait_for(stop.wait(), timeout=25)
                except asyncio.TimeoutError:
                    pass
        reporter = asyncio.create_task(report_loop())
        try:
            await client.connect()
            await stop.wait()
        finally:
            reporter.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await reporter
            client.disconnect(); await runner.cleanup()


if __name__ == "__main__":
    asyncio.run(main())
