#!/usr/bin/env bash
# 异步执行者(配合 SHEPHERD_AGENT_ASYNC=1)—— 把 WorkSpec 交给本机 `claude` 跑,
# 跑完经 HTTP 回调 `/delivery/{id}/complete|fail|events` 自行收尾。
#
# 交付物 = 真实代码变动:跑前记 HEAD,跑后把 Claude 的改动**快照成一个 commit**
# 推到 per-task 分支 `shepherd/deliver/<id>`,交付物 reference = GitHub/GitLab commit URL。
# 用 git 底层命令(write-tree/commit-tree)生成快照 —— 不切分支、不动工作区,
# 你照样能在 dev 分支 review/提交这批改动。无改动则标「无代码变动」。
#
# server 通过环境变量注入:
#   SHEPHERD_ATTEMPT_ID / SHEPHERD_CALLBACK_URL / SHEPHERD_CALLBACK_TOKEN / SHEPHERD_EXECUTOR
#
# 启用:
#   SHEPHERD_AGENT_CMD="$PWD/scripts/agent/claude-agent-async.sh" SHEPHERD_AGENT_ASYNC=1 \
#   DATABASE_URL=… MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret ./target/debug/server
set -uo pipefail

CLAUDE_BIN="${CLAUDE_BIN:-claude}"
prompt="$(cat)"
BASE="${SHEPHERD_CALLBACK_URL:?need SHEPHERD_CALLBACK_URL}"
AID="${SHEPHERD_ATTEMPT_ID:?need SHEPHERD_ATTEMPT_ID}"
TOK="${SHEPHERD_CALLBACK_TOKEN:?need SHEPHERD_CALLBACK_TOKEN}"
title="$(printf '%s\n' "$prompt" | sed -n 's/^# Task: //p' | head -1)"
[ -n "${title:-}" ] || title="task ${SHEPHERD_TASK_ID:-}"

post() { # path json
  curl -s -o /dev/null -m 15 -H "authorization: Bearer $TOK" -H 'content-type: application/json' \
    -X POST "$BASE$1" --data-binary "$2" || true
}
event() { post "/delivery/$AID/events" "$(python3 -c 'import json,sys;print(json.dumps({"kind":sys.argv[1],"message":sys.argv[2]}))' "$1" "$2")"; }

# 把 origin remote URL + commit sha 拼成网页 commit URL(github / gitlab 兼容)。
web_commit_url() { # sha -> url(失败回空)
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
event DECISION "调用 $CLAUDE_BIN 执行任务(headless, async)"

err="$(mktemp)"
out="$(printf '%s' "$prompt" | "$CLAUDE_BIN" -p --output-format json --permission-mode acceptEdits 2>"$err")"
rc=$?
# claude 出错时退出码可能仍为 0,但 JSON 里 is_error=true,真正错误在 stdout(result/error)
# 而非 stderr —— 所以成败要看 stdout 的 is_error,失败要把 stdout 的错误文本回报。
verdict="$(printf '%s' "$out" | python3 -c '
import sys,json
try:
    d=json.load(sys.stdin)
    txt=(d.get("result") or d.get("error") or "").replace(chr(10)," ").strip()[:700]
    print(("OK" if not d.get("is_error") else "ERR")+chr(31)+(d.get("subtype") or "")+chr(31)+txt)
except Exception:
    print("RAW"+chr(31)+chr(31))' 2>/dev/null)"
flag="${verdict%%$'\x1f'*}"; rest="${verdict#*$'\x1f'}"; subtype="${rest%%$'\x1f'*}"; rtext="${rest#*$'\x1f'}"
if [ "$rc" -eq 0 ] && [ "$flag" = "OK" ]; then
  summary="$rtext"; [ -n "$summary" ] || summary="$out"
  summary="$(printf '%s' "$summary" | tr '\n' ' ' | cut -c1-700)"

  # —— 交付物:把 Claude 的真实改动快照成 commit 推远端 ——
  ref="claude://$AID"
  short="${AID%%-*}"
  branch="shepherd/deliver/$short"
  if [ -n "$BEFORE" ] && [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    stat="$(git diff --stat "$BEFORE" 2>/dev/null | tail -20)"
    # 串行化对 index 的操作(并发交付时避免相互踩),用 commit-tree 不切分支/不动工作区。
    sha=""
    {
      flock 9 || true
      git add -A 2>/dev/null
      tree="$(git write-tree 2>/dev/null)"
      git reset -q 2>/dev/null            # 还原暂存区,工作区保持不变
      if [ -n "$tree" ]; then
        sha="$(printf 'deliver(%s): %s\n\nattempt %s\n' "$short" "$title" "$AID" | git commit-tree "$tree" -p "$BEFORE" 2>/dev/null)"
        if [ -n "$sha" ]; then
          git branch -f "$branch" "$sha" 2>/dev/null
          git push -q -f origin "$branch" 2>/dev/null || event LOG "push $branch 失败(本地已建分支 $sha)"
        fi
      fi
    } 9>/tmp/shepherd-deliver.lock
    if [ -n "$sha" ]; then
      url="$(web_commit_url "$sha")"
      ref="${url:-git://$branch@$sha}"
      event FILE_CHANGE "已提交到分支 $branch($(printf '%s' "$sha" | cut -c1-9))"
      summary="变更:$(printf '%s' "$stat" | tr '\n' ';' | cut -c1-300) | $summary"
    else
      event FILE_CHANGE "Claude 有改动但快照失败(见工作区 git diff)"
    fi
  else
    event FILE_CHANGE "无代码变动(任务已实现或仅核验)"
    summary="(无代码变动)$summary"
  fi

  post "/delivery/$AID/complete" \
    "$(python3 -c 'import json,sys;print(json.dumps({"kind":"DIFF","reference":sys.argv[1],"summary":sys.argv[2]}))' "$ref" "$summary")"
else
  # 失败原因优先级:stdout 的 result/error 文本 → stderr → 原始 stdout 截断。
  estderr="$(tr '\n' ' ' <"$err" | cut -c1-300)"
  reason="$rtext"
  [ -n "$reason" ] || reason="$estderr"
  [ -n "$reason" ] || reason="$(printf '%s' "$out" | tr '\n' ' ' | cut -c1-300)"
  [ -n "$reason" ] || reason="无可读错误(stdout/stderr 皆空)"
  post "/delivery/$AID/fail" \
    "$(python3 -c 'import json,sys;print(json.dumps({"error":sys.argv[1]}))' "claude 失败(exit=$rc${subtype:+, $subtype}): $reason")"
fi
rm -f "$err"
