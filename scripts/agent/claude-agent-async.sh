#!/usr/bin/env bash
# 异步执行者(配合 SHEPHERD_AGENT_ASYNC=1)—— 把 WorkSpec 交给本机 `claude` 跑,
# 跑完经 HTTP 回调 `/delivery/{id}/complete|fail|events` 自行收尾。
#
# server 已把派发立即返回(Accepted/Running),本脚本在后台跑,不受请求超时影响。
# server 通过环境变量注入:
#   SHEPHERD_ATTEMPT_ID    本次交付尝试 id(回调目标)
#   SHEPHERD_CALLBACK_URL  回调基址(如 http://127.0.0.1:9180)
#   SHEPHERD_CALLBACK_TOKEN Bearer 令牌(DELIVERY:READ+UPDATE)
#   SHEPHERD_EXECUTOR      执行者种类(CLAUDE_CODE / CODEX)
#
# 启用:
#   SHEPHERD_AGENT_CMD="$PWD/scripts/agent/claude-agent-async.sh" \
#   SHEPHERD_AGENT_ASYNC=1 \
#   DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
#   MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret \
#   ./target/debug/server
set -uo pipefail

CLAUDE_BIN="${CLAUDE_BIN:-claude}"
prompt="$(cat)"
BASE="${SHEPHERD_CALLBACK_URL:?need SHEPHERD_CALLBACK_URL}"
AID="${SHEPHERD_ATTEMPT_ID:?need SHEPHERD_ATTEMPT_ID}"
TOK="${SHEPHERD_CALLBACK_TOKEN:?need SHEPHERD_CALLBACK_TOKEN}"

post() { # path json
  curl -s -o /dev/null -m 15 -H "authorization: Bearer $TOK" -H 'content-type: application/json' \
    -X POST "$BASE$1" --data-binary "$2" || true
}
event() { # kind message
  post "/delivery/$AID/events" "$(python3 -c 'import json,sys;print(json.dumps({"kind":sys.argv[1],"message":sys.argv[2]}))' "$1" "$2")"
}

event DECISION "调用 $CLAUDE_BIN 执行任务(headless, async)"

err="$(mktemp)"
if out="$(printf '%s' "$prompt" | "$CLAUDE_BIN" -p --output-format json --permission-mode acceptEdits 2>"$err")"; then
  summary="$(printf '%s' "$out" | python3 -c 'import sys,json
try: print(json.load(sys.stdin).get("result",""))
except Exception: print("")' 2>/dev/null)"
  [ -n "$summary" ] || summary="$out"
  summary="$(printf '%s' "$summary" | tr '\n' ' ' | cut -c1-800)"
  event FILE_CHANGE "Claude 已落地变更(见工作区 git diff)"
  post "/delivery/$AID/complete" \
    "$(python3 -c 'import json,sys;print(json.dumps({"kind":"DIFF","reference":"claude://"+sys.argv[1],"summary":sys.argv[2]}))' "$AID" "$summary")"
else
  msg="$(tr '\n' ' ' <"$err" | cut -c1-400)"
  [ -n "$msg" ] || msg="claude 执行失败(无 stderr)"
  post "/delivery/$AID/fail" "$(python3 -c 'import json,sys;print(json.dumps({"error":sys.argv[1]}))' "$msg")"
fi
rm -f "$err"
