#!/usr/bin/env python3
"""Shepherd full-module scenario bootstrap: one real chained scenario per business module (OpenAPI tag).

Extends selftest.py (single chain) to **per-module** coverage: one scenario
per module, referencing real api cases (kind=CASE, with auth headers /
variable extraction / assertions), chained in CRUD/lifecycle order and
executed in-process with an environment. Failure strategy CONTINUE -> even if
a step fails the rest still run, so one run yields per-step pass/fail +
reasons for every module.

Idempotent: cases/scenarios are create-or-reuse by name; repeated runs do not
accumulate. Persisted lookup names (自举环境 / 自举-全模块 / 自举M·*) are kept
as-is — they are reuse-by-name keys; renaming them would orphan existing rows.
Exit codes: 0 all green; 1 some step failed; 2 the flow itself errored.
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


def S(code):       # StatusIs assertion (exact status code, e.g. negative 401)
    return {"type": "StatusIs", "args": code}


def OK():          # generic "2xx/3xx success" assertion: status < 400 (covers 200/201/204)
    return {"type": "ResponseCode", "args": {"condition": "LT", "expected": "400"}}


def C(sub):        # BodyContains assertion
    return {"type": "BodyContains", "args": sub}


def EX(*pairs):    # Extract processor: (var, jsonpath) ...
    return [{"type": "Extract", "args": {"extractors": [
        {"variable": v, "kind": "JSON_PATH", "expression": e, "scope": "TEMP"} for v, e in pairs]}}]


BEARER = [{"key": "Authorization", "value": "Bearer ${token}"}]
JSONH = [{"key": "Content-Type", "value": "application/json"}]


def step(name, method, url, asserts, body=None, headers=None, processors=None, auth=True):
    """One step = one case spec. auth=True adds Bearer ${token} automatically."""
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
    """Returns [(key, title, [step,...]), ...]. Each chain starts with login extracting ${token}.

    pid/oid/admin_uid are known constants at build time (baked into body/url);
    ${var} is only used for ids extracted within the chain.
    tok is this run's unique short string: persistent resources with no delete
    endpoint (project/user/requirement) carry it in their names, so repeated
    runs do not hit unique constraints.
    """
    login = step("login·extract token", "POST", "/auth/login",
                 [OK(), C("token")], body={"username": USER, "password": PASS},
                 processors=EX(("token", "$.token")), auth=False)

    M = []

    # ---- organization: full CRUD lifecycle ----
    M.append(("organization", "organization CRUD", [login,
        step("create org", "POST", "/organization", [OK(), C("id")],
             body={"name": "boot-org", "enable": True}, processors=EX(("orgId2", "$.id"))),
        step("get org", "GET", "/organization/${orgId2}", [OK(), C("boot-org")]),
        step("update org", "PUT", "/organization/${orgId2}", [OK()],
             body={"name": "boot-org-upd", "enable": True}),
        step("list orgs", "GET", "/organization?pageSize=5", [OK(), C("items")]),
        step("delete org", "DELETE", "/organization/${orgId2}", [OK()]),
    ]))

    # ---- project ----
    M.append(("project", "project create+list", [login,
        step("create project", "POST", "/project", [OK(), C("id")],
             body={"name": "boot-proj-${__runid}", "organizationId": oid}, processors=EX(("projId2", "$.id"))),
        step("list projects", "GET", f"/project?organizationId={oid}&pageSize=5", [OK(), C("total")]),
    ]))

    # ---- environment ----
    M.append(("environment", "environment CRUD", [login,
        step("create env", "POST", "/api/environment", [OK(), C("id")],
             body={"projectId": pid, "name": "boot-env", "baseUrl": BASE, "headers": [], "variables": {}, "enabled": True},
             processors=EX(("envId2", "$.id"))),
        step("get env", "GET", "/api/environment/${envId2}", [OK()]),
        step("update env", "PUT", "/api/environment/${envId2}", [OK()],
             body={"projectId": pid, "name": "boot-env-upd", "baseUrl": BASE, "headers": [], "variables": {}, "enabled": True}),
        step("list envs", "GET", f"/api/environment?projectId={pid}", [OK()]),
        step("delete env", "DELETE", "/api/environment/${envId2}", [OK()]),
    ]))

    # ---- api-definition (+case) ----
    M.append(("api-definition", "api definition CRUD+case", [login,
        step("create definition", "POST", "/api/definition", [OK(), C("id")],
             body={"projectId": pid, "name": "boot-def", "method": "GET", "path": "/healthz", "protocol": "HTTP"},
             processors=EX(("defId2", "$.id"))),
        step("get definition", "GET", "/api/definition/${defId2}", [OK()]),
        step("create definition case", "POST", "/api/definition/${defId2}/case", [OK(), C("id")],
             body={"name": "boot-def-case", "method": "GET", "url": "/healthz"}),
        step("list definition cases", "GET", "/api/definition/${defId2}/case", [OK()]),
        step("update definition status", "PUT", "/api/definition/${defId2}/status", [OK()], body={"status": "DEBUGGING"}),
        step("get references", "GET", "/api/definition/${defId2}/references", [OK()]),
        step("delete definition", "DELETE", "/api/definition/${defId2}", [OK()]),
    ]))

    # ---- api-scenario ----
    M.append(("api-scenario", "scenario CRUD+compile+run", [login,
        step("create scenario", "POST", "/api/scenario", [OK(), C("id")],
             body={"projectId": pid, "name": "boot-sub-scenario"}, processors=EX(("scId2", "$.id"))),
        step("add request step", "POST", "/api/scenario/${scId2}/step", [OK()],
             body={"kind": "REQUEST", "order": 0, "request": {"method": "GET", "url": "/healthz"}}),
        step("get scenario", "GET", "/api/scenario/${scId2}", [OK(), C("steps")]),
        step("compile scenario", "GET", "/api/scenario/${scId2}/compile", [OK()]),
        step("delete scenario", "DELETE", "/api/scenario/${scId2}", [OK()]),
    ]))

    # ---- api-test (resource-pool) ----
    M.append(("api-test", "resource pool CRUD", [login,
        step("list pools", "GET", "/api/resource-pool", [OK()]),
        step("create pool", "POST", "/api/resource-pool", [OK(), C("id")],
             body={"name": "boot-pool", "poolType": "Node", "maxConcurrency": 5, "enabled": True,
                   "allOrg": True, "orgIds": [], "serverUrl": "", "description": "bootstrap"},
             processors=EX(("poolId2", "$.id"))),
        step("get pool", "GET", "/api/resource-pool/${poolId2}", [OK()]),
        step("delete pool", "DELETE", "/api/resource-pool/${poolId2}", [OK()]),
    ]))

    # ---- requirement (versions/baseline; note: baseline body is {version:N}) ----
    M.append(("requirement", "requirement lifecycle", [login,
        step("create requirement", "POST", "/requirement", [OK(), C("id")],
             body={"projectId": pid, "title": "boot-req-${__runid}", "description": "desc", "acceptanceCriteria": ["AC1"]},
             processors=EX(("reqId2", "$.id"))),
        step("get requirement", "GET", "/requirement/${reqId2}", [OK()]),
        step("update requirement", "PUT", "/requirement/${reqId2}", [OK()], body={"title": "boot-req-${__runid}-upd"}),
        step("add version", "POST", "/requirement/${reqId2}/version", [OK()],
             body={"description": "v2", "acceptanceCriteria": ["AC1", "AC2"]}),
        step("baseline", "PUT", "/requirement/${reqId2}/baseline", [OK()], body={"version": 1}),
        step("get version 1", "GET", "/requirement/${reqId2}/version/1", [OK()]),
        step("list requirements", "GET", f"/requirement?projectId={pid}", [OK()]),
    ]))

    # ---- test-plan ----
    M.append(("test-plan", "test plan lifecycle", [login,
        step("create plan", "POST", "/test-plan", [OK(), C("id")],
             body={"projectId": pid, "name": "boot-plan", "type": "TEST_PLAN"},
             processors=EX(("planId2", "$.id"))),
        step("link case", "POST", "/test-plan/${planId2}/cases", [OK()],
             body={"caseId": sample_case_id, "name": "boot-plan-case"}),
        step("list plan cases", "GET", "/test-plan/${planId2}/cases", [OK()]),
        step("plan statistics", "GET", "/test-plan/${planId2}/statistics", [OK()]),
        step("plan report", "GET", "/test-plan/${planId2}/report", [OK()]),
    ]))

    # ---- role / user-role ----
    M.append(("role", "role CRUD", [login,
        step("create role", "POST", "/role", [OK(), C("id")],
             body={"name": "boot-role", "permissions": []}, processors=EX(("roleId2", "$.id"))),
        step("get role", "GET", "/role/${roleId2}", [OK()]),
        step("update role", "PUT", "/role/${roleId2}", [OK()], body={"name": "boot-role-upd", "permissions": []}),
        step("list roles", "GET", "/role", [OK()]),
        step("delete role", "DELETE", "/role/${roleId2}", [OK()]),
    ]))

    # ---- user ----
    M.append(("user", "user CRUD", [login,
        step("create user", "POST", "/system/user", [OK(), C("id")],
             body={"name": "boot-user-${__runid}", "email": "bootstrap-${__runid}@example.com"}, processors=EX(("userId2", "$.id"))),
        step("get user", "GET", "/system/user/${userId2}", [OK()]),
        step("update user", "PUT", "/system/user/${userId2}", [OK()],
             body={"name": "boot-user-${__runid}-upd", "email": "bootstrap-${__runid}@example.com", "enable": True}),
        step("list users", "GET", "/system/user", [OK()]),
        step("user name map", "GET", "/system/user/names", [OK()]),
        step("delete user", "DELETE", "/system/user/${userId2}", [OK()]),
    ]))

    # ---- skill ----
    M.append(("skill", "skill CRUD+compose", [login,
        step("create skill", "POST", "/skill", [OK(), C("id")],
             body={"projectId": pid, "name": "boot-skill", "instructions": "do x", "description": "d", "includes": []},
             processors=EX(("skillId2", "$.id"))),
        step("get skill", "GET", "/skill/${skillId2}", [OK()]),
        step("update skill", "PUT", "/skill/${skillId2}", [OK()],
             body={"name": "boot-skill-upd", "instructions": "do y", "description": "d", "includes": [], "enabled": True}),
        step("list skills", "GET", f"/skill?projectId={pid}", [OK()]),
        step("compose skills", "POST", "/skill/compose", [OK()],
             body={"projectId": pid, "skillIds": ["${skillId2}"]}),
        step("delete skill", "DELETE", "/skill/${skillId2}", [OK()]),
    ]))

    # ---- functional-case ----
    M.append(("functional-case", "functional case create+list+export", [login,
        step("create functional case", "POST", "/functional-case", [OK(), C("id")],
             body={"projectId": pid, "name": "boot-func-case-${__runid}", "priority": "P1", "status": "PREPARED",
                   "module": "", "steps": [], "customFields": {}}),
        step("list functional cases", "GET", f"/functional-case?projectId={pid}", [OK()]),
        step("export functional cases", "GET", f"/functional-case/export?projectId={pid}", [OK()]),
    ]))

    # ---- bug ----
    M.append(("bug", "bug create+transition", [login,
        step("create bug", "POST", "/bug", [OK(), C("id")],
             body={"projectId": pid, "title": "boot-bug-${__runid}", "initialStatus": "NEW"},
             processors=EX(("bugId2", "$.id"))),
        step("transition bug status", "POST", "/bug/${bugId2}/status", [OK()], body={"status": "RESOLVED"}),
    ]))

    # ---- case-review ----
    M.append(("case", "case review", [login,
        step("start review", "POST", "/case-review", [OK(), C("id")],
             body={"projectId": pid, "caseIds": [sample_case_id], "passRule": "SINGLE", "reviewerCount": 1},
             processors=EX(("reviewId2", "$.id"))),
        step("get review", "GET", "/case-review/${reviewId2}", [OK()]),
        step("submit review opinion", "POST", f"/case-review/${{reviewId2}}/{sample_case_id}", [OK()],
             body={"reviewerId": admin_uid, "status": "PASS", "content": "ok"}),
        step("list reviews", "GET", f"/case-review?projectId={pid}", [OK()]),
    ]))

    # ---- task / decomposition (depends on a requirement) ----
    M.append(("task", "task decomposition", [login,
        step("create requirement (seed)", "POST", "/requirement", [OK(), C("id")],
             body={"projectId": pid, "title": "boot-task-req-${__runid}", "description": "d", "acceptanceCriteria": ["AC1"]},
             processors=EX(("reqForTask", "$.id"))),
        step("baseline v1", "PUT", "/requirement/${reqForTask}/baseline", [OK()], body={"version": 1}),
        step("create decomposition", "POST", "/decomposition", [OK(), C("id")],
             body={"requirementId": "${reqForTask}", "requirementVersion": 1}, processors=EX(("decId2", "$.id"))),
        step("get decomposition", "GET", "/decomposition/${decId2}", [OK()]),
        step("add task", "POST", "/decomposition/${decId2}/task", [OK(), C("taskId")],
             body={"title": "boot-task", "description": "d", "acceptanceCriteria": [], "dependencies": [], "points": 3},
             processors=EX(("taskId2", "$.taskId"))),
        step("update task points", "POST", "/decomposition/${decId2}/task/${taskId2}/points", [OK()], body={"points": 5}),
        step("update task status", "POST", "/decomposition/${decId2}/task/${taskId2}/status", [OK()], body={"status": "DISPATCHED"}),
        step("decomposition ready", "GET", "/decomposition/${decId2}/ready", [OK()]),
    ]))

    # ---- delivery (depends on decomposition+task) ----
    M.append(("delivery", "delivery lifecycle", [login,
        step("create requirement (seed)", "POST", "/requirement", [OK(), C("id")],
             body={"projectId": pid, "title": "boot-delivery-req-${__runid}", "description": "d", "acceptanceCriteria": ["AC1"]},
             processors=EX(("reqForDel", "$.id"))),
        step("baseline v1", "PUT", "/requirement/${reqForDel}/baseline", [OK()], body={"version": 1}),
        step("create decomposition", "POST", "/decomposition", [OK(), C("id")],
             body={"requirementId": "${reqForDel}", "requirementVersion": 1}, processors=EX(("decForDel", "$.id"))),
        step("add task", "POST", "/decomposition/${decForDel}/task", [OK(), C("taskId")],
             body={"title": "boot-delivery-task", "description": "d", "acceptanceCriteria": [], "dependencies": [], "points": 3},
             processors=EX(("taskForDel", "$.taskId"))),
        # CLAUDE_CODE is a **synchronous stub executor**: POST /delivery completes in
        # one step, created directly as DELIVERED. So no running/complete transitions
        # (those belong to async executors and need an online agent); asserting
        # DELIVERED confirms the synchronous delivery.
        step("create delivery (sync->DELIVERED)", "POST", "/delivery", [OK(), C("DELIVERED")],
             body={"decompositionId": "${decForDel}", "taskId": "${taskForDel}", "executor": "CLAUDE_CODE",
                   "title": "boot-delivery", "description": "d", "acceptanceCriteria": []},
             processors=EX(("delId2", "$.id"))),
        step("get delivery", "GET", "/delivery/${delId2}", [OK(), C("DELIVERED")]),
        step("add delivery event", "POST", "/delivery/${delId2}/events", [OK()], body={"kind": "LOG", "message": "hello"}),
        step("list delivery events", "GET", "/delivery/${delId2}/events", [OK()]),
    ]))

    # ---- verification (depends on a requirement + acceptance criteria) ----
    M.append(("verification", "verification create+link+report", [login,
        step("create requirement (seed)", "POST", "/requirement", [OK(), C("id")],
             body={"projectId": pid, "title": "boot-verify-req-${__runid}", "description": "d", "acceptanceCriteria": ["AC1"]},
             processors=EX(("reqForVer", "$.id"))),
        step("baseline v1", "PUT", "/requirement/${reqForVer}/baseline", [OK()], body={"version": 1}),
        step("create verification", "POST", "/verification", [OK(), C("id")],
             body={"requirementId": "${reqForVer}", "requirementVersion": 1, "criteria": ["AC1"]},
             processors=EX(("verId2", "$.id"))),
        step("get verification", "GET", "/verification/${verId2}", [OK()]),
        step("verification report", "GET", "/verification/${verId2}/report", [OK()]),
    ]))

    # ---- runner ----
    # Note: /runner/probe and /runner-agent/{id}/run need an **online runner agent**
    # (dispatch failure returns 502); no agent is online locally, so the runner
    # chain only covers the management plane (register/list/records).
    M.append(("runner", "runner register+list", [login,
        step("list agents", "GET", "/runner-agent", [OK()]),
        step("register agent", "POST", "/runner-agent", [OK(), C("id")],
             body={"name": "boot-agent-${__runid}", "baseUrl": BASE, "enabled": True}, processors=EX(("agentId2", "$.id"))),
        step("agent executions", "GET", "/runner-agent/${agentId2}/executions", [OK()]),
    ]))

    # ---- perf ----
    M.append(("perf", "perf single-endpoint+report", [login,
        step("start single-endpoint perf run", "POST", "/perf/run", [OK(), C("reportId")],
             body={"projectId": pid, "method": "GET", "url": BASE + "/healthz", "concurrency": 2, "iterations": 5},
             processors=EX(("perfRep", "$.reportId"))),
        step("get perf report", "GET", "/perf/report/${perfRep}", [OK()]),
    ]))

    # ---- auth ----
    M.append(("auth", "login/logout/negative", [login,
        step("logout", "POST", "/auth/logout", [OK()]),
        step("negative: wrong password", "POST", "/auth/login", [S(401)],
             body={"username": USER, "password": "wrong-pass"}, auth=False),
    ]))

    return M


def main():
    print(f"== Shepherd full-module scenario bootstrap @ {BASE} ==\n")
    token = j("POST", "/auth/login", body={"username": USER, "password": PASS})["token"]

    # Resolve org/project/admin uid
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
    print(f"project projectId={pid}  org orgId={oid}  adminUid={admin_uid}")

    # Environment (baseUrl points at this host, prefixes relative urls) — create or reuse
    envs = j("GET", f"/api/environment?projectId={pid}", token)
    env = find(envs, "自举环境")
    if not env:
        env = j("POST", "/api/environment", token,
                {"projectId": pid, "name": "自举环境", "baseUrl": BASE, "headers": [], "variables": {}})
    eid = env["id"]

    # Holder definition (carries all module cases so they can bear auth headers).
    # There is no "update case" endpoint -> delete the old holder each run
    # (cascades its cases) and recreate, so case specs track the script.
    defs = j("GET", f"/api/definition?projectId={pid}", token)
    old = find(defs, "自举-全模块")
    if old:
        j("DELETE", f"/api/definition/{old['id']}", token)
    holder = j("POST", "/api/definition", token,
               {"projectId": pid, "name": "自举-全模块", "method": "POST", "path": "/auth/login"})
    did = holder["id"]
    existing = {}

    # A real sample case id for test-plan/case-review: get-or-create a plain case
    sample_name = "boot-sample-case"
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

    print(f"\n{len(modules)} modules; building and running one scenario each (failure strategy CONTINUE)\n")
    grand_pass = grand_total = 0
    rows = []
    for key, title, steps in modules:
        # The login step reuses the same case across modules
        cases = []
        for sp in steps:
            prefix = "shared" if sp["name"].startswith("login") else key
            cases.append(ensure_case(prefix, sp))
        sname = f"自举M·{key}"
        scn = scns.get(sname)
        if not scn:
            scn = j("POST", "/api/scenario", token, {"projectId": pid, "name": sname})
            scns[sname] = scn
        sid = scn["id"]
        want = [c["id"] for c in cases]
        cur = j("GET", f"/api/scenario/{sid}", token).get("steps") or []
        # Note: in the scenario step response the CASE reference field is caseId
        # (not refId); align on it for idempotency (otherwise steps rebuild every run).
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

    print("\n== full-module summary ==")
    for mark, key, title, npass, ntot in rows:
        print(f"  {mark} {key:16} {npass}/{ntot}  {title}")
    nfull = sum(1 for r in rows if r[3] == r[4])
    print(f"\nmodules all green {nfull}/{len(rows)}; steps passed {grand_pass}/{grand_total}")
    return 0 if grand_pass == grand_total else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:
        print(f"\n!! bootstrap failed: {e}", file=sys.stderr)
        sys.exit(2)
