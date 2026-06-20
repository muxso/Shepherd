#!/bin/bash
# 自举:全自动测试计划报告。
# 建真实 api-case → 挂入计划 → 跑计划(自动执行+回写结果)→ 取富 HTML 报告。
# 前置:server 连的 PG 已就绪(本机 docker CLI 偶发卡死,见 memory: live-test-env-quirks)。
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
echo "server up; token len=${#TOKEN}"

# 建真实 api-case(指向 server 自身端点 + 断言),返回 case id
mkcase() { # name method url assertionsJson
  curl -s -XPOST "$BASE/api/case" -H "$AUTH" -H 'content-type: application/json' \
    -d "{\"projectId\":\"p1\",\"name\":\"$1\",\"method\":\"$2\",\"url\":\"$3\",\"assertions\":$4}" | J 'd["id"]'
}
C1=$(mkcase "基础健康检查" GET "$BASE/healthz" '[{"type":"StatusIs","args":200},{"type":"BodyContains","args":"ok"}]')
C2=$(mkcase "就绪检查"     GET "$BASE/readyz"  '[{"type":"StatusIs","args":200}]')
C3=$(mkcase "断言失败示例" GET "$BASE/healthz" '[{"type":"StatusIs","args":500}]')
echo "cases: $C1 $C2 $C3"

# 建一个场景(CASE 步骤引用 C1 + 内联 REQUEST 带断言)→ 展示报告里的嵌套步骤树
SC=$(curl -s -XPOST $BASE/api/scenario -H "$AUTH" -H 'content-type: application/json' -d '{"projectId":"p1","name":"健康检查场景"}' | J 'd["id"]')
curl -s -XPOST "$BASE/api/scenario/$SC/step" -H "$AUTH" -H 'content-type: application/json' -d "{\"kind\":\"CASE\",\"order\":1,\"refId\":\"$C1\"}" >/dev/null
curl -s -XPOST "$BASE/api/scenario/$SC/step" -H "$AUTH" -H 'content-type: application/json' -d "{\"kind\":\"REQUEST\",\"order\":2,\"request\":{\"method\":\"GET\",\"url\":\"$BASE/readyz\",\"assertions\":[{\"type\":\"StatusIs\",\"args\":200}]}}" >/dev/null
echo "scenario=$SC"

# 建计划 + 挂入用例 + 挂入场景
PID=$(curl -s -XPOST $BASE/test-plan -H "$AUTH" -H 'content-type: application/json' -d '{"projectId":"p1","name":"Shepherd 自举回归","type":"TEST_PLAN"}' | J 'd["id"]')
for pair in "$C1:基础健康检查" "$C2:就绪检查" "$C3:断言失败示例" "$SC:健康检查场景"; do
  cid="${pair%%:*}"; cname="${pair##*:}"
  curl -s -XPOST "$BASE/test-plan/$PID/cases" -H "$AUTH" -H 'content-type: application/json' -d "{\"caseId\":\"$cid\",\"name\":\"$cname\"}" >/dev/null
done
echo "plan=$PID linked 3 cases + 1 scenario"

# 跑计划:自动执行 + 回写结果
echo "=== POST /test-plan/$PID/run(自动执行+回写)==="
curl -s -XPOST "$BASE/test-plan/$PID/run" -H "$AUTH" -H 'content-type: application/json' -d '{}'; echo
echo "=== statistics ==="
curl -s -H "$AUTH" $BASE/test-plan/$PID/statistics; echo
echo "=== 富 HTML 报告 → docs/dogfood-report.html ==="
mkdir -p docs
curl -s -H "$AUTH" $BASE/test-plan/$PID/report -o docs/dogfood-report.html -w "HTTP %{http_code} %{content_type} %{size_download} bytes\n"

kill $SRV 2>/dev/null
echo done
