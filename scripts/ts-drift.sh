#!/usr/bin/env bash
# 对账:后端 OpenAPI 路径 vs 前端手写客户端(web/src/api.ts)引用的路径。
# 前端引用了 OpenAPI 里不存在的路径 → 大概率拼写/漂移,退出码 1;
# 反向(后端有、前端没用)只作信息输出(CLI/MCP 专用端点属正常)。
# 需要一个在跑的 server:BASE=http://127.0.0.1:9180 bash scripts/ts-drift.sh
set -euo pipefail
BASE="${BASE:-http://127.0.0.1:9180}"
API_TS="${API_TS:-web/src/api.ts}"

SPEC_FILE=$(mktemp)
trap 'rm -f "$SPEC_FILE"' EXIT
curl -sf "$BASE/api-docs/openapi.json" -o "$SPEC_FILE" || { echo "无法获取 $BASE/api-docs/openapi.json"; exit 2; }

python3 - "$API_TS" "$SPEC_FILE" <<'EOF'
import json, re, sys

spec = json.load(open(sys.argv[2]))
# 模板段归一:/bug/{id}/relation → /bug/*/relation
def norm(p):
    # 顺序敏感:先替换 TS 模板 ${...},再替换 OpenAPI 的 {...},否则 ${x} 会残留 "$"。
    p = re.sub(r"\$\{[^}]+\}", "*", p)
    p = re.sub(r"\{[^}]+\}", "*", p)
    return p.rstrip("/")

openapi = {norm(p) for p in spec.get("paths", {})}

src = open(sys.argv[1]).read()
# 提取 http.get/post/put/del/patch/getText/getBlob/upload 的首个字符串/模板参数里的路径
used = set()
for m in re.finditer(r"http\.(?:get|post|put|del|patch|getText|getBlob|upload)\s*(?:<[^>]*>)?\(\s*[`'\"]([^`'\"?]+)", src):
    path = m.group(1).split("?")[0]
    if path.startswith("/"):
        used.add(norm(path))

unknown = sorted(u for u in used if u not in openapi)
unused = sorted(o for o in openapi if o not in used)

# 基线:历史缺口(路由存在但未注册进 utoipa,待补 OpenAPI);只有新增漂移才挡板。
import os
allow_file = os.environ.get("DRIFT_ALLOW", "scripts/ts-drift-allow.txt")
allowed = set()
if os.path.exists(allow_file):
    allowed = {l.strip() for l in open(allow_file) if l.strip() and not l.startswith("#")}
fresh = [u for u in unknown if u not in allowed]
stale = sorted(allowed - set(unknown))

print(f"OpenAPI 路径 {len(openapi)} 个;api.ts 引用 {len(used)} 个;基线缺口 {len(allowed & set(unknown))} 个")
if unused:
    print(f"\n[信息] 后端有、前端未引用({len(unused)} 个,CLI/MCP 专用属正常):")
    for p in unused: print(f"  {p}")
if stale:
    print(f"\n[信息] 基线里已修复的条目,可从 ts-drift-allow.txt 删除({len(stale)} 个):")
    for p in stale: print(f"  {p}")
if fresh:
    print(f"\n[漂移] api.ts 引用了 OpenAPI 中不存在且不在基线里的路径({len(fresh)} 个):")
    for p in fresh: print(f"  {p}")
    sys.exit(1)
print("\n对账通过:无新增漂移。")
EOF
