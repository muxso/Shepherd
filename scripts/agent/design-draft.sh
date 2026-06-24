#!/usr/bin/env bash
# 设计起草桥接(design 模式):跑 CLI 产出 **markdown 设计稿**,POST 到 /proposal/{id}/design。
#
# 与实现 bridge 不同:设计阶段**只产文档,不改文件、不快照 commit**。
# attempt_id == proposal id(由 DeliveryDesignDrafter 设定),故回调 /proposal/$AID/design。
# prompt(架构师角色 + 任务)由 fleet-runtime.sh 经 stdin 传入;失败仅日志(无 /fail 端点,
# 提案保持 DRAFTING,人可重试或手填)。
#
# env:SHEPHERD_ATTEMPT_ID / SHEPHERD_CALLBACK_URL / SHEPHERD_CALLBACK_TOKEN;CLAUDE_BIN 可覆盖。
set -uo pipefail

CLAUDE_BIN="${CLAUDE_BIN:-claude}"
prompt="$(cat)"
BASE="${SHEPHERD_CALLBACK_URL:?need SHEPHERD_CALLBACK_URL}"
AID="${SHEPHERD_ATTEMPT_ID:?need SHEPHERD_ATTEMPT_ID}"
TOK="${SHEPHERD_CALLBACK_TOKEN:?need SHEPHERD_CALLBACK_TOKEN}"

err="$(mktemp)"
# headless 文本生成:stdout 即设计稿(不带工具/文件编辑)。
doc="$("$CLAUDE_BIN" -p "$prompt" 2>"$err")"
rc=$?

if [ "$rc" -eq 0 ] && [ -n "$doc" ]; then
  code=$(curl -s -o /dev/null -w "%{http_code}" -m 20 -H "authorization: Bearer $TOK" -H 'content-type: application/json' \
    -X POST "$BASE/proposal/$AID/design" \
    --data-binary "$(python3 -c 'import json,sys;print(json.dumps({"doc":sys.stdin.read()}))' <<<"$doc")")
  echo "design-draft: proposal=$AID 回填设计稿 → HTTP $code" >&2
else
  echo "design-draft: 起草失败(exit=$rc): $(tr '\n' ' ' <"$err" | cut -c1-200)" >&2
fi
rm -f "$err"
