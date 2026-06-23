#!/usr/bin/env bash
# 真实执行者桥接 —— 把 Shepherd 的 WorkSpec 交给本机 `claude` CLI(headless),
# 再把 Claude 的输出转成交付协议回流给 Shepherd。
#
# 启用:
#   SHEPHERD_AGENT_CMD="/绝对路径/scripts/agent/claude-agent.sh" \
#   DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
#   MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret \
#   ./target/debug/server
#
# 注意:
#   - Claude 会在 server 进程的「当前工作目录」里真实改文件,请在期望的仓库根启动 server;
#   - 会消耗 Claude 用量;需已 `claude` 登录可用;
#   - 默认非交互(--permission-mode acceptEdits)以便无人值守落地变更,按需收紧。
#
# 协议(见 crates/delivery/src/adapters/local.rs):
#   stdout 行 {"event":...} = 审计事件;{"reference":...,"summary":...} = 交付物;退出码非 0 = 失败。
set -euo pipefail

CLAUDE_BIN="${CLAUDE_BIN:-claude}"
prompt="$(cat)"

printf '{"event":"DECISION","message":"调用 %s 执行任务(headless)"}\n' "$CLAUDE_BIN"

# claude -p:从 stdin 读 prompt,--output-format json 返回 {"result":"...", ...}
err="$(mktemp)"
if ! out="$(printf '%s' "$prompt" | "$CLAUDE_BIN" -p --output-format json --permission-mode acceptEdits 2>"$err")"; then
  msg="$(tr '\n' ' ' <"$err" | cut -c1-400)"
  # 把失败原因作为 LOG 回流后非 0 退出 → Shepherd 记为 Failed
  python3 -c 'import sys,json;print(json.dumps({"event":"LOG","message":"claude 执行失败: "+sys.argv[1]}))' "$msg"
  rm -f "$err"
  exit 1
fi
rm -f "$err"

# 取 result 字段作 summary;失败则退回原始输出。单行化以符合按行协议。
summary="$(printf '%s' "$out" | python3 -c 'import sys,json
try:
    print(json.load(sys.stdin).get("result",""))
except Exception:
    print(sys.stdin.read() if False else "")' 2>/dev/null || true)"
[ -n "$summary" ] || summary="$out"
summary="$(printf '%s' "$summary" | tr '\n' ' ' | cut -c1-800)"

printf '{"event":"FILE_CHANGE","message":"Claude 已落地变更(见工作区 git diff)"}\n'
# 用 python 安全构造 JSON(summary 可能含引号/CJK)
python3 -c 'import sys,json;print(json.dumps({"reference":"claude://"+sys.argv[1],"summary":sys.argv[2]}))' "$$" "$summary"
