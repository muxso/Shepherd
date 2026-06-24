#!/usr/bin/env bash
# 通用异步桥接(codex / opencode 等非 claude CLI)。
#
# 与 claude-agent-async.sh 的区别:**非流式**(不解析 stream-json),
# 直接跑 `$CLI_CMD "<prompt>"` 捕获 stdout,成功则把改动快照成 commit、回调 complete。
# 适配任意「吃一个 prompt 参数、就地改文件」的 CLI。流式进度是 claude 专属增强。
#
# 由 wrapper(codex-agent-async.sh / opencode-agent-async.sh)设置 `CLI_CMD` 后 exec 进来;
# 也可直接 `CLI_CMD="codex exec" bash cli-agent-async.sh`。
#
# server/fleet-runtime 注入:SHEPHERD_ATTEMPT_ID / SHEPHERD_CALLBACK_URL / SHEPHERD_CALLBACK_TOKEN
set -uo pipefail

CLI_CMD="${CLI_CMD:?need CLI_CMD (如 'codex exec' / 'opencode run')}"
prompt="$(cat)"
BASE="${SHEPHERD_CALLBACK_URL:?need SHEPHERD_CALLBACK_URL}"
AID="${SHEPHERD_ATTEMPT_ID:?need SHEPHERD_ATTEMPT_ID}"
TOK="${SHEPHERD_CALLBACK_TOKEN:?need SHEPHERD_CALLBACK_TOKEN}"
title="$(printf '%s\n' "$prompt" | sed -n 's/^# Task: //p' | head -1)"
[ -n "${title:-}" ] || title="task ${SHEPHERD_TASK_ID:-}"

post() { curl -s -o /dev/null -m 15 -H "authorization: Bearer $TOK" -H 'content-type: application/json' -X POST "$BASE$1" --data-binary "$2" || true; }
event() { post "/delivery/$AID/events" "$(python3 -c 'import json,sys;print(json.dumps({"kind":sys.argv[1],"message":sys.argv[2]}))' "$1" "$2")"; }

web_commit_url() { # sha -> url
  local sha="$1" remote
  remote="$(git remote get-url origin 2>/dev/null)" || return 0
  python3 - "$remote" "$sha" <<'PY' 2>/dev/null || true
import re,sys
remote,sha=sys.argv[1],sys.argv[2]
m=re.match(r'(?:git@|https?://)([^/:]+)[/:](.+?)(?:\.git)?$',remote.strip())
if not m: sys.exit(0)
host,path=m.group(1),m.group(2)
seg='/-/commit/' if 'gitlab' in host else '/commit/'
print(f"https://{host}/{path}{seg}{sha}")
PY
}

BEFORE="$(git rev-parse HEAD 2>/dev/null || true)"
event DECISION "调用 $CLI_CMD 执行任务(headless, async, 非流式)"

err="$(mktemp)"
# 通用调用:prompt 作为末位参数。捕获 stdout 作结果文本。
out="$($CLI_CMD "$prompt" 2>"$err")"
rc=$?

if [ "$rc" -eq 0 ]; then
  summary="$(printf '%s' "$out" | tr '\n' ' ' | cut -c1-700)"; [ -n "$summary" ] || summary="(无输出)"
  event VERDICT "执行结果: $summary"

  # —— 交付物:把改动快照成 commit 推远端(同 claude-agent-async.sh)——
  ref="${CLI_CMD%% *}://$AID"
  short="${AID%%-*}"
  branch="shepherd/deliver/$short"
  if [ -n "$BEFORE" ] && [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    stat="$(git diff --stat "$BEFORE" 2>/dev/null | tail -20)"
    sha=""
    {
      flock 9 || true
      git add -A 2>/dev/null
      tree="$(git write-tree 2>/dev/null)"
      git reset -q 2>/dev/null
      if [ -n "$tree" ]; then
        sha="$(printf 'deliver(%s): %s\n\nattempt %s\n' "$short" "$title" "$AID" | git commit-tree "$tree" -p "$BEFORE" 2>/dev/null)"
        if [ -n "$sha" ]; then
          git branch -f "$branch" "$sha" 2>/dev/null
          git push -q -f origin "$branch" 2>/dev/null || event LOG "push $branch 失败(本地已建分支 $sha)"
        fi
      fi
    } 9>/tmp/shepherd-deliver.lock
    if [ -n "$sha" ]; then
      url="$(web_commit_url "$sha")"; ref="${url:-git://$branch@$sha}"
      event FILE_CHANGE "已提交到分支 $branch($(printf '%s' "$sha" | cut -c1-9))"
      summary="变更:$(printf '%s' "$stat" | tr '\n' ';' | cut -c1-300) | $summary"
    else
      event FILE_CHANGE "有改动但快照失败(见工作区 git diff)"
    fi
  else
    event FILE_CHANGE "无代码变动(任务已实现或仅核验)"; summary="(无代码变动)$summary"
  fi
  post "/delivery/$AID/complete" "$(python3 -c 'import json,sys;print(json.dumps({"kind":"DIFF","reference":sys.argv[1],"summary":sys.argv[2]}))' "$ref" "$summary")"
else
  reason="$(tr '\n' ' ' <"$err" | cut -c1-300)"; [ -n "$reason" ] || reason="$(printf '%s' "$out" | tr '\n' ' ' | cut -c1-300)"
  [ -n "$reason" ] || reason="无可读错误"
  event LOG "执行失败: $reason"
  post "/delivery/$AID/fail" "$(python3 -c 'import json,sys;print(json.dumps({"error":sys.argv[1]}))' "$CLI_CMD 失败(exit=$rc): $reason")"
fi
rm -f "$err"
