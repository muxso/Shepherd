#!/usr/bin/env bash
# 执行者机群 runtime(PoC):出站长轮询认领任务 → 本机 claude 跑 → 回调收尾。
#
# 关键:agent 这一侧**无需公网入站**。runtime 主动出站拨号到有公网的 server,
# 长轮询 `GET /agent/work/claim` 认领任务,再复用 `claude-agent-async.sh` 的
# 执行/快照/回调逻辑(它读 stdin + SHEPHERD_* env,经 /delivery/{id}/* 出站回调)。
#
# 前提:
#   - server 以 SHEPHERD_AGENT_FLEET=1 启动(派发即入队);
#   - 本机已安装并登录 `claude`;
#   - 在期望被改动的 git 仓库根运行(claude 在此 cwd 改文件,交付物=该仓库 commit)。
#
# 用法:
#   BASE=http://127.0.0.1:9180 ./scripts/agent/fleet-runtime.sh
set -uo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
BASE="${BASE:-http://127.0.0.1:9180}"
USER_NAME="${MS_ADMIN_USER:-admin}"
PASS="${MS_ADMIN_PASSWORD:-s3cret}"
CAPS="${CAPS:-CLAUDE_CODE}"

login() {
  curl -s -m 10 -XPOST "$BASE/auth/login" -H 'content-type: application/json' \
    -d "$(python3 -c 'import json,sys;print(json.dumps({"username":sys.argv[1],"password":sys.argv[2]}))' "$USER_NAME" "$PASS")" \
    | python3 -c 'import sys,json;print(json.load(sys.stdin).get("token",""))' 2>/dev/null
}

TOKEN="$(login)"
[ -n "$TOKEN" ] || { echo "登录失败($BASE, 用户 $USER_NAME)" >&2; exit 1; }

# 注册到机群(出站);拿到 runtimeId 供心跳续约。
NAME="${RUNTIME_NAME:-$(hostname -s 2>/dev/null || echo runtime)}"
capjson="$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1].split(",")))' "$CAPS")"
RTID="$(curl -s -XPOST "$BASE/agent/runtime" -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d "$(python3 -c 'import json,sys;print(json.dumps({"name":sys.argv[1],"caps":json.loads(sys.argv[2]),"maxConcurrency":1}))' "$NAME" "$capjson")" \
  | python3 -c 'import sys,json;print(json.load(sys.stdin).get("runtimeId",""))' 2>/dev/null)"
echo "fleet-runtime 上线:$BASE  caps=$CAPS  runtimeId=${RTID:-?}  name=$NAME" >&2

while true; do
  # 心跳续约(每轮一次,≤20s 间隔 < 30s TTL);404 → 重新注册。
  if [ -n "$RTID" ]; then
    hb=$(curl -s -o /dev/null -w "%{http_code}" -XPOST "$BASE/agent/runtime/$RTID/heartbeat" -H "authorization: Bearer $TOKEN")
    [ "$hb" = "404" ] && RTID="$(curl -s -XPOST "$BASE/agent/runtime" -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
      -d "$(python3 -c 'import json,sys;print(json.dumps({"name":sys.argv[1],"caps":json.loads(sys.argv[2]),"maxConcurrency":1}))' "$NAME" "$capjson")" \
      | python3 -c 'import sys,json;print(json.load(sys.stdin).get("runtimeId",""))' 2>/dev/null)"
  fi
  # 长轮询认领(server 端 hold ≤20s);带 runtime id 作 PEL 归属(死 runtime 回收用)。204/空 → 无活。
  body="$(curl -s -m 30 -H "authorization: Bearer $TOKEN" "$BASE/agent/work/claim?caps=$CAPS&runtime=$RTID")"
  [ -n "$body" ] || continue

  # 解析 WorkSpec 头部字段;解析失败(如 403 文本)→ 跳过。
  hdr="$(printf '%s' "$body" | python3 -c '
import sys, json
try:
    s = json.load(sys.stdin)
    print("%s\t%s\t%s\t%s" % (s["attemptId"], s.get("taskId",""), s.get("executor","CLAUDE_CODE"), s.get("context") or ""))
except Exception:
    pass
')"
  [ -n "$hdr" ] || { echo "认领响应非法,跳过:$(printf '%s' "$body" | cut -c1-120)" >&2; sleep 1; continue; }
  AID="$(printf '%s' "$hdr" | cut -f1)"
  TASKID="$(printf '%s' "$hdr" | cut -f2)"
  EXECUTOR="$(printf '%s' "$hdr" | cut -f3)"
  MODE="$(printf '%s' "$hdr" | cut -f4)"
  echo "认领任务 attempt=$AID task=$TASKID executor=$EXECUTOR mode=${MODE:-implement}" >&2

  # 构造 prompt(对齐 delivery 的 spec_to_prompt:Behavior / # Task / Acceptance / Context)。
  prompt="$(printf '%s' "$body" | python3 -c '
import sys, json
s = json.load(sys.stdin)
out = []
if s.get("instructions"):
    out.append("# Behavior (skills)\n%s\n\n" % s["instructions"])
out.append("# Task: %s\n\n%s\n" % (s.get("title",""), s.get("description","")))
ac = s.get("acceptanceCriteria") or []
if ac:
    out.append("\nAcceptance criteria:\n" + "".join("- %s\n" % c for c in ac))
if s.get("context"):
    out.append("\nContext: %s\n" % s["context"])
sys.stdout.write("".join(out))
')"

  # 路由:design 模式(产设计稿→POST /proposal/{id}/design)优先;否则按 executor 选实现 bridge。
  if [ "$MODE" = "design" ]; then
    bridge="$DIR/design-draft.sh"
  else
    case "$EXECUTOR" in
      CLAUDE_CODE) bridge="$DIR/claude-agent-async.sh" ;;
      CODEX)       bridge="$DIR/codex-agent-async.sh" ;;
      OPENCODE)    bridge="$DIR/opencode-agent-async.sh" ;;
      *)           bridge="$DIR/claude-agent-async.sh" ;;
    esac
  fi
  # 桥接读 stdin(prompt)+ SHEPHERD_* env,跑对应 CLI、快照 commit、回调 complete/fail/events。
  printf '%s' "$prompt" | \
    SHEPHERD_ATTEMPT_ID="$AID" \
    SHEPHERD_CALLBACK_URL="$BASE" \
    SHEPHERD_CALLBACK_TOKEN="$TOKEN" \
    SHEPHERD_EXECUTOR="$EXECUTOR" \
    SHEPHERD_TASK_ID="$TASKID" \
    bash "$bridge"
done
