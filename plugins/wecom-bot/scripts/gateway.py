"""WeCom long-connection adapter. Credentials and chat contents never enter logs."""
import asyncio
import json
import os
import re
import signal
import time
import uuid
from pathlib import Path
from urllib.parse import quote
from channel import Conversations, chart_request, incoming_text, render_chart, utf8_limit


class SafeLogger:
    def debug(self, *args): pass
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

    async def ask(self, project, question, history, frame, stream):
        headers = {"Authorization": "Bearer "+os.environ["SHEPHERD_API_KEY"]}
        body = {"projectId": project, "question": question, "history": history, "topK": 8, "rerank": True, "trace": False}
        answer, citations, event, last_update = "", [], "", 0.0
        completed = False
        async with self.http.post(os.environ["SHEPHERD_URL"].rstrip("/")+"/rag/ask/stream", json=body, headers=headers) as response:
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
        if citations:
            answer += "\n\n资料来源：\n"+"\n".join(f"[{i+1}] {str(c.get('title') or '项目资料')[:160]}" for i, c in enumerate(citations[:5]))
        return answer or "当前没有足够资料回答，请补充问题背景。"

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
        config = self.config()
        binding = re.fullmatch(r"(?:绑定|bind)\s+(\S+)", question, flags=re.I)
        if binding:
            success = self.state.bind(room, binding.group(1), os.environ.get("WECOM_SETUP_CODE", ""))
            await self.client.reply_stream(frame, stream, "绑定成功。本群已获授权访问项目的机器人共享资料，可直接 @ 我提问或请求统计图。" if success else "绑定码无效或已使用，请联系管理员。", True)
            return
        if not room or not self.state.allowed(room, config.get("allowedChatIds", [])):
            await self.client.reply_stream(frame, stream, "此会话尚未授权。请管理员在目标群内 @ 我发送一次性绑定指令；绑定前不会读取或发送项目资料。", True)
            return
        if not question:
            print("message_skipped unsupported_content", flush=True)
            await self.client.reply_stream(frame, stream, "当前支持文字提问；图片、文件和语音暂时无法读取，请 @ 我用文字描述问题。", True)
            return
        try:
            await asyncio.wait_for(self.tasks.acquire(), timeout=0.1)
        except asyncio.TimeoutError:
            await self.client.reply_stream(frame, stream, "当前查询较多，请稍后重试。", True)
            return
        try:
            lock = self.locks.setdefault((room, sender), asyncio.Lock())
            async with lock:
                await self.client.reply_stream(frame, stream, "正在查询项目资料…", False)
                print("message_acknowledged", flush=True)
                chart = chart_request(question)
                images = None
                if chart:
                    days, role, metric = chart
                    sources = await self.api("/api/devops/sources")
                    source = next((s for s in sources.get("items", []) if s.get("project_id") == config["projectId"]), None)
                    if source is None:
                        raise ValueError("项目尚未配置统计数据源")
                    data = await self.api("/api/devops/sources/"+quote(source["id"], safe="")+f"/team-activity?role={role}&days={days}")
                    images = [await asyncio.to_thread(render_chart, data, metric, os.environ["WECOM_CHART_FONT"])]
                    answer = f"{data['from']} 至 {data['through']}（上海时间）\n已采集活动 {len(data.get('events', []))} 条，新增责任缺陷 {len(data.get('bugs', []))} 个。\n统计图按实际来源记录汇总，完整明细可在项目页面查看。"
                else:
                    print("query_started", flush=True)
                    answer = await self.ask(config["projectId"], question, self.state.history(room, sender), frame, stream)
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
    client = WSClient(WSClientOptions(bot_id=os.environ["WECOM_BOT_ID"], secret=os.environ["WECOM_BOT_SECRET"], max_reconnect_attempts=-1, logger=SafeLogger()))
    async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=240)) as http:
        gateway = Gateway(client, http, os.environ.get("WECOM_CONFIG", "/config/bindings.json"), "/data/private/conversations.sqlite")
        @client.on("authenticated")
        def authenticated():
            gateway.authenticated = True
            print("websocket_authenticated", flush=True)
        @client.on("disconnected")
        def disconnected(*args):
            gateway.authenticated = False
            print("websocket_disconnected", flush=True)
        @client.on("error")
        def error(*args):
            print("websocket_error", flush=True)
        client.on("message", gateway.on_message)
        async def health(request):
            return web.json_response({"authenticated": gateway.authenticated, "lastMessageAt": gateway.last_message_at, "activeQueries": len(gateway.locks)}, status=200 if request.path == "/healthz" or gateway.authenticated else 503)
        app = web.Application(); app.router.add_get("/healthz", health); app.router.add_get("/readyz", health)
        runner = web.AppRunner(app, access_log=None); await runner.setup(); await web.TCPSite(runner, "0.0.0.0", 8080).start()
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        await client.connect()
        await stop.wait()
        client.disconnect(); await runner.cleanup()


if __name__ == "__main__":
    asyncio.run(main())
