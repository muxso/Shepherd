#!/usr/bin/env python3
"""把 `claude -p --output-format stream-json` 的事件流实时回流成交付审计事件,
让执行进度抽屉能看到 AI 在干什么(编辑哪个文件 / 跑什么命令 / 决策),而非干等。

从 stdin 读 claude 流式 JSON(一行一事件),边读边 POST /delivery/{id}/events;
末尾向 stdout 打一行 `OK\x1f<summary>` 或 `ERR\x1f<reason>` 供 bash 据此收尾。
env 注入:SHEPHERD_CALLBACK_URL / SHEPHERD_ATTEMPT_ID / SHEPHERD_CALLBACK_TOKEN。
"""
import json
import os
import sys
import urllib.request

BASE = os.environ["SHEPHERD_CALLBACK_URL"]
AID = os.environ["SHEPHERD_ATTEMPT_ID"]
TOK = os.environ["SHEPHERD_CALLBACK_TOKEN"]
US = "\x1f"


def post_event(kind, message):
    body = json.dumps({"kind": kind, "message": (message or "")[:300]}).encode()
    req = urllib.request.Request(
        f"{BASE}/delivery/{AID}/events",
        data=body,
        method="POST",
        headers={"authorization": f"Bearer {TOK}", "content-type": "application/json"},
    )
    try:
        urllib.request.urlopen(req, timeout=10).read()
    except Exception:
        pass  # 事件回流尽力而为,失败不影响执行


result, is_error = "", False
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        ev = json.loads(line)
    except Exception:
        continue
    t = ev.get("type")
    if t == "assistant":
        for c in ev.get("message", {}).get("content", []):
            ct = c.get("type")
            if ct == "tool_use":
                name, inp = c.get("name", ""), c.get("input", {})
                if name in ("Edit", "Write", "MultiEdit", "NotebookEdit"):
                    post_event("FILE_CHANGE", f"{name} {inp.get('file_path', '')}")
                elif name == "Bash":
                    post_event("TOOL_CALL", f"$ {(inp.get('command') or '')[:120]}")
                elif name in ("Grep", "Glob", "Read"):
                    pass  # 检索类噪声,跳过
                elif name == "TodoWrite":
                    cur = next((x.get("content") for x in inp.get("todos", []) if x.get("status") == "in_progress"), None)
                    if cur:
                        post_event("DECISION", f"计划: {cur}")
                else:
                    post_event("TOOL_CALL", name)
            elif ct == "text":
                txt = (c.get("text") or "").strip()
                if txt:
                    post_event("DECISION", txt)
    elif t == "result":
        is_error = bool(ev.get("is_error"))
        result = ev.get("result") or ev.get("error") or ""

print(("ERR" if is_error else "OK") + US + result.replace("\n", " ")[:700])
