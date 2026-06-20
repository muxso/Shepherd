#!/bin/bash
# 真实流程产出富报告:建计划 → 挂用例 → 真实探测执行 → 回写结果(含断言)→ 取 HTML 报告。
set -uo pipefail
cd /Users/zhiyi/Code/rust/Shepherd
J() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)" 2>/dev/null; }

export DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest
MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret RUST_LOG=warn ./target/debug/server > /tmp/shep_server.log 2>&1 &
SRV=$!
RUNNER_BIND=127.0.0.1:9190 RUST_LOG=warn ./target/debug/runner-agent > /tmp/shep_agent.log 2>&1 &
AGT=$!
for i in $(seq 1 100); do curl -sf http://127.0.0.1:9180/healthz >/dev/null 2>&1 && break; sleep 0.5; done
for i in $(seq 1 40); do curl -sf http://127.0.0.1:9190/healthz >/dev/null 2>&1 && break; sleep 0.5; done
BASE=http://127.0.0.1:9180
TOKEN=$(curl -s -XPOST $BASE/auth/login -H 'content-type: application/json' -d '{"username":"admin","password":"s3cret"}' | J 'd["token"]')
AUTH="authorization: Bearer $TOKEN"
echo "server+agent up, token len=${#TOKEN}"
curl -s -XPOST $BASE/runner-agent -H "$AUTH" -H 'content-type: application/json' -d '{"name":"自举环境","baseUrl":"http://127.0.0.1:9190"}' >/dev/null

# 建计划
PID=$(curl -s -XPOST $BASE/test-plan -H "$AUTH" -H 'content-type: application/json' -d '{"projectId":"p1","name":"Shepherd 自举回归","type":"TEST_PLAN"}' | J 'd["id"]')
echo "plan=$PID"

# 一个用例 = 一次真实探测 + 回写结果(含断言)
run_case() {  # name  target  expect_status  expect_contains
  local name="$1" target="$2" code="$3" contains="$4" cid
  cid="case-$(echo "$name" | md5 -q | cut -c1-8)"
  curl -s -XPOST "$BASE/test-plan/$PID/cases" -H "$AUTH" -H 'content-type: application/json' \
    -d "{\"caseId\":\"$cid\",\"name\":\"$name\"}" >/dev/null
  # 真实探测(经中央 → agent → 目标)
  local pb assertJson="[{\"type\":\"status_is\",\"value\":$code}]"
  [ -n "$contains" ] && assertJson="[{\"type\":\"status_is\",\"value\":$code},{\"type\":\"output_contains\",\"value\":\"$contains\"}]"
  pb=$(curl -s -XPOST "$BASE/runner/probe" -H "$AUTH" -H 'content-type: application/json' \
    -d "{\"protocol\":\"http\",\"target\":\"$target\",\"assertions\":$assertJson}")
  # 解析探测结果 → 组装 record 请求(状态/耗时/状态码/响应体/断言行)
  python3 - "$cid" "$name" "$code" "$contains" "$pb" <<'PY' > /tmp/_rec.json
import sys,json
cid,name,code,contains,pb=sys.argv[1:6]
o=json.loads(pb)["outcome"]
ok=o["success"]
asserts=[{"item":"状态码","actual":str(o.get("status")),"condition":"等于","expected":code,
          "passed":str(o.get("status"))==code,"reason":"" if str(o.get("status"))==code else f"期望 {code},实际 {o.get('status')}"}]
if contains:
  body=o.get("output") or ""
  asserts.append({"item":"响应体","actual":body[:40],"condition":"包含","expected":contains,
                  "passed":contains in body,"reason":"" if contains in body else f"不含 {contains}"})
print(json.dumps({"status":"SUCCESS" if ok else "ERROR","latencyMs":o.get("latencyMs",0),
  "statusCode":o.get("status"),"body":o.get("output"),"assertions":asserts},ensure_ascii=False))
PY
  curl -s -XPOST "$BASE/test-plan/$PID/cases/$cid/result" -H "$AUTH" -H 'content-type: application/json' -d @/tmp/_rec.json >/dev/null
  echo "  · $name → $(J 'd["status"]' < /tmp/_rec.json)"
}

echo "=== 执行用例(真实探测)==="
run_case "基础健康检查"   "$BASE/healthz" 200 "ok"
run_case "就绪检查"       "$BASE/readyz"  200 ""
run_case "断言失败示例"   "$BASE/healthz" 500 ""
# 一个只挂不执行 → 未执行
curl -s -XPOST "$BASE/test-plan/$PID/cases" -H "$AUTH" -H 'content-type: application/json' -d '{"caseId":"case-pending","name":"待执行示例"}' >/dev/null

echo "=== statistics ==="
curl -s -H "$AUTH" $BASE/test-plan/$PID/statistics; echo
echo "=== cases ==="
curl -s -H "$AUTH" $BASE/test-plan/$PID/cases | python3 -m json.tool --no-ensure-ascii 2>/dev/null || curl -s -H "$AUTH" $BASE/test-plan/$PID/cases
echo "=== 取 HTML 报告 → docs/dogfood-report.html ==="
curl -s -H "$AUTH" $BASE/test-plan/$PID/report -o docs/dogfood-report.html -w "HTTP %{http_code} %{content_type} %{size_download} bytes\n"

kill $SRV $AGT 2>/dev/null
echo done
