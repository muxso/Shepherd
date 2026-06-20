#!/bin/bash
# 自举回归(真覆盖版):用 Shepherd 自己的接口用例 + 业务场景,经测试计划编排自测自身 REST API。
# 关键:建一个带鉴权头的环境,计划执行时注入 → 能自测**带鉴权**的接口(非只健康检查)。
# 前置:server 连的 PG 已就绪(见 memory: live-test-env-quirks)。
set -uo pipefail
cd "$(dirname "$0")/.."
J() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)" 2>/dev/null; }

export DATABASE_URL=${DATABASE_URL:-postgres://msuser:mspass@localhost:55432/mstest}
MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret RUST_LOG=warn ./target/debug/server > /tmp/shep_server.log 2>&1 &
SRV=$!
for i in $(seq 1 100); do curl -sf http://127.0.0.1:9180/healthz >/dev/null 2>&1 && break; sleep 0.5; done
BASE=http://127.0.0.1:9180
TOKEN=$(curl -s -XPOST $BASE/auth/login -H 'content-type: application/json' -d '{"username":"admin","password":"s3cret"}' | J 'd["token"]')
AUTH="authorization: Bearer $TOKEN"
PROJ="selfreg-$RANDOM"
echo "server up; token len=${#TOKEN}; proj=$PROJ"

# 1) 环境:base_url=本机 server + 默认头(鉴权 + JSON)。计划执行时注入每个请求。
ENVID=$(curl -s -XPOST $BASE/api/environment -H "$AUTH" -H 'content-type: application/json' -d "{
  \"projectId\":\"$PROJ\",\"name\":\"自举环境\",\"baseUrl\":\"$BASE\",
  \"headers\":[{\"name\":\"Authorization\",\"value\":\"Bearer $TOKEN\"},{\"name\":\"Content-Type\",\"value\":\"application/json\"}]
}" | J 'd["id"]')
echo "env=$ENVID"

# 2) 接口用例:打 Shepherd 自身各上下文端点(相对 url,env 拼 base_url + 注入鉴权)。
CASES=()
mkcase() { # name method relurl bodyJson assertionsJson
  local id
  id=$(curl -s -XPOST "$BASE/api/case" -H "$AUTH" -H 'content-type: application/json' -d "{
    \"projectId\":\"$PROJ\",\"name\":\"$1\",\"method\":\"$2\",\"url\":\"$3\",\"body\":$4,\"assertions\":$5}" | J 'd["id"]')
  CASES+=("$id"); echo "  + $1 ($2 $3) = ${id:0:8}"
}
S200='[{"type":"StatusIs","args":200}]'
mkcase "健康检查"     GET  /healthz       null "$(echo '[{"type":"StatusIs","args":200},{"type":"BodyContains","args":"ok"}]')"
mkcase "就绪检查"     GET  /readyz        null "$S200"
mkcase "OpenAPI"     GET  /api-docs/openapi.json  null '[{"type":"StatusIs","args":200},{"type":"BodyContains","args":"openapi"}]'
mkcase "项目列表"     GET  "/project?organizationId=$PROJ" null "$S200"
mkcase "用户列表"     GET  "/system/user?current=1&pageSize=5" null "$S200"
mkcase "环境列表"     GET  "/api/environment?projectId=$PROJ" null "$S200"
mkcase "建项目"       POST /project       "\"{\\\"organizationId\\\":\\\"$PROJ\\\",\\\"name\\\":\\\"P\\\"}\"" '[{"type":"StatusIs","args":201}]'
mkcase "建需求"       POST /requirement   "\"{\\\"projectId\\\":\\\"$PROJ\\\",\\\"title\\\":\\\"登录\\\",\\\"acceptanceCriteria\\\":[\\\"登录成功\\\"]}\"" '[{"type":"StatusIs","args":201},{"type":"BodyContains","args":"id"}]'
mkcase "建测试计划"   POST /test-plan     "\"{\\\"projectId\\\":\\\"$PROJ\\\",\\\"name\\\":\\\"P\\\",\\\"type\\\":\\\"TEST_PLAN\\\"}\"" '[{"type":"StatusIs","args":201}]'
mkcase "建技能"       POST /skill         "\"{\\\"projectId\\\":\\\"$PROJ\\\",\\\"name\\\":\\\"S\\\",\\\"instructions\\\":\\\"x\\\"}\"" '[{"type":"StatusIs","args":201}]'
mkcase "建缺陷"       POST /bug           "\"{\\\"projectId\\\":\\\"$PROJ\\\",\\\"title\\\":\\\"b\\\",\\\"initialStatus\\\":\\\"NEW\\\"}\"" '[{"type":"StatusIs","args":201},{"type":"BodyContains","args":"NEW"}]'
mkcase "建场景"       POST /api/scenario  "\"{\\\"projectId\\\":\\\"$PROJ\\\",\\\"name\\\":\\\"Sc\\\"}\"" '[{"type":"StatusIs","args":201}]'

# 3) 业务场景(多步编排):CASE(建需求)→ 内联 REQUEST(健康检查)。
SCN=$(curl -s -XPOST $BASE/api/scenario -H "$AUTH" -H 'content-type: application/json' -d "{\"projectId\":\"$PROJ\",\"name\":\"需求冒烟编排\"}" | J 'd["id"]')
curl -s -XPOST "$BASE/api/scenario/$SCN/step" -H "$AUTH" -H 'content-type: application/json' -d "{\"kind\":\"CASE\",\"order\":1,\"refId\":\"${CASES[0]}\"}" >/dev/null
curl -s -XPOST "$BASE/api/scenario/$SCN/step" -H "$AUTH" -H 'content-type: application/json' -d "{\"kind\":\"REQUEST\",\"order\":2,\"request\":{\"method\":\"GET\",\"url\":\"/healthz\",\"assertions\":[{\"type\":\"StatusIs\",\"args\":200}]}}" >/dev/null
echo "scenario=$SCN"

# 4) 测试计划:挂入全部用例 + 业务场景。
PID=$(curl -s -XPOST $BASE/test-plan -H "$AUTH" -H 'content-type: application/json' -d "{\"projectId\":\"$PROJ\",\"name\":\"Shepherd 自举回归(全量)\",\"type\":\"TEST_PLAN\"}" | J 'd["id"]')
i=1; for c in "${CASES[@]}"; do curl -s -XPOST "$BASE/test-plan/$PID/cases" -H "$AUTH" -H 'content-type: application/json' -d "{\"caseId\":\"$c\",\"name\":\"用例$i\"}" >/dev/null; i=$((i+1)); done
curl -s -XPOST "$BASE/test-plan/$PID/cases" -H "$AUTH" -H 'content-type: application/json' -d "{\"caseId\":\"$SCN\",\"name\":\"需求冒烟编排\"}" >/dev/null
echo "plan=$PID linked ${#CASES[@]} 用例 + 1 场景"

# 5) 执行(注入环境=鉴权)→ 统计 + 报告。
echo "=== run(environmentId=$ENVID)==="
curl -s -XPOST "$BASE/test-plan/$PID/run" -H "$AUTH" -H 'content-type: application/json' -d "{\"environmentId\":\"$ENVID\"}"; echo
echo "=== statistics ==="; curl -s -H "$AUTH" $BASE/test-plan/$PID/statistics; echo
echo "=== 各用例结果 ==="; curl -s -H "$AUTH" $BASE/test-plan/$PID/cases | python3 -c 'import sys,json;[print(f"  {c[\"status\"]:8} {c[\"name\"]}") for c in json.load(sys.stdin)]' 2>/dev/null
echo "=== Markdown 报告 → docs/self-regression.md ==="; mkdir -p docs
curl -s -H "$AUTH" "$BASE/test-plan/$PID/report.md" -o docs/self-regression.md -w "HTTP %{http_code} %{content_type} %{size_download} bytes\n"

kill $SRV 2>/dev/null
echo done
