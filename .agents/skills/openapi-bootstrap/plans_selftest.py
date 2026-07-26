#!/usr/bin/env python3
"""Test-plan operations self-check: verify plan CRUD, planning doc, case links,
plan runs and schedules end-to-end through Shepherd's own HTTP API.

Chain:
  1. Login; resolve/create organization -> project
  2. Ensure a small real scenario "plan-selftest-chain" (2 CASE-referenced
     steps: login-and-extract-token, authed-list-orgs) with its default
     environment set (plan runs pass no environment, so the scenario's own
     meta.envId must resolve relative urls)
  3. Create-or-reuse the plan (id kept in the local state file
     .plan_selftest_id — there is no plan list/delete endpoint)
  4. PUT update (description/tags/passThreshold) -> GET round-trip
  5. PUT planning doc carrying the scenario id -> linkedCases > 0, GET returns
     the doc verbatim
  6. Link one API case (absolute-url healthz, runs without an environment)
  7. POST run {} -> all executed, all SUCCESS; scenario row carries a reportId
     whose step results are all SUCCESS
  8. Single-case re-run of the scenario -> SUCCESS
  9. Schedule: create 201, delete 204, second delete 404
 10. Unlink the API case: 204, list shrinks

Idempotent: create-or-reuse by name; running twice in a row stays green.
Exit codes: 0 all green; 1 some check failed; 2 the flow itself errored.
"""
import json
import os
import sys
import urllib.request
import urllib.error

BASE = os.environ.get("SHEPHERD_BASE", "http://127.0.0.1:9180")
USER = os.environ.get("SHEPHERD_USER", "admin")
PASS = os.environ.get("SHEPHERD_PASS", "s3cret")

STATE_FILE = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".plan_selftest_id")

PLAN_NAME = "plan-selftest"
SCENARIO_NAME = "plan-selftest-chain"
ENV_NAME = "plan-selftest-env"
HOLDER_NAME = "plan-selftest-cases"
HEALTHZ_CASE = "plan-selftest-healthz"

failures = []


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


def check(label, ok, detail=""):
    mark = "✅" if ok else "❌"
    suffix = f"  {detail}" if detail else ""
    print(f"  {mark} {label}{suffix}")
    if not ok:
        failures.append(label)
    return ok


def main():
    print(f"== Shepherd test-plan self-check @ {BASE} ==\n")

    # 1) Login + org/project
    token = j("POST", "/auth/login", body={"username": USER, "password": PASS})["token"]
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
    print(f"[1/8] logged in, project={pid}")

    # 2) Environment + holder definition + cases + scenario (create-or-reuse by name)
    envs = j("GET", f"/api/environment?projectId={pid}", token)
    env = find(envs, ENV_NAME)
    if not env:
        env = j("POST", "/api/environment", token,
                {"projectId": pid, "name": ENV_NAME, "baseUrl": BASE, "headers": [], "variables": {}})
    eid = env["id"]

    defs = j("GET", f"/api/definition?projectId={pid}", token)
    holder = find(defs, HOLDER_NAME)
    if not holder:
        holder = j("POST", "/api/definition", token,
                   {"projectId": pid, "name": HOLDER_NAME, "method": "POST", "path": "/auth/login"})
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

    login_case = ensure_case(
        "login-and-extract-token", "POST", "/auth/login",
        [{"type": "StatusIs", "args": 200}, {"type": "BodyContains", "args": "token"}],
        headers=[{"key": "Content-Type", "value": "application/json"}],
        body=json.dumps({"username": USER, "password": PASS}),
        processors=[{"type": "Extract", "args": {"extractors": [
            {"variable": "token", "kind": "JSON_PATH", "expression": "$.token", "scope": "TEMP"}]}}])
    orgs_case = ensure_case(
        "authed-list-orgs", "GET", "/organization?pageSize=5",
        [{"type": "StatusIs", "args": 200}, {"type": "BodyContains", "args": "items"}],
        headers=[{"key": "Authorization", "value": "Bearer ${token}"}])
    # Absolute url: plan runs execute API cases without an environment.
    healthz_case = ensure_case(
        HEALTHZ_CASE, "GET", BASE + "/healthz",
        [{"type": "ResponseCode", "args": {"condition": "LT", "expected": "400"}}])

    scns = j("GET", f"/api/scenario?projectId={pid}", token)
    scn = find(scns, SCENARIO_NAME)
    if not scn:
        scn = j("POST", "/api/scenario", token, {"projectId": pid, "name": SCENARIO_NAME})
    sid = scn["id"]
    want = [login_case["id"], orgs_case["id"]]
    cur = j("GET", f"/api/scenario/{sid}", token).get("steps") or []
    if [s.get("caseId") for s in cur] != want:
        for s in cur:
            j("DELETE", f"/api/scenario/{sid}/step/{s['id']}", token)
        for i, cid in enumerate(want):
            j("POST", f"/api/scenario/{sid}/step", token, {"kind": "CASE", "order": i, "refId": cid})
    # Default environment for plan-triggered runs (meta.envId).
    j("PATCH", f"/api/scenario/{sid}", token,
      {"name": SCENARIO_NAME, "meta": {"envId": eid}})
    print(f"[2/8] scenario {SCENARIO_NAME}={sid} (2 CASE steps, env={eid})")

    # 3) Plan: reuse the id from the state file if it still resolves, else create
    plan_id = None
    if os.path.exists(STATE_FILE):
        with open(STATE_FILE) as f:
            candidate = f.read().strip()
        if candidate and call("GET", f"/test-plan/{candidate}", token)[0] == 200:
            plan_id = candidate
    if not plan_id:
        plan = j("POST", "/test-plan", token,
                 {"projectId": pid, "name": PLAN_NAME, "type": "TEST_PLAN"})
        plan_id = plan["id"]
        with open(STATE_FILE, "w") as f:
            f.write(plan_id)
    print(f"[3/8] plan {PLAN_NAME}={plan_id}")

    # 4) Update round-trip
    j("PUT", f"/test-plan/{plan_id}", token,
      {"description": "test-plan self-check", "tags": ["selftest"], "passThreshold": 90})
    detail = j("GET", f"/test-plan/{plan_id}", token)
    print("[4/8] update round-trip")
    check("PUT/GET description", detail.get("description") == "test-plan self-check")
    check("PUT/GET tags", detail.get("tags") == ["selftest"])
    check("PUT/GET passThreshold", abs(detail.get("passThreshold", 0) - 90.0) < 1e-9)

    # 5) Planning doc with a test point carrying the scenario
    doc = {
        "nodes": [{
            "id": "n-root", "name": "api", "kind": "category",
            "children": [{
                "id": "n-chain", "name": "auth chain", "kind": "point",
                "caseIds": [], "scenarioIds": [sid], "config": {"mode": "serial"},
            }],
        }],
        "scenarioNames": {sid: SCENARIO_NAME},
    }
    saved = j("PUT", f"/test-plan/{plan_id}/planning", token, doc)
    detail = j("GET", f"/test-plan/{plan_id}", token)
    print("[5/8] planning doc")
    check("planning linkedCases > 0", saved.get("linkedCases", 0) > 0,
          f"linkedCases={saved.get('linkedCases')}")
    check("planning GET round-trip", detail.get("planning") == doc)

    # 6) Case links: the planning save synced the scenario; add one API case
    cases = j("GET", f"/test-plan/{plan_id}/cases", token)
    check("scenario linked via planning", any(c["caseId"] == sid for c in cases))
    if not any(c["caseId"] == healthz_case["id"] for c in cases):
        st, _ = call("POST", f"/test-plan/{plan_id}/cases", token,
                     {"caseId": healthz_case["id"], "name": HEALTHZ_CASE})
        check("link API case 201", st == 201, f"status={st}")
    cases = j("GET", f"/test-plan/{plan_id}/cases", token)
    check("plan has scenario + API case", len(cases) == 2, f"linked={len(cases)}")

    # 7) Full plan run
    run = j("POST", f"/test-plan/{plan_id}/run", token, {})
    print("[7/8] plan run")
    check("executed > 0", run["executed"] > 0, f"executed={run['executed']}")
    check("success == executed", run["success"] == run["executed"],
          f"success={run['success']} failed={run['failed']}")
    cases = j("GET", f"/test-plan/{plan_id}/cases", token)
    check("every linked case SUCCESS", all(c["status"] == "SUCCESS" for c in cases),
          ", ".join(f"{c['name']}={c['status']}" for c in cases))
    scn_row = next((c for c in cases if c["caseId"] == sid), None)
    report_id = (scn_row or {}).get("reportId")
    check("scenario row carries reportId", bool(report_id))
    if report_id:
        rpt = j("GET", f"/api/scenario-report/{report_id}", token)
        results = rpt.get("results") or []
        check("scenario report steps all SUCCESS",
              bool(results) and all(r["outcome"] == "SUCCESS" for r in results),
              f"{sum(1 for r in results if r['outcome'] == 'SUCCESS')}/{len(results)}")

    # 8) Single-case re-run of the scenario entry
    rerun = j("POST", f"/test-plan/{plan_id}/cases/{sid}/run", token)
    check("single-case re-run SUCCESS", rerun.get("status") == "SUCCESS",
          f"status={rerun.get('status')}")

    # 9) Schedule lifecycle
    st, _ = call("POST", f"/test-plan/{plan_id}/schedule", token, {"cron": "0 0 * * * *"})
    check("schedule create 201", st == 201, f"status={st}")
    st, _ = call("DELETE", f"/test-plan/{plan_id}/schedule", token)
    check("schedule delete 204", st == 204, f"status={st}")
    st, _ = call("DELETE", f"/test-plan/{plan_id}/schedule", token)
    check("second schedule delete 404", st == 404, f"status={st}")

    # 10) Unlink the API case; the scenario link stays for the next run
    before = len(j("GET", f"/test-plan/{plan_id}/cases", token))
    st, _ = call("DELETE", f"/test-plan/{plan_id}/cases/{healthz_case['id']}", token)
    check("unlink API case 204", st == 204, f"status={st}")
    after = j("GET", f"/test-plan/{plan_id}/cases", token)
    check("cases list shrank", len(after) == before - 1
          and not any(c["caseId"] == healthz_case["id"] for c in after),
          f"{before} -> {len(after)}")

    total = len(failures)
    if total:
        print(f"\n== test-plan self-check: {total} check(s) failed ==")
        for f in failures:
            print(f"   ❌ {f}")
        return 1
    print("\n== test-plan self-check: all checks passed ==")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:
        print(f"\n!! test-plan self-check failed: {e}", file=sys.stderr)
        sys.exit(2)
