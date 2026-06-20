#!/bin/bash
# 全量 API 自测:对 Shepherd 暴露的(几乎)所有 REST 端点做编排自测(建资源→串 id→打子端点→断状态码)。
# 覆盖:需求/任务/测试用例(接口+功能)/场景用例/交付/验证/技能/缺陷/项目组织角色/环境/资源池/
#       测试计划/压测/runner/MCP/健康/OpenAPI。前置:PG 已就绪(见 memory: live-test-env-quirks)。
set -uo pipefail
cd "$(dirname "$0")/.."
J() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)" 2>/dev/null; }
export DATABASE_URL=${DATABASE_URL:-postgres://msuser:mspass@localhost:55432/mstest}
MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret RUST_LOG=warn ./target/debug/server > /tmp/shep_server.log 2>&1 &
SRV=$!; trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 100); do curl -sf http://127.0.0.1:9180/healthz >/dev/null 2>&1 && break; sleep 0.5; done
B=http://127.0.0.1:9180
TOK=$(curl -s -XPOST $B/auth/login -H 'content-type: application/json' -d '{"username":"admin","password":"s3cret"}' | J 'd["token"]')
A="authorization: Bearer $TOK"
PJ="cov-$RANDOM"

T=0; P=0; F=0
code() { local m=$1 p=$2 b=${3:-}; if [ -n "$b" ]; then curl -s -o /dev/null -w '%{http_code}' -X "$m" -H "$A" -H 'content-type: application/json' "$B$p" -d "$b"; else curl -s -o /dev/null -w '%{http_code}' -X "$m" -H "$A" "$B$p"; fi; }
idof() { curl -s -X "$1" -H "$A" -H 'content-type: application/json' "$B$2" -d "${3:-}" | J 'd.get("id","")'; }
ok() { local d=$1 exp=$2; shift 2; local c; c=$(code "$@"); T=$((T+1)); if [[ " $exp " == *" $c "* ]]; then P=$((P+1)); else F=$((F+1)); echo "  ✗ $d → $c (want: $exp)"; fi; }
SX="200 201"   # 成功(2xx,创建多为 201)
echo "server up; proj=$PJ"

echo "== 健康 / OpenAPI / 鉴权 =="
ok "GET /healthz"             "$SX" GET /healthz
ok "GET /readyz"             "$SX" GET /readyz
ok "GET /api-docs/openapi.json" "$SX" GET /api-docs/openapi.json
ok "GET /swagger-ui"         "200 301 302 307" GET /swagger-ui
ok "POST /auth/login(错密码→401)" "401" POST /auth/login '{"username":"admin","password":"bad"}'

echo "== 组织 / 角色 / 用户 / 项目 =="
OID=$(idof POST /organization "{\"name\":\"$PJ-org\"}")
ok "POST /organization"      "$SX" POST /organization "{\"name\":\"$PJ-o2\"}"
ok "GET /organization"       "$SX" GET /organization
ok "GET /organization/{id}"  "$SX" GET "/organization/$OID"
RID2=$(idof POST /role "{\"name\":\"$PJ-role\",\"permissions\":[\"PROJECT:READ\"]}")
ok "POST /role"              "$SX" POST /role "{\"name\":\"$PJ-r2\",\"permissions\":[]}"
ok "GET /role"               "$SX" GET /role
ok "GET /role/{id}"          "$SX" GET "/role/$RID2"
USERID=$(idof POST /system/user "{\"name\":\"$PJ-u\",\"email\":\"$PJ@x.com\"}")
ok "GET /system/user"        "$SX" GET "/system/user?current=1&pageSize=5"
ok "GET /system/user/{id}"   "$SX" GET "/system/user/$USERID"
ok "GET /system/user/names"  "$SX" GET "/system/user/names?ids=$USERID"
ok "POST /user-role/grant"   "$SX 204" POST /user-role/grant "{\"userId\":\"$USERID\",\"roleId\":\"$RID2\"}"
ok "POST /user-role/revoke"  "$SX 204" POST /user-role/revoke "{\"userId\":\"$USERID\",\"roleId\":\"$RID2\"}"
ok "POST /project"           "$SX" POST /project "{\"organizationId\":\"$PJ\",\"name\":\"P\"}"
ok "GET /project"            "$SX" GET "/project?organizationId=$PJ"

echo "== 需求 =="
RID=$(idof POST /requirement "{\"projectId\":\"$PJ\",\"title\":\"登录\",\"acceptanceCriteria\":[\"登录成功\",\"错误密码拒绝\"]}")
ok "GET /requirement/{id}"   "$SX" GET "/requirement/$RID"
ok "POST /requirement/{id}/version" "$SX" POST "/requirement/$RID/version" '{"description":"v2","acceptanceCriteria":["登录成功","错误密码拒绝"]}'
ok "GET /requirement/{id}/version/1" "$SX" GET "/requirement/$RID/version/1"
ok "PUT /requirement/{id}/baseline"  "$SX" PUT "/requirement/$RID/baseline" '{"version":2}'

echo "== 任务 / 拆分图 =="
BD=$(curl -s -XPOST $B/requirement/$RID/breakdown -H "$A" -H 'content-type: application/json' -d '{}')   # 自动拆分(并自动开验证账本)
DID=$(echo "$BD" | J 'd["id"]'); VID=$(echo "$BD" | J 'd["verificationId"]')
ok "POST /decomposition(已存在→409)" "$SX 409" POST /decomposition "{\"requirementId\":\"$RID\",\"requirementVersion\":2}"
ok "GET /decomposition/{id}" "$SX" GET "/decomposition/$DID"
ok "GET /decomposition/{id}/ready" "$SX" GET "/decomposition/$DID/ready"
ok "POST /decomposition/{id}/task" "$SX" POST "/decomposition/$DID/task" '{"title":"额外任务","acceptanceCriteria":["x"],"dependencies":[]}'
ok "POST .../task/{tid}/dispatch"  "$SX 409" POST "/decomposition/$DID/task/t1/dispatch" '{"executor":"CLAUDE_CODE"}'
ok "POST .../task/{tid}/status"    "$SX 409" POST "/decomposition/$DID/task/t1/status" '{"status":"RUNNING"}'
ok "POST /decomposition/{id}/run"  "$SX" POST "/decomposition/$DID/run" '{}'
ok "POST /decomposition/breakdown(已存在→409)" "$SX 409" POST /decomposition/breakdown "{\"requirementId\":\"$RID\",\"requirementVersion\":2,\"title\":\"登录\",\"acceptanceCriteria\":[\"登录成功\"]}"

echo "== 交付 =="
AID=$(idof POST /delivery "{\"decompositionId\":\"$DID\",\"taskId\":\"t1\",\"title\":\"实现\",\"executor\":\"CLAUDE_CODE\"}")
ok "GET /delivery/{id}"      "$SX" GET "/delivery/$AID"
ok "GET /delivery?taskId"    "$SX" GET "/delivery?decompositionId=$DID&taskId=t1"
ok "POST /delivery/{id}/events(记录)" "$SX" POST "/delivery/$AID/events" '{"kind":"DECISION","message":"x"}'
ok "GET /delivery/{id}/events"        "$SX" GET "/delivery/$AID/events"

echo "== 验证 =="
ok "GET /verification/{id}"  "$SX" GET "/verification/$VID"
ok "POST /verification/{id}/link" "$SX" POST "/verification/$VID/link" "{\"criterionIndex\":0,\"decompositionId\":\"$DID\",\"taskId\":\"t1\"}"
ok "POST /verification/{id}/sync" "$SX" POST "/verification/$VID/sync" "{\"decompositionId\":\"$DID\",\"taskId\":\"t1\",\"satisfied\":true}"
ok "GET /verification/{id}/report" "$SX" GET "/verification/$VID/report"

echo "== 技能 / 缺陷 =="
SID=$(idof POST /skill "{\"projectId\":\"$PJ\",\"name\":\"基础\",\"instructions\":\"遵循六边形\"}")
ok "GET /skill/{id}"         "$SX" GET "/skill/$SID"
ok "POST /skill/compose"     "$SX" POST /skill/compose "{\"projectId\":\"$PJ\",\"skillIds\":[\"$SID\"]}"
BID=$(idof POST /bug "{\"projectId\":\"$PJ\",\"title\":\"b\",\"initialStatus\":\"NEW\"}")
ok "POST /bug/{id}/status"   "$SX" POST "/bug/$BID/status" '{"status":"RESOLVED"}'

echo "== 功能用例 =="
ok "POST /functional-case"   "$SX" POST /functional-case "{\"projectId\":\"$PJ\",\"name\":\"功能1\"}"
ok "GET /functional-case"    "$SX" GET "/functional-case?projectId=$PJ"
ok "GET /functional-case/export" "$SX" GET "/functional-case/export?projectId=$PJ"

echo "== 接口定义 / 接口用例 =="
DEF=$(idof POST /api/definition "{\"projectId\":\"$PJ\",\"name\":\"登录API\",\"protocol\":\"HTTP\",\"method\":\"POST\",\"path\":\"/login\"}")
ok "GET /api/definition"     "$SX" GET "/api/definition?projectId=$PJ"
ok "GET /api/definition/{id}" "$SX" GET "/api/definition/$DEF"
ok "POST /api/definition/{id}/case" "$SX" POST "/api/definition/$DEF/case" "{\"name\":\"c200\",\"method\":\"GET\",\"url\":\"$B/healthz\"}"
ok "POST /api/definition/{id}/mock" "$SX" POST "/api/definition/$DEF/mock" '{"name":"m200","responseStatus":200}'
CASE=$(idof POST /api/case "{\"projectId\":\"$PJ\",\"name\":\"健康\",\"method\":\"GET\",\"url\":\"$B/healthz\",\"assertions\":[{\"type\":\"StatusIs\",\"args\":200}]}")
ok "GET /api/case"           "$SX" GET "/api/case?projectId=$PJ"
ok "GET /api/case/{id}/executions" "$SX" GET "/api/case/$CASE/executions"
ok "POST /api/case/{id}/run" "$SX 400 502" POST "/api/case/$CASE/run" "{\"projectId\":\"$PJ\",\"runMode\":\"PARALLEL\"}"

echo "== 场景用例 =="
SCN=$(idof POST /api/scenario "{\"projectId\":\"$PJ\",\"name\":\"登录冒烟\"}")
ok "GET /api/scenario"       "$SX" GET "/api/scenario?projectId=$PJ"
ok "GET /api/scenario/{id}"  "$SX" GET "/api/scenario/$SCN"
ok "POST /api/scenario/{id}/step(CASE)" "$SX" POST "/api/scenario/$SCN/step" "{\"kind\":\"CASE\",\"order\":1,\"refId\":\"$CASE\"}"
ok "POST /api/scenario/{id}/step(REQUEST)" "$SX" POST "/api/scenario/$SCN/step" '{"kind":"REQUEST","order":2,"request":{"method":"GET","url":"/healthz","assertions":[{"type":"StatusIs","args":200}]}}'
ok "GET /api/scenario/{id}/compile" "$SX" GET "/api/scenario/$SCN/compile"
ok "POST /api/scenario/{id}/run"    "$SX" POST "/api/scenario/$SCN/run" "{\"projectId\":\"$PJ\",\"runMode\":\"PARALLEL\"}"
ok "GET /api/scenario/{id}/executions" "$SX" GET "/api/scenario/$SCN/executions"

echo "== 环境 / 资源池 / 批量 =="
ENV=$(idof POST /api/environment "{\"projectId\":\"$PJ\",\"name\":\"env\",\"baseUrl\":\"$B\"}")
ok "GET /api/environment"    "$SX" GET "/api/environment?projectId=$PJ"
ok "PUT /api/environment/{id}" "$SX" PUT "/api/environment/$ENV" "{\"projectId\":\"$PJ\",\"name\":\"env2\",\"baseUrl\":\"$B\"}"
ok "POST /api/resource-pool" "$SX" POST /api/resource-pool '{"name":"本地池"}'
ok "GET /api/resource-pool"  "$SX" GET /api/resource-pool
ok "POST /api/batch-run"     "$SX 400" POST /api/batch-run "{\"projectId\":\"$PJ\",\"caseIds\":[\"$CASE\"],\"runMode\":\"PARALLEL\"}"

echo "== 测试计划 =="
TP=$(idof POST /test-plan "{\"projectId\":\"$PJ\",\"name\":\"计划\",\"type\":\"TEST_PLAN\"}")
ok "POST /test-plan/{id}/cases" "$SX" POST "/test-plan/$TP/cases" "{\"caseId\":\"$CASE\",\"name\":\"健康\"}"
ok "POST .../cases/{cid}/result" "$SX" POST "/test-plan/$TP/cases/$CASE/result" '{"status":"SUCCESS","latencyMs":1,"statusCode":200}'
ok "GET /test-plan/{id}/statistics" "$SX" GET "/test-plan/$TP/statistics"
ok "GET /test-plan/{id}/report"  "$SX" GET "/test-plan/$TP/report"
ok "GET /test-plan/{id}/report.md" "$SX" GET "/test-plan/$TP/report.md"
ok "POST /test-plan/{id}/run"    "$SX" POST "/test-plan/$TP/run" '{}'
ok "POST /test-plan/{id}/schedule" "$SX" POST "/test-plan/$TP/schedule" '{"cron":"0 0 * * * *"}'
ok "GET /test-plan/{id}/runs"    "$SX" GET "/test-plan/$TP/runs"

echo "== 压测 / runner / MCP =="
PRID=$(curl -s -XPOST $B/perf/run -H "$A" -H 'content-type: application/json' -d "{\"url\":\"$B/healthz\",\"concurrency\":2,\"iterations\":4}" | J 'd["reportId"]')
ok "POST /perf/run"          "$SX" POST /perf/run "{\"url\":\"$B/healthz\",\"concurrency\":2,\"iterations\":4}"
ok "GET /perf/report/{id}"   "$SX" GET "/perf/report/$PRID"
AG=$(idof POST /runner-agent "{\"name\":\"env\",\"baseUrl\":\"http://127.0.0.1:9190\"}")
ok "GET /runner-agent"       "$SX" GET /runner-agent
ok "GET /runner-agent/{id}/executions" "$SX" GET "/runner-agent/$AG/executions"
ok "POST /runner-agent/{id}/refresh(无 agent→502)" "$SX 502" POST "/runner-agent/$AG/refresh" '{}'
ok "POST /runner-agent/{id}/run(无 agent→502)" "$SX 502" POST "/runner-agent/$AG/run" '{"request":{"method":"GET","url":"http://127.0.0.1:1/x","headers":[],"body":null}}'
ok "POST /runner-agent/{id}/run-case(无 agent→404/502)" "$SX 404 502" POST "/runner-agent/$AG/run-case" '{"caseId":"x"}'
ok "POST /runner/probe(agent 不可达→502)" "$SX 404 502" POST /runner/probe '{"protocol":"http","target":"http://127.0.0.1:1/x"}'
ok "POST /mcp(tools/list)"   "$SX" POST /mcp '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

echo
echo "############ 全量 API 自测:覆盖 $T 个端点,通过 $P,失败 $F ############"
[ "$F" -eq 0 ] && echo "✅ 全绿" || echo "❌ 有失败"
exit $F
