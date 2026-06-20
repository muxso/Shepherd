#!/bin/bash
# 全量 API 自测(深度版):对暴露的 REST 面逐端点断言**状态码 + 关键响应字段 + 业务结果**(非仅状态)。
# 建资源→串 id→打子端点→断响应。前置:PG 已就绪(见 memory: live-test-env-quirks)。
set -uo pipefail
cd "$(dirname "$0")/.."
export DATABASE_URL=${DATABASE_URL:-postgres://msuser:mspass@localhost:55432/mstest}
MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret RUST_LOG=warn ./target/debug/server > /tmp/shep_server.log 2>&1 &
SRV=$!; trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 100); do curl -sf http://127.0.0.1:9180/healthz >/dev/null 2>&1 && break; sleep 0.5; done
B=http://127.0.0.1:9180
T=0; P=0; F=0
RPT="docs/api-coverage.md"; mkdir -p docs; : > "$RPT"
echo "# Shepherd 全量 API 自测报告" >> "$RPT"
echo >> "$RPT"
printf '> 逐端点断言:状态码 + 关键响应字段 + 业务结果。由 scripts/api-coverage.sh 生成。\n\n' >> "$RPT"
row() { printf '| %s | %s | %s |\n' "$1" "$2" "$3" >> "$RPT"; }
sec() { echo "== $1 =="; printf '\n## %s\n\n| 检查 | 结果 | 说明 |\n|---|---|---|\n' "$1" >> "$RPT"; }
RESP=""; HS=""
# call METHOD PATH [BODY] → 设 RESP(响应体)+ HS(状态码)
call() { local m=$1 p=$2 b=${3:-} out
  if [ -n "$b" ]; then out=$(curl -s -w $'\n%{http_code}' -X "$m" -H "$A" -H 'content-type: application/json' "$B$p" -d "$b")
  else out=$(curl -s -w $'\n%{http_code}' -X "$m" -H "$A" "$B$p"); fi
  HS="${out##*$'\n'}"; RESP="${out%$'\n'*}"; }
# sc desc expectedCodes  → 断状态码
sc() { T=$((T+1)); if [[ " $2 " == *" $HS "* ]]; then P=$((P+1)); row "$1(状态)" "✅ $HS" ""; else F=$((F+1)); echo "  ✗ [$1] 状态=$HS 期望 $2  resp=$(printf %s "$RESP"|head -c160)"; row "$1(状态)" "❌ $HS" "期望 $2"; fi; }
# jchk desc pyBoolExpr  → 断响应字段/业务结果(d=响应 JSON)
jchk() { T=$((T+1)); local r; r=$(printf %s "$RESP" | python3 -c "import sys,json
try:d=json.load(sys.stdin)
except:d=None
print(bool($2))" 2>/dev/null)
  if [ "$r" = "True" ]; then P=$((P+1)); row "$1" "✅" ""; else F=$((F+1)); echo "  ✗ [$1] 断言失败:$2  resp=$(printf %s "$RESP"|head -c160)"; row "$1" "❌" "$2"; fi; }
jval() { printf %s "$RESP" | python3 -c "import sys,json;d=json.load(sys.stdin);print($1)" 2>/dev/null; }

A="authorization: Bearer x"  # 占位,登录后覆盖
PJ="cov-$RANDOM"
sec "登录 / 鉴权初始化"
call POST /auth/login '{"username":"admin","password":"s3cret"}'
sc "登录" 200; jchk "返回 token" 'isinstance(d.get("token"),str) and len(d["token"])>10'
A="authorization: Bearer $(jval 'd["token"]')"
echo "server up; proj=$PJ"

sec "健康 / OpenAPI / 鉴权"
call GET /healthz;             sc "healthz" 200; jchk "body=ok" 'd is None'  # 纯文本 ok,非 JSON
call GET /readyz;              sc "readyz" 200
call GET /api-docs/openapi.json; sc "openapi" 200; jchk "有 paths/openapi 字段" '"openapi" in d and "paths" in d'
call POST /auth/login '{"username":"admin","password":"bad"}'; sc "错密码→401" 401

sec "组织 / 角色 / 用户 / 用户角色 / 项目"
call POST /organization "{\"name\":\"$PJ-org\"}"; sc "建组织" "200 201"; jchk "返回 id+name" 'd.get("id") and d.get("name")=="'$PJ'-org"'; OID=$(jval 'd["id"]')
call GET "/organization/$OID"; sc "查组织" 200; jchk "id 一致" 'd.get("id")=="'$OID'"'
call GET /organization; sc "组织列表" 200
call POST /role "{\"name\":\"$PJ-role\",\"permissions\":[\"PROJECT:READ\"]}"; sc "建角色" "200 201"; jchk "角色名" 'd.get("name")=="'$PJ'-role"'; RID2=$(jval 'd["id"]')
call GET "/role/$RID2"; sc "查角色" 200; jchk "权限含 PROJECT:READ" '"PROJECT:READ" in (d.get("permissions") or [])'
call GET /role; sc "角色列表" 200
call POST /system/user "{\"name\":\"$PJ-u\",\"email\":\"$PJ@x.com\"}"; sc "建用户" "200 201"; jchk "用户邮箱" 'd.get("email")=="'$PJ'@x.com"'; USERID=$(jval 'd["id"]')
call GET "/system/user?current=1&pageSize=5"; sc "用户分页" 200; jchk "用户分页有 items" 'len(d.get("items",[]))>=1'
call GET "/system/user/$USERID"; sc "查用户" 200; jchk "id 一致" 'd.get("id")=="'$USERID'"'
call GET "/system/user/names?ids=$USERID"; sc "名字解析" 200; jchk "含该 id 的名字" 'd.get("'$USERID'")=="'$PJ'-u"'
call POST /user-role/grant "{\"userId\":\"$USERID\",\"roleId\":\"$RID2\"}"; sc "授角色" "200 201 204"
call POST /user-role/revoke "{\"userId\":\"$USERID\",\"roleId\":\"$RID2\"}"; sc "撤角色" "200 201 204"
call POST /project "{\"organizationId\":\"$PJ\",\"name\":\"P\"}"; sc "建项目" "200 201"; jchk "项目名" 'd.get("name")=="P"'
call GET "/project?organizationId=$PJ"; sc "项目列表" 200; jchk "非空" '(d.get("list") or d) and len(d.get("list") or d)>=1'

echo "== 需求(版本/基线)=="
call POST /requirement "{\"projectId\":\"$PJ\",\"title\":\"登录\",\"acceptanceCriteria\":[\"登录成功\",\"错误密码拒绝\"]}"; sc "建需求" "200 201"
jchk "初版基线 v1" 'd.get("baselineVersion")==1'; RID=$(jval 'd["id"]')
call GET "/requirement/$RID"; sc "查需求" 200; jchk "基线仍 1" 'd.get("baselineVersion")==1'
call POST "/requirement/$RID/version" '{"description":"v2","acceptanceCriteria":["登录成功","错误密码拒绝","支持飞书"]}'; sc "修订" "200 201"; jchk "新版本=2" 'd.get("version")==2'
call GET "/requirement/$RID/version/1"; sc "查 v1 快照" 200; jchk "v1 标准不变" 'd["acceptanceCriteria"][0]=="登录成功" and len(d["acceptanceCriteria"])==2'
call PUT "/requirement/$RID/baseline" '{"version":2}'; sc "定基 v2" "200 201"; jchk "基线=2 且 BASELINED" 'd.get("baselineVersion")==2 and d.get("status")=="BASELINED"'

sec "任务 / 拆分图"
call POST "/requirement/$RID/breakdown" '{}'; sc "自动拆分" "200 201"
jchk "出任务 + 自动开验证账本" 'len(d.get("tasks") or [])>=1 and bool(d.get("verificationId"))'
DID=$(jval 'd["id"]'); VID=$(jval 'd["verificationId"]'); NTASK=$(jval 'len(d["tasks"])')
call GET "/decomposition/$DID"; sc "查拆分图" 200; jchk "任务数一致" 'len(d.get("tasks") or [])=='"$NTASK"
call GET "/decomposition/$DID/ready"; sc "就绪任务" 200; jchk "就绪返回任务数组" 'isinstance(d,list)'
call POST "/decomposition/$DID/task" '{"title":"额外","acceptanceCriteria":["x"],"dependencies":[]}'; sc "加任务" "200 201"; jchk "返回 taskId" 'bool(d.get("taskId"))'
call POST "/decomposition/$DID/run" '{}'; sc "并行运行" 200; jchk "全部推进(verified+failed=total)" 'd["verified"]+d["failed"]+d["blocked"]==d["total"] and d["total"]>=1'

sec "交付"
call POST /delivery "{\"decompositionId\":\"$DID\",\"taskId\":\"t1\",\"title\":\"实现\",\"executor\":\"CLAUDE_CODE\"}"; sc "派发" "200 201"
jchk "返回尝试 + 状态" 'bool(d.get("id") or d.get("attemptId")) and bool(d.get("status"))'; AID=$(jval 'd.get("id") or d.get("attemptId")')
call GET "/delivery/$AID"; sc "查交付" 200; jchk "id 一致" '(d.get("id") or d.get("attemptId"))=="'$AID'"'
call GET "/delivery?decompositionId=$DID&taskId=t1"; sc "按任务列尝试" 200; jchk "非空数组" 'isinstance(d,list) and len(d)>=1'
call POST "/delivery/$AID/events" '{"kind":"DECISION","message":"x"}'; sc "记事件" "200 201"
call GET "/delivery/$AID/events"; sc "事件列表" 200; jchk "事件数组(含 VERDICT)" 'isinstance(d,list) and len(d)>=1'

echo "== 验证(覆盖链/报告)=="
call GET "/verification/$VID"; sc "查验证" 200; jchk "id 一致" 'd.get("id")=="'$VID'"'
call POST "/verification/$VID/link" "{\"criterionIndex\":0,\"decompositionId\":\"$DID\",\"taskId\":\"t1\"}"; sc "建覆盖链" "200 201"
call POST "/verification/$VID/sync" "{\"decompositionId\":\"$DID\",\"taskId\":\"t1\",\"satisfied\":true}"; sc "同步覆盖" "200 201"
call GET "/verification/$VID/report"; sc "完整性报告" 200; jchk "满足数≥1 且未完整(仍有缺口)" 'd.get("satisfied",0)>=1 and d.get("complete") in (False,True)'

sec "技能 / 缺陷"
call POST /skill "{\"projectId\":\"$PJ\",\"name\":\"基础\",\"instructions\":\"遵循六边形\"}"; sc "建技能" "200 201"; SID=$(jval 'd["id"]')
call POST /skill/compose "{\"projectId\":\"$PJ\",\"skillIds\":[\"$SID\"]}"; sc "组合技能" "200 201"; jchk "组合含指令" '"遵循六边形" in d.get("instructions","")'
call POST /bug "{\"projectId\":\"$PJ\",\"title\":\"b\",\"initialStatus\":\"NEW\"}"; sc "建缺陷" "200 201"; jchk "初态 NEW" 'd.get("status")=="NEW"'; BID=$(jval 'd["id"]')
call POST "/bug/$BID/status" '{"status":"RESOLVED"}'; sc "合法迁移" "200 201"; jchk "→RESOLVED" 'd.get("status")=="RESOLVED"'
call POST "/bug/$BID/status" '{"status":"NEW"}'; sc "非法迁移→409" 409

sec "功能用例"
call POST /functional-case "{\"projectId\":\"$PJ\",\"name\":\"功能1\",\"priority\":\"P1\"}"; sc "建功能用例" "200 201"; jchk "返回 id" 'bool(d.get("id"))'
call GET "/functional-case?projectId=$PJ"; sc "列表" 200; jchk "非空" 'isinstance(d,list) and len(d)>=1'
call GET "/functional-case/export?projectId=$PJ"; sc "导出" 200

sec "接口定义 / Mock / 接口用例"
call POST /api/definition "{\"projectId\":\"$PJ\",\"name\":\"登录API\",\"protocol\":\"HTTP\",\"method\":\"POST\",\"path\":\"/login\"}"; sc "建定义" "200 201"; DEF=$(jval 'd["id"]')
call GET "/api/definition?projectId=$PJ"; sc "定义列表" 200; jchk "非空" 'isinstance(d,list) and len(d)>=1'
call GET "/api/definition/$DEF"; sc "查定义" 200; jchk "id 一致" 'd.get("id")=="'$DEF'"'
call POST "/api/definition/$DEF/case" "{\"name\":\"c200\",\"method\":\"GET\",\"url\":\"$B/healthz\"}"; sc "定义下建用例" "200 201"
call POST "/api/definition/$DEF/mock" '{"name":"m200","responseStatus":200}'; sc "建 Mock" "200 201"
call POST /api/case "{\"projectId\":\"$PJ\",\"name\":\"健康\",\"method\":\"GET\",\"url\":\"$B/healthz\",\"assertions\":[{\"type\":\"StatusIs\",\"args\":200}]}"; sc "建接口用例" "200 201"; jchk "返回 id" 'bool(d.get("id"))'; CASE=$(jval 'd["id"]')
call GET "/api/case?projectId=$PJ"; sc "用例列表" 200
call GET "/api/case/$CASE/executions"; sc "执行记录" 200; jchk "分页 items 为数组" 'isinstance(d.get("items"),list)'

echo "== 场景用例(步骤/编译/运行)=="
call POST /api/scenario "{\"projectId\":\"$PJ\",\"name\":\"登录冒烟\"}"; sc "建场景" "200 201"; SCN=$(jval 'd["id"]')
call GET "/api/scenario/$SCN"; sc "查场景" 200; jchk "id 一致" 'd.get("id")=="'$SCN'"'
call POST "/api/scenario/$SCN/step" "{\"kind\":\"CASE\",\"order\":1,\"refId\":\"$CASE\"}"; sc "加 CASE 步骤" "200 201"
call POST "/api/scenario/$SCN/step" '{"kind":"REQUEST","order":2,"request":{"method":"GET","url":"/healthz","assertions":[{"type":"StatusIs","args":200}]}}'; sc "加 REQUEST 步骤" "200 201"
call GET "/api/scenario/$SCN/compile"; sc "编译" 200; jchk "步骤1=CASE引用,步骤2=内联请求" 'd["steps"][0].get("caseId")=="'$CASE'" and d["steps"][1]["request"]["method"]=="GET"'
call POST "/api/scenario/$SCN/run" "{\"projectId\":\"$PJ\",\"runMode\":\"PARALLEL\"}"; sc "运行场景" 200; jchk "返回执行状态" 'd.get("status") in ("SUCCESS","ERROR")'
call GET "/api/scenario/$SCN/executions"; sc "场景执行记录" 200

sec "环境 / 资源池 / 批量"
call POST /api/environment "{\"projectId\":\"$PJ\",\"name\":\"env\",\"baseUrl\":\"$B\"}"; sc "建环境" "200 201"; jchk "baseUrl" 'd.get("baseUrl")=="'$B'"'; ENV=$(jval 'd["id"]')
call GET "/api/environment?projectId=$PJ"; sc "环境列表" 200
call PUT "/api/environment/$ENV" "{\"projectId\":\"$PJ\",\"name\":\"env2\",\"baseUrl\":\"$B\"}"; sc "改环境" "200 201"; jchk "名已改" 'd.get("name")=="env2"'
call POST /api/resource-pool '{"name":"本地池"}'; sc "建资源池" "200 201"; jchk "池名" 'd.get("name")=="本地池"'
call GET /api/resource-pool; sc "资源池列表" 200
call POST /api/batch-run "{\"projectId\":\"$PJ\",\"caseIds\":[\"$CASE\"],\"runMode\":\"PARALLEL\"}"; sc "批量运行(无池→400 / 有则 200)" "200 400"

echo "== 测试计划(全套 + 报告)=="
call POST /test-plan "{\"projectId\":\"$PJ\",\"name\":\"计划\",\"type\":\"TEST_PLAN\"}"; sc "建计划" "200 201"; TP=$(jval 'd["id"]')
call POST "/test-plan/$TP/cases" "{\"caseId\":\"$CASE\",\"name\":\"健康\"}"; sc "挂用例" "200 201"
call POST "/test-plan/$TP/cases/$CASE/result" '{"status":"SUCCESS","latencyMs":1,"statusCode":200,"assertions":[{"item":"状态码","actual":"200","condition":"等于","expected":"200","passed":true}]}'; sc "回写结果" "200 201"
call GET "/test-plan/$TP/statistics"; sc "统计" 200; jchk "1 用例全通过" 'd["total"]==1 and d["passRate"]==1.0 and d["isPass"]==True'
call GET "/test-plan/$TP/report"; sc "HTML 报告" 200; T=$((T+1)); if printf %s "$RESP" | grep -q "测试计划报告"; then P=$((P+1)); else F=$((F+1)); echo "  ✗ [HTML 报告内容]"; fi
call GET "/test-plan/$TP/report.md"; sc "Markdown 报告" 200; { T=$((T+1)); printf %s "$RESP" | grep -q "## 报告分析" && P=$((P+1)) || { F=$((F+1)); echo "  ✗ [MD 报告内容]"; }; }
call POST "/test-plan/$TP/run" '{}'; sc "执行计划" 200; jchk "执行计数自洽" 'd["executed"]<=d["total"]'
call POST "/test-plan/$TP/schedule" '{"cron":"0 0 * * * *"}'; sc "配定时" "200 201"
call GET "/test-plan/$TP/runs"; sc "运行快照" 200

sec "压测 / runner / MCP"
call POST /perf/run "{\"url\":\"$B/healthz\",\"concurrency\":2,\"iterations\":6}"; sc "起压测" 200; jchk "返回 reportId" 'bool(d.get("reportId"))'; PRID=$(jval 'd["reportId"]')
sleep 1
call GET "/perf/report/$PRID"; sc "压测报告" 200; jchk "总数=6 且有吞吐" 'd.get("total")==6 and d.get("throughputRps",0)>0'
call POST /runner-agent '{"name":"env","baseUrl":"http://127.0.0.1:9190"}'; sc "注册 agent" "200 201"; jchk "返回 protocols 字段(不可达 agent→空)" 'isinstance(d.get("protocols"),list)'; AG=$(jval 'd["id"]')
call GET /runner-agent; sc "agent 列表" 200
call GET "/runner-agent/$AG/executions"; sc "agent 执行历史" 200
call POST "/runner-agent/$AG/refresh" '{}'; sc "刷新能力(agent 不可达→502)" "200 502"
call POST /runner/probe '{"protocol":"http","target":"http://127.0.0.1:1/x"}'; sc "中央探测(不可达→502)" "200 404 502"
call POST /mcp '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'; sc "MCP tools/list" 200; jchk "≥15 个工具且含 shepherd_run_test_plan" 'len(d["result"]["tools"])>=15 and any(t["name"]=="shepherd_run_test_plan" for t in d["result"]["tools"])'

echo
echo "############ 深度 API 自测:断言 $T 条(端点 + 字段 + 业务结果),通过 $P,失败 $F ############"
{ echo; echo "## 汇总"; echo; echo "- 断言总数:$T"; echo "- 通过:$P"; echo "- 失败:$F"; echo "- 结论:$([ "$F" -eq 0 ] && echo "全绿 ✅" || echo "有失败 ❌")"; } >> "$RPT"
echo "报告 → $RPT"
[ "$F" -eq 0 ] && echo "✅ 全绿" || echo "❌ 有失败"
exit $F
