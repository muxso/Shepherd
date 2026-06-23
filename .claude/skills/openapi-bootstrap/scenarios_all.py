#!/usr/bin/env python3
"""Shepherd 全业务模块 场景自举:为每个业务模块(OpenAPI tag)建一条真实链路场景并执行。

在 selftest.py(单链路)的基础上扩展为**逐模块**覆盖:每个模块一条场景,引用真实接口用例
(kind=CASE,带鉴权头 / 变量提取 / 断言),按 CRUD/生命周期顺序串起来,带环境进程内执行。
失败策略 CONTINUE → 即便某步失败也跑完全部步,一次跑出所有模块的逐步通过/失败 + 原因。

幂等:用例/场景按名「建或复用」,可重复运行不堆积。
退出码:0 全绿;1 有步骤失败;2 流程本身报错。
"""
import json
import os
import sys
import urllib.request
import urllib.error

BASE = os.environ.get("SHEPHERD_BASE", "http://127.0.0.1:9180")
USER = os.environ.get("SHEPHERD_USER", "admin")
PASS = os.environ.get("SHEPHERD_PASS", "s3cret")


def call(method, path, token=None, body=None, timeout=60):
    url = path if path.startswith("http") else BASE + path
    data = None
    headers = {}
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = "Bearer " + token
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()


def j(method, path, token=None, body=None):
    st, raw = call(method, path, token, body)
    if st >= 400:
        raise RuntimeError(f"{method} {path} -> {st}: {raw[:200]}")
    return json.loads(raw) if raw else None


def find(items, name, key="name"):
    for it in items or []:
        if it.get(key) == name:
            return it
    return None


def S(code):       # StatusIs 断言(精确状态码,用于负向 401 等)
    return {"type": "StatusIs", "args": code}


def OK():          # 通用「2xx/3xx 成功」断言:状态码 < 400(覆盖 200/201/204)
    return {"type": "ResponseCode", "args": {"condition": "LT", "expected": "400"}}


def C(sub):        # BodyContains 断言
    return {"type": "BodyContains", "args": sub}


def EX(*pairs):    # Extract 处理器:(var, jsonpath) ...
    return [{"type": "Extract", "args": {"extractors": [
        {"variable": v, "kind": "JSON_PATH", "expression": e, "scope": "TEMP"} for v, e in pairs]}}]


BEARER = [{"key": "Authorization", "value": "Bearer ${token}"}]
JSONH = [{"key": "Content-Type", "value": "application/json"}]


def step(name, method, url, asserts, body=None, headers=None, processors=None, auth=True):
    """一步 = 一条用例规格。auth=True 自动带 Bearer ${token}。"""
    h = list(headers or [])
    if auth:
        h = BEARER + h
    if body is not None and not any(x["key"].lower() == "content-type" for x in h):
        h = h + JSONH
    return {
        "name": name, "method": method, "url": url, "assertions": asserts,
        "headers": h, "body": json.dumps(body) if isinstance(body, (dict, list)) else body,
        "processors": processors or [],
    }


def build_modules(pid, oid, admin_uid, sample_case_id, tok):
    """返回 [(key, 标题, [step,...]), ...]。每条链首步登录提取 ${token}。

    pid/oid/admin_uid 为运行时已知常量(直接烤进 body/url);${var} 仅用于链内提取的新建 id。
    tok 为本次运行的唯一短串:无删除端点的持久资源(项目/用户/需求)名字带上它,避免重复运行撞唯一约束。
    """
    login = step("登录·提取token", "POST", "/auth/login",
                 [OK(), C("token")], body={"username": USER, "password": PASS},
                 processors=EX(("token", "$.token")), auth=False)

    M = []

    # ---- organization:CRUD 全生命周期 ----
    M.append(("organization", "组织 CRUD", [login,
        step("创建组织", "POST", "/organization", [OK(), C("id")],
             body={"name": "自举-org", "enable": True}, processors=EX(("orgId2", "$.id"))),
        step("查组织", "GET", "/organization/${orgId2}", [OK(), C("自举-org")]),
        step("改组织", "PUT", "/organization/${orgId2}", [OK()],
             body={"name": "自举-org-改", "enable": True}),
        step("列组织", "GET", "/organization?pageSize=5", [OK(), C("items")]),
        step("删组织", "DELETE", "/organization/${orgId2}", [OK()]),
    ]))

    # ---- project ----
    M.append(("project", "项目 创建+列", [login,
        step("创建项目", "POST", "/project", [OK(), C("id")],
             body={"name": f"自举-proj-{tok}", "organizationId": oid}, processors=EX(("projId2", "$.id"))),
        step("列项目", "GET", f"/project?organizationId={oid}&pageSize=5", [OK(), C("total")]),
    ]))

    # ---- environment ----
    M.append(("environment", "环境 CRUD", [login,
        step("创建环境", "POST", "/api/environment", [OK(), C("id")],
             body={"projectId": pid, "name": "自举-env", "baseUrl": BASE, "headers": [], "variables": {}, "enabled": True},
             processors=EX(("envId2", "$.id"))),
        step("查环境", "GET", "/api/environment/${envId2}", [OK()]),
        step("改环境", "PUT", "/api/environment/${envId2}", [OK()],
             body={"projectId": pid, "name": "自举-env-改", "baseUrl": BASE, "headers": [], "variables": {}, "enabled": True}),
        step("列环境", "GET", f"/api/environment?projectId={pid}", [OK()]),
        step("删环境", "DELETE", "/api/environment/${envId2}", [OK()]),
    ]))

    # ---- api-definition (+case) ----
    M.append(("api-definition", "接口定义 CRUD+用例", [login,
        step("创建定义", "POST", "/api/definition", [OK(), C("id")],
             body={"projectId": pid, "name": "自举-def", "method": "GET", "path": "/healthz", "protocol": "HTTP"},
             processors=EX(("defId2", "$.id"))),
        step("查定义", "GET", "/api/definition/${defId2}", [OK()]),
        step("建定义用例", "POST", "/api/definition/${defId2}/case", [OK(), C("id")],
             body={"name": "自举-def-case", "method": "GET", "url": "/healthz"}),
        step("列定义用例", "GET", "/api/definition/${defId2}/case", [OK()]),
        step("改定义状态", "PUT", "/api/definition/${defId2}/status", [OK()], body={"status": "DEBUGGING"}),
        step("查引用", "GET", "/api/definition/${defId2}/references", [OK()]),
        step("删定义", "DELETE", "/api/definition/${defId2}", [OK()]),
    ]))

    # ---- api-scenario ----
    M.append(("api-scenario", "场景 CRUD+编译+执行", [login,
        step("创建场景", "POST", "/api/scenario", [OK(), C("id")],
             body={"projectId": pid, "name": "自举-子场景"}, processors=EX(("scId2", "$.id"))),
        step("加请求步骤", "POST", "/api/scenario/${scId2}/step", [OK()],
             body={"kind": "REQUEST", "order": 0, "request": {"method": "GET", "url": "/healthz"}}),
        step("查场景", "GET", "/api/scenario/${scId2}", [OK(), C("steps")]),
        step("编译场景", "GET", "/api/scenario/${scId2}/compile", [OK()]),
        step("删场景", "DELETE", "/api/scenario/${scId2}", [OK()]),
    ]))

    # ---- api-test (resource-pool) ----
    M.append(("api-test", "资源池 CRUD", [login,
        step("列资源池", "GET", "/api/resource-pool", [OK()]),
        step("创建资源池", "POST", "/api/resource-pool", [OK(), C("id")],
             body={"name": "自举-pool", "poolType": "Node", "maxConcurrency": 5, "enabled": True,
                   "allOrg": True, "orgIds": [], "serverUrl": "", "description": "自举"},
             processors=EX(("poolId2", "$.id"))),
        step("查资源池", "GET", "/api/resource-pool/${poolId2}", [OK()]),
        step("删资源池", "DELETE", "/api/resource-pool/${poolId2}", [OK()]),
    ]))

    # ---- requirement(含版本/基线;见记忆:baseline body {version:N}) ----
    M.append(("requirement", "需求 生命周期", [login,
        step("创建需求", "POST", "/requirement", [OK(), C("id")],
             body={"projectId": pid, "title": f"自举-需求-{tok}", "description": "desc", "acceptanceCriteria": ["AC1"]},
             processors=EX(("reqId2", "$.id"))),
        step("查需求", "GET", "/requirement/${reqId2}", [OK()]),
        step("改需求", "PUT", "/requirement/${reqId2}", [OK()], body={"title": f"自举-需求-{tok}-改"}),
        step("新增版本", "POST", "/requirement/${reqId2}/version", [OK()],
             body={"description": "v2", "acceptanceCriteria": ["AC1", "AC2"]}),
        step("基线化", "PUT", "/requirement/${reqId2}/baseline", [OK()], body={"version": 1}),
        step("查版本1", "GET", "/requirement/${reqId2}/version/1", [OK()]),
        step("列需求", "GET", f"/requirement?projectId={pid}", [OK()]),
    ]))

    # ---- test-plan ----
    M.append(("test-plan", "测试计划 生命周期", [login,
        step("创建计划", "POST", "/test-plan", [OK(), C("id")],
             body={"projectId": pid, "name": "自举-计划", "type": "TEST_PLAN"},
             processors=EX(("planId2", "$.id"))),
        step("关联用例", "POST", "/test-plan/${planId2}/cases", [OK()],
             body={"caseId": sample_case_id, "name": "自举-计划用例"}),
        step("列计划用例", "GET", "/test-plan/${planId2}/cases", [OK()]),
        step("计划统计", "GET", "/test-plan/${planId2}/statistics", [OK()]),
        step("计划报告", "GET", "/test-plan/${planId2}/report", [OK()]),
    ]))

    # ---- role / user-role ----
    M.append(("role", "角色 CRUD", [login,
        step("创建角色", "POST", "/role", [OK(), C("id")],
             body={"name": "自举-role", "permissions": []}, processors=EX(("roleId2", "$.id"))),
        step("查角色", "GET", "/role/${roleId2}", [OK()]),
        step("改角色", "PUT", "/role/${roleId2}", [OK()], body={"name": "自举-role-改", "permissions": []}),
        step("列角色", "GET", "/role", [OK()]),
        step("删角色", "DELETE", "/role/${roleId2}", [OK()]),
    ]))

    # ---- user ----
    M.append(("user", "用户 CRUD", [login,
        step("创建用户", "POST", "/system/user", [OK(), C("id")],
             body={"name": f"自举-用户-{tok}", "email": f"bootstrap-{tok}@example.com"}, processors=EX(("userId2", "$.id"))),
        step("查用户", "GET", "/system/user/${userId2}", [OK()]),
        step("改用户", "PUT", "/system/user/${userId2}", [OK()],
             body={"name": f"自举-用户-{tok}-改", "email": f"bootstrap-{tok}@example.com", "enable": True}),
        step("列用户", "GET", "/system/user", [OK()]),
        step("用户名表", "GET", "/system/user/names", [OK()]),
        step("删用户", "DELETE", "/system/user/${userId2}", [OK()]),
    ]))

    # ---- skill ----
    M.append(("skill", "技能 CRUD+合成", [login,
        step("创建技能", "POST", "/skill", [OK(), C("id")],
             body={"projectId": pid, "name": "自举-skill", "instructions": "do x", "description": "d", "includes": []},
             processors=EX(("skillId2", "$.id"))),
        step("查技能", "GET", "/skill/${skillId2}", [OK()]),
        step("改技能", "PUT", "/skill/${skillId2}", [OK()],
             body={"name": "自举-skill-改", "instructions": "do y", "description": "d", "includes": [], "enabled": True}),
        step("列技能", "GET", f"/skill?projectId={pid}", [OK()]),
        step("合成技能", "POST", "/skill/compose", [OK()],
             body={"projectId": pid, "skillIds": ["${skillId2}"]}),
        step("删技能", "DELETE", "/skill/${skillId2}", [OK()]),
    ]))

    # ---- functional-case ----
    M.append(("functional-case", "功能用例 创建+列+导出", [login,
        step("创建功能用例", "POST", "/functional-case", [OK(), C("id")],
             body={"projectId": pid, "name": f"自举-功能用例-{tok}", "priority": "P1", "status": "PREPARED",
                   "module": "", "steps": [], "customFields": {}}),
        step("列功能用例", "GET", f"/functional-case?projectId={pid}", [OK()]),
        step("导出功能用例", "GET", f"/functional-case/export?projectId={pid}", [OK()]),
    ]))

    # ---- bug ----
    M.append(("bug", "缺陷 创建+流转", [login,
        step("创建缺陷", "POST", "/bug", [OK(), C("id")],
             body={"projectId": pid, "title": f"自举-缺陷-{tok}", "initialStatus": "NEW"},
             processors=EX(("bugId2", "$.id"))),
        step("流转缺陷状态", "POST", "/bug/${bugId2}/status", [OK()], body={"status": "RESOLVED"}),
    ]))

    # ---- case-review ----
    M.append(("case", "用例评审", [login,
        step("发起评审", "POST", "/case-review", [OK(), C("id")],
             body={"projectId": pid, "caseIds": [sample_case_id], "passRule": "SINGLE", "reviewerCount": 1},
             processors=EX(("reviewId2", "$.id"))),
        step("查评审", "GET", "/case-review/${reviewId2}", [OK()]),
        step("提交评审意见", "POST", f"/case-review/${{reviewId2}}/{sample_case_id}", [OK()],
             body={"reviewerId": admin_uid, "status": "PASS", "content": "ok"}),
        step("列评审", "GET", f"/case-review?projectId={pid}", [OK()]),
    ]))

    # ---- task / decomposition(依赖需求) ----
    M.append(("task", "任务拆分", [login,
        step("建需求(种子)", "POST", "/requirement", [OK(), C("id")],
             body={"projectId": pid, "title": f"自举-拆分需求-{tok}", "description": "d", "acceptanceCriteria": ["AC1"]},
             processors=EX(("reqForTask", "$.id"))),
        step("基线v1", "PUT", "/requirement/${reqForTask}/baseline", [OK()], body={"version": 1}),
        step("创建拆分", "POST", "/decomposition", [OK(), C("id")],
             body={"requirementId": "${reqForTask}", "requirementVersion": 1}, processors=EX(("decId2", "$.id"))),
        step("查拆分", "GET", "/decomposition/${decId2}", [OK()]),
        step("加任务", "POST", "/decomposition/${decId2}/task", [OK(), C("taskId")],
             body={"title": "自举-任务", "description": "d", "acceptanceCriteria": [], "dependencies": [], "points": 3},
             processors=EX(("taskId2", "$.taskId"))),
        step("改任务点数", "POST", "/decomposition/${decId2}/task/${taskId2}/points", [OK()], body={"points": 5}),
        step("改任务状态", "POST", "/decomposition/${decId2}/task/${taskId2}/status", [OK()], body={"status": "DISPATCHED"}),
        step("拆分就绪", "GET", "/decomposition/${decId2}/ready", [OK()]),
    ]))

    # ---- delivery(依赖拆分+任务) ----
    M.append(("delivery", "交付 生命周期", [login,
        step("建需求(种子)", "POST", "/requirement", [OK(), C("id")],
             body={"projectId": pid, "title": f"自举-交付需求-{tok}", "description": "d", "acceptanceCriteria": ["AC1"]},
             processors=EX(("reqForDel", "$.id"))),
        step("基线v1", "PUT", "/requirement/${reqForDel}/baseline", [OK()], body={"version": 1}),
        step("创建拆分", "POST", "/decomposition", [OK(), C("id")],
             body={"requirementId": "${reqForDel}", "requirementVersion": 1}, processors=EX(("decForDel", "$.id"))),
        step("加任务", "POST", "/decomposition/${decForDel}/task", [OK(), C("taskId")],
             body={"title": "自举-交付任务", "description": "d", "acceptanceCriteria": [], "dependencies": [], "points": 3},
             processors=EX(("taskForDel", "$.taskId"))),
        # CLAUDE_CODE 是**同步 stub 执行器**:POST /delivery 一步到位,创建即 DELIVERED。
        # 故无 running/complete(那是异步执行器的转移,本地无在线 agent 跑不到);断言落 DELIVERED 即印证同步交付。
        step("创建交付(同步→DELIVERED)", "POST", "/delivery", [OK(), C("DELIVERED")],
             body={"decompositionId": "${decForDel}", "taskId": "${taskForDel}", "executor": "CLAUDE_CODE",
                   "title": "自举-交付", "description": "d", "acceptanceCriteria": []},
             processors=EX(("delId2", "$.id"))),
        step("查交付", "GET", "/delivery/${delId2}", [OK(), C("DELIVERED")]),
        step("加交付事件", "POST", "/delivery/${delId2}/events", [OK()], body={"kind": "LOG", "message": "hello"}),
        step("列交付事件", "GET", "/delivery/${delId2}/events", [OK()]),
    ]))

    # ---- verification(依赖需求 + 验收标准索引) ----
    M.append(("verification", "验证 创建+链接+报告", [login,
        step("建需求(种子)", "POST", "/requirement", [OK(), C("id")],
             body={"projectId": pid, "title": f"自举-验证需求-{tok}", "description": "d", "acceptanceCriteria": ["AC1"]},
             processors=EX(("reqForVer", "$.id"))),
        step("基线v1", "PUT", "/requirement/${reqForVer}/baseline", [OK()], body={"version": 1}),
        step("创建验证", "POST", "/verification", [OK(), C("id")],
             body={"requirementId": "${reqForVer}", "requirementVersion": 1, "criteria": ["AC1"]},
             processors=EX(("verId2", "$.id"))),
        step("查验证", "GET", "/verification/${verId2}", [OK()]),
        step("验证报告", "GET", "/verification/${verId2}/report", [OK()]),
    ]))

    # ---- runner ----
    # 注:/runner/probe 与 /runner-agent/{id}/run 需要**在线 runner agent**(派发失败回 502),
    # 本地无 agent 环境跑不通,故 runner 链只覆盖注册/列表/记录这类无需在线 agent 的管理面。
    M.append(("runner", "执行器 注册+列表", [login,
        step("列执行器", "GET", "/runner-agent", [OK()]),
        step("注册执行器", "POST", "/runner-agent", [OK(), C("id")],
             body={"name": f"自举-agent-{tok}", "baseUrl": BASE, "enabled": True}, processors=EX(("agentId2", "$.id"))),
        step("执行器记录", "GET", "/runner-agent/${agentId2}/executions", [OK()]),
    ]))

    # ---- perf(引用自举场景做场景压测) ----
    M.append(("perf", "性能压测 单接口+报告", [login,
        step("发起单接口压测", "POST", "/perf/run", [OK(), C("reportId")],
             body={"projectId": pid, "method": "GET", "url": BASE + "/healthz", "concurrency": 2, "iterations": 5},
             processors=EX(("perfRep", "$.reportId"))),
        step("查压测报告", "GET", "/perf/report/${perfRep}", [OK()]),
    ]))

    # ---- auth ----
    M.append(("auth", "登录/登出/负向", [login,
        step("登出", "POST", "/auth/logout", [OK()]),
        step("负向·错密码", "POST", "/auth/login", [S(401)],
             body={"username": USER, "password": "wrong-pass"}, auth=False),
    ]))

    return M


def main():
    print(f"== Shepherd 全模块场景自举 @ {BASE} ==\n")
    token = j("POST", "/auth/login", body={"username": USER, "password": PASS})["token"]

    # 解析 org/project/admin uid
    pid = os.environ.get("SHEPHERD_PROJECT_ID")
    orgs = j("GET", "/organization?pageSize=100", token)["items"]
    if not orgs:
        orgs = [j("POST", "/organization", token, {"name": "bootstrap-org"})]
    oid = orgs[0]["id"]
    if not pid:
        projs = j("GET", f"/project?organizationId={oid}&pageSize=100", token).get("items", [])
        if not projs:
            projs = [j("POST", "/project", token, {"organizationId": oid, "name": "bootstrap"})]
        pid = projs[0]["id"]
    users = j("GET", "/system/user?pageSize=5", token)
    ulist = users.get("items", users) if isinstance(users, dict) else users
    admin_uid = (ulist[0]["id"] if ulist else "admin")
    print(f"项目 projectId={pid}  组织 orgId={oid}  adminUid={admin_uid}")

    # 环境(baseUrl 指向本机,供相对 url 拼接)— 建或复用
    envs = j("GET", f"/api/environment?projectId={pid}", token)
    env = find(envs, "自举环境")
    if not env:
        env = j("POST", "/api/environment", token,
                {"projectId": pid, "name": "自举环境", "baseUrl": BASE, "headers": [], "variables": {}})
    eid = env["id"]

    # holder 定义(承载所有模块用例,以便用例带鉴权头)。
    # 无「改用例」端点 → 每次删旧 holder(级联删其用例)再重建,确保用例规格随脚本更新。
    defs = j("GET", f"/api/definition?projectId={pid}", token)
    old = find(defs, "自举-全模块")
    if old:
        j("DELETE", f"/api/definition/{old['id']}", token)
    holder = j("POST", "/api/definition", token,
               {"projectId": pid, "name": "自举-全模块", "method": "POST", "path": "/auth/login"})
    did = holder["id"]
    existing = {}

    # 给 test-plan/case-review 用的真实样例用例 id:取/建一个普通用例
    sample_name = "自举-样例用例"
    sample = existing.get(sample_name)
    if not sample:
        sample = j("POST", f"/api/definition/{did}/case", token,
                   {"name": sample_name, "method": "GET", "url": "/healthz", "assertions": [OK()]})
        existing[sample_name] = sample
    sample_case_id = sample["id"]

    def ensure_case(prefix, sp):
        name = f"{prefix}·{sp['name']}"
        hit = existing.get(name)
        if hit:
            return hit
        created = j("POST", f"/api/definition/{did}/case", token, {
            "name": name, "method": sp["method"], "url": sp["url"], "assertions": sp["assertions"],
            "headers": sp["headers"], "body": sp["body"], "processors": sp["processors"],
        })
        existing[name] = created
        return created

    modules = build_modules(pid, oid, admin_uid, sample_case_id, did[:8])
    scns = {s["name"]: s for s in j("GET", f"/api/scenario?projectId={pid}", token)}

    print(f"\n模块数 {len(modules)};逐模块建场景并执行(失败策略 CONTINUE)\n")
    grand_pass = grand_total = 0
    rows = []
    for key, title, steps in modules:
        # login 步骤跨模块复用同一条用例
        cases = []
        for sp in steps:
            prefix = "公共" if sp["name"].startswith("登录") else key
            cases.append(ensure_case(prefix, sp))
        sname = f"自举M·{key}"
        scn = scns.get(sname)
        if not scn:
            scn = j("POST", "/api/scenario", token, {"projectId": pid, "name": sname})
            scns[sname] = scn
        sid = scn["id"]
        want = [c["id"] for c in cases]
        cur = j("GET", f"/api/scenario/{sid}", token).get("steps") or []
        # 注:场景步骤响应里 CASE 引用字段是 caseId(非 refId);用它对齐才幂等(否则每次都重建)。
        if [s.get("caseId") for s in cur] != want:
            for s in cur:
                j("DELETE", f"/api/scenario/{sid}/step/{s['id']}", token)
            for i, c in enumerate(cases):
                j("POST", f"/api/scenario/{sid}/step", token, {"kind": "CASE", "order": i, "refId": c["id"]})
        run = j("POST", f"/api/scenario/{sid}/run", token,
                {"projectId": pid, "environmentId": eid, "failureStrategy": "CONTINUE"})
        rpt = j("GET", f"/api/scenario-report/{run['reportId']}", token)
        id2name = {c["id"]: c["name"] for c in cases}
        npass = sum(1 for r in rpt["results"] if r["outcome"] == "SUCCESS")
        ntot = len(rpt["results"])
        grand_pass += npass
        grand_total += ntot
        mark = "✅" if npass == ntot else ("⚠️" if npass else "❌")
        rows.append((mark, key, title, npass, ntot))
        print(f"{mark} [{key}] {title}: {npass}/{ntot}")
        for r in rpt["results"]:
            if r["outcome"] != "SUCCESS":
                nm = id2name.get(r["caseId"], r["caseId"][:30])
                reason = (r["failures"][0] if r.get("failures") else "")[:120]
                print(f"      ❌ [{r.get('statusCode')}] {nm}  ↳ {reason}")

    print("\n== 全模块汇总 ==")
    for mark, key, title, npass, ntot in rows:
        print(f"  {mark} {key:16} {npass}/{ntot}  {title}")
    nfull = sum(1 for r in rows if r[3] == r[4])
    print(f"\n模块全绿 {nfull}/{len(rows)};步骤总通过 {grand_pass}/{grand_total}")
    return 0 if grand_pass == grand_total else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:
        print(f"\n!! 自举失败: {e}", file=sys.stderr)
        sys.exit(2)
