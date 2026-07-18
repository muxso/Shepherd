#!/usr/bin/env python3
"""Shepherd OpenAPI self-bootstrap (dogfooding): test Shepherd's HTTP API with Shepherd itself.

Differences from the early version (fixed after feedback):
  - No longer runs hardcoded inline GETs; builds **real api cases** referenced
    by the scenario (kind=CASE).
  - Covers POST + GET + auth headers + variable extraction + cross-step
    chaining (login -> extract token -> authed call with token -> extract
    orgId -> chained call).
  - Cases execute **in-process** (no resource pool); assertions include status
    codes + business checks; a negative case verifies unauthorized rejection.

Flow:
  1. Login for a token (admin calls for resource setup only)
  2. Resolve/create organization -> project
  3. Fetch this system's OpenAPI -> idempotent import as API definitions
     (same method+path overwrites, no duplicates); check spec/case quality
  4. Create-or-reuse an environment pointing at this host (baseUrl = this server)
  5. Create-or-reuse 4 real cases under the bootstrap-chain definition
     (extraction/auth/negative included)
  6. Create-or-reuse a scenario **referencing** those cases as steps
     (kind=CASE), execute with the environment
  7. Fetch the report; print per-step pass/fail + extracted variables +
     assertion counts

Note: persisted resource names (自举环境 / 自举链路 / 自举链路场景 and the case
names) are reuse-by-name keys — renaming them would orphan existing rows.
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
    print(f"== Shepherd OpenAPI self-bootstrap @ {BASE} ==\n")

    # 1) Login
    token = j("POST", "/auth/login", body={"username": USER, "password": PASS})["token"]
    print(f"[1/7] logged in, token={token[:8]}…")

    # 2) Organization -> project
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
    print(f"[2/7] project projectId={pid}")

    # 3) Fetch OpenAPI -> idempotent import
    spec_doc = json.loads(call("GET", "/api-docs/openapi.json")[1])
    imported = j("POST", "/api/definition/import", token,
                 {"projectId": pid, "content": spec_doc})
    print(f"[3/7] OpenAPI {spec_doc['info']['title']} v{spec_doc['info']['version']} "
          f"paths={len(spec_doc.get('paths', {}))} -> created {len(imported['created'])} "
          f"updated {imported.get('updated', 0)} skipped {imported['skipped']}")

    # 3b) Import quality sample: spec populated + assertion-bearing case present
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
    print(f"      import quality (sample {len(sample)}): with spec {with_spec}/{len(sample)}, "
          f"with assertion case {with_case}/{len(sample)}")

    # 4) Environment (baseUrl points at this host) — create or reuse
    envs = j("GET", f"/api/environment?projectId={pid}", token)
    env = find(envs, "自举环境")
    if not env:
        env = j("POST", "/api/environment", token,
                {"projectId": pid, "name": "自举环境", "baseUrl": BASE, "headers": [], "variables": {}})
    eid = env["id"]
    print(f"[4/7] environment 自举环境 baseUrl={env['baseUrl']}")

    # 5) Real chained cases (create or reuse, under the 自举链路 holder definition)
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
    print(f"[5/7] chain cases: {len(cases)} "
          "(login -> extract token -> authed list orgs -> extract orgId -> authed list projects; plus negative 401)")

    # 6) Scenario referencing the cases (kind=CASE) — create or reuse + run with env
    scns = j("GET", f"/api/scenario?projectId={pid}", token)
    scn = find(scns, "自举链路场景")
    if not scn:
        scn = j("POST", "/api/scenario", token, {"projectId": pid, "name": "自举链路场景"})
    sid = scn["id"]
    # Align steps: if the current steps do not reference exactly these cases,
    # clear and rebuild in order (idempotent; avoids stale steps).
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
    print(f"[6/7] scenario run (case refs, with env) status={run['status']} caseCount={run['caseCount']}")

    # 7) Report
    rpt = j("GET", f"/api/scenario-report/{run['reportId']}", token)
    id2name = {c["id"]: c["name"] for c in cases}
    print("\n[7/7] bootstrap chain results (per step):")
    passed = 0
    for r in rpt["results"]:
        ok = r["outcome"] == "SUCCESS"
        passed += ok
        asn = r.get("assertions") or []
        npass = sum(1 for a in asn if a.get("passed"))
        name = id2name.get(r["caseId"], r["caseId"][:40])
        print(f"  {'✅' if ok else '❌'} [{r.get('statusCode')}] {name}  "
              f"assertions {npass}/{len(asn)}  ({r.get('latencyMs')}ms)")
        for f in r["failures"]:
            print(f"        ↳ {f}")
        for k, v in (r.get("extractions") or []):
            print(f"        ⇒ extracted {k} = {v}")
    total = len(rpt["results"])
    print(f"\n== bootstrap summary: {passed}/{total} steps passed, scenario status={rpt['status']} ==")
    print(f"   report: {BASE}/api/scenario-report/{run['reportId']}  (UI: scenarios -> {sid})")
    return 0 if passed == total else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:
        print(f"\n!! bootstrap failed: {e}", file=sys.stderr)
        sys.exit(2)
