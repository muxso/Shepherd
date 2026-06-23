#!/usr/bin/env python3
"""Shepherd OpenAPI 自举(dogfooding):用 Shepherd 自己测 Shepherd 自己的 HTTP API。

与早期版本的区别(针对反馈修正):
  - 不再只跑写死的 GET 内联请求;而是建**真实接口用例**并由场景**引用**(kind=CASE)。
  - 覆盖 POST + GET + 鉴权头 + 变量提取 + 跨步链路(login → 提取 token → 带 token 鉴权调用 → 提取 orgId → 再调用)。
  - 用例经资源池外的**进程内**场景执行,断言含状态码 + 业务断言;负向用例验证未授权被拒。

流程:
  1. 登录拿 token(仅用于建资源的管理调用)
  2. 解析/创建 组织 → 项目
  3. 抓本系统 OpenAPI → 幂等导入为接口定义(同 方法+路径 覆盖,不重复堆积);校验 spec/用例质量
  4. 建/复用 指向本机的环境(baseUrl = 本服务)
  5. 建/复用 一条「自举链路」定义下的 4 个真实用例(含提取/鉴权/负向)
  6. 建/复用 场景,**引用**这些用例为步骤(kind=CASE),带环境执行
  7. 拉报告,逐步打印 ✅/❌ + 提取变量 + 断言通过数
"""
import json
import os
import sys
import urllib.request
import urllib.error

BASE = os.environ.get("SHEPHERD_BASE", "http://127.0.0.1:9180")
USER = os.environ.get("SHEPHERD_USER", "admin")
PASS = os.environ.get("SHEPHERD_PASS", "s3cret")


def call(method, path, token=None, body=None, timeout=30):
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


def main():
    print(f"== Shepherd OpenAPI 自举 @ {BASE} ==\n")

    # 1) 登录
    token = j("POST", "/auth/login", body={"username": USER, "password": PASS})["token"]
    print(f"[1/7] 登录成功 token={token[:8]}…")

    # 2) 组织 → 项目
    pid = os.environ.get("SHEPHERD_PROJECT_ID")
    if not pid:
        orgs = j("GET", "/organization?pageSize=100", token)["items"]
        if not orgs:
            orgs = [j("POST", "/organization", token, {"name": "bootstrap-org"})]
        oid = orgs[0]["id"]
        projs = j("GET", f"/project?organizationId={oid}&pageSize=100", token).get("items", [])
        if not projs:
            projs = [j("POST", "/project", token, {"organizationId": oid, "name": "bootstrap"})]
        pid = projs[0]["id"]
    print(f"[2/7] 项目 projectId={pid}")

    # 3) 抓 OpenAPI → 幂等导入
    spec_doc = json.loads(call("GET", "/api-docs/openapi.json")[1])
    imported = j("POST", "/api/definition/import", token,
                 {"projectId": pid, "content": spec_doc})
    print(f"[3/7] OpenAPI {spec_doc['info']['title']} v{spec_doc['info']['version']} "
          f"paths={len(spec_doc.get('paths', {}))} → 新增 {len(imported['created'])} "
          f"覆盖 {imported.get('updated', 0)} 跳过 {imported['skipped']}")

    # 3b) 导入质量抽样:spec 是否填充 + 是否带断言用例
    sample = (imported["created"] or
              [d for d in j("GET", f"/api/definition?projectId={pid}", token)][:12])[:12]
    with_spec = with_case = 0
    for d in sample:
        full = j("GET", f"/api/definition/{d['id']}", token)
        s = full.get("spec") or {}
        with_spec += any(s.get(k) for k in
                         ("requestQuery", "requestHeaders", "restParams", "bodySchema", "responses"))
        cs = j("GET", f"/api/definition/{d['id']}/case", token)
        with_case += bool(cs) and bool(cs[0].get("assertions"))
    print(f"      导入质量(抽样 {len(sample)}):有 spec {with_spec}/{len(sample)},"
          f"带断言用例 {with_case}/{len(sample)}")

    # 4) 环境(baseUrl 指向本机)— 建或复用
    envs = j("GET", f"/api/environment?projectId={pid}", token)
    env = find(envs, "自举环境")
    if not env:
        env = j("POST", "/api/environment", token,
                {"projectId": pid, "name": "自举环境", "baseUrl": BASE, "headers": [], "variables": {}})
    eid = env["id"]
    print(f"[4/7] 环境 自举环境 baseUrl={env['baseUrl']}")

    # 5) 真实链路用例(建或复用,挂在「自举链路」定义下)
    defs = j("GET", f"/api/definition?projectId={pid}", token)
    holder = find(defs, "自举链路")
    if not holder:
        holder = j("POST", "/api/definition", token,
                   {"projectId": pid, "name": "自举链路", "method": "POST", "path": "/auth/login"})
    did = holder["id"]
    existing_cases = j("GET", f"/api/definition/{did}/case", token)

    def ensure_case(name, method, url, assertions, headers=None, body=None, processors=None):
        hit = find(existing_cases, name)
        if hit:
            return hit
        return j("POST", f"/api/definition/{did}/case", token, {
            "name": name, "method": method, "url": url, "assertions": assertions,
            "headers": headers or [], "body": body, "processors": processors or [],
        })

    bearer = [{"key": "Authorization", "value": "Bearer ${token}"}]
    cases = [
        ensure_case(
            "登录并提取token", "POST", "/auth/login",
            [{"type": "StatusIs", "args": 200}, {"type": "BodyContains", "args": "token"}],
            headers=[{"key": "Content-Type", "value": "application/json"}],
            body=json.dumps({"username": USER, "password": PASS}),
            processors=[{"type": "Extract", "args": {"extractors": [
                {"variable": "token", "kind": "JSON_PATH", "expression": "$.token", "scope": "TEMP"}]}}]),
        ensure_case(
            "鉴权列组织并提取orgId", "GET", "/organization?pageSize=5",
            [{"type": "StatusIs", "args": 200}, {"type": "BodyContains", "args": "items"}],
            headers=bearer,
            processors=[{"type": "Extract", "args": {"extractors": [
                {"variable": "orgId", "kind": "JSON_PATH", "expression": "$.items[0].id", "scope": "TEMP"}]}}]),
        ensure_case(
            "鉴权按orgId列项目", "GET", "/project?organizationId=${orgId}&pageSize=5",
            [{"type": "StatusIs", "args": 200}, {"type": "BodyContains", "args": "total"}],
            headers=bearer),
        ensure_case(
            "负向·无token访问被拒", "GET", "/organization?pageSize=5",
            [{"type": "StatusIs", "args": 401}]),
    ]
    print(f"[5/7] 链路用例 {len(cases)} 条(登录→提取token→鉴权列组织→提取orgId→鉴权列项目;含负向401)")

    # 6) 场景引用用例(kind=CASE)— 建或复用 + 带环境执行
    scns = j("GET", f"/api/scenario?projectId={pid}", token)
    scn = find(scns, "自举链路场景")
    if not scn:
        scn = j("POST", "/api/scenario", token, {"projectId": pid, "name": "自举链路场景"})
    sid = scn["id"]
    # 对齐步骤:若现有步骤未精确引用这批用例,清空后按序重建(幂等,避免历史脏步骤)。
    want = [c["id"] for c in cases]
    steps = j("GET", f"/api/scenario/{sid}", token).get("steps") or []
    if [s.get("refId") for s in steps] != want:
        for s in steps:
            j("DELETE", f"/api/scenario/{sid}/step/{s['id']}", token)
        for i, c in enumerate(cases):
            j("POST", f"/api/scenario/{sid}/step", token,
              {"kind": "CASE", "order": i, "refId": c["id"]})
    run = j("POST", f"/api/scenario/{sid}/run", token,
            {"projectId": pid, "environmentId": eid, "failureStrategy": "CONTINUE"})
    print(f"[6/7] 场景执行(引用用例,带环境)status={run['status']} caseCount={run['caseCount']}")

    # 7) 报告
    rpt = j("GET", f"/api/scenario-report/{run['reportId']}", token)
    id2name = {c["id"]: c["name"] for c in cases}
    print("\n[7/7] 自举链路结果(逐步):")
    passed = 0
    for r in rpt["results"]:
        ok = r["outcome"] == "SUCCESS"
        passed += ok
        asn = r.get("assertions") or []
        npass = sum(1 for a in asn if a.get("passed"))
        name = id2name.get(r["caseId"], r["caseId"][:40])
        print(f"  {'✅' if ok else '❌'} [{r.get('statusCode')}] {name}  "
              f"断言 {npass}/{len(asn)}  ({r.get('latencyMs')}ms)")
        for f in r["failures"]:
            print(f"        ↳ {f}")
        for k, v in (r.get("extractions") or []):
            print(f"        ⇒ 提取 {k} = {v}")
    total = len(rpt["results"])
    print(f"\n== 自举汇总:{passed}/{total} 步通过,场景状态={rpt['status']} ==")
    print(f"   报告: {BASE}/api/scenario-report/{run['reportId']}  (UI: 场景 → {sid})")
    return 0 if passed == total else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:
        print(f"\n!! 自举失败: {e}", file=sys.stderr)
        sys.exit(2)
