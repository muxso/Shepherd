---
name: openapi-bootstrap
description: >-
  Self-bootstrap (dogfood) Shepherd's own HTTP API. Fetches the running
  system's OpenAPI (/api-docs/openapi.json), idempotently imports it as API
  definitions (with parsed request params / required flags / responses + a
  default assertion-bearing case per interface), then builds a REAL chained
  scenario that references api cases (kind=CASE): login → extract token →
  authenticated calls (Bearer ${token}) → extract orgId → chained call, plus a
  negative 401 case. Runs it in-process with an environment and reports
  per-step pass/fail + extracted variables. A companion script
  (scenarios_all.py) builds one real CRUD/lifecycle scenario per business
  module (every OpenAPI tag — org, project, requirement, task, delivery,
  verification, test-plan, etc.) and executes all of them with a per-module
  pass/fail report (currently 20/20 modules green). Use when the user asks to
  自举 / dogfood / smoke-test the live API, to get scenario coverage across
  all business modules (所有业务模块的场景测试), or to orchestrate the
  system's own OpenAPI through /api/definition and /api/scenario.
---

# OpenAPI Self-Bootstrap

Test Shepherd's running HTTP API with Shepherd's own capabilities: API
definitions + cases + scenario orchestration + execution.

## Run

```bash
# A. Single-chain bootstrap (login → extract → authed chain + negative 401):
#    verifies the real-chain execution machinery
python3 .claude/skills/openapi-bootstrap/selftest.py

# B. Full-module scenario coverage: one CRUD/lifecycle scenario per OpenAPI tag,
#    executed with a per-module report
python3 .claude/skills/openapi-bootstrap/scenarios_all.py

# C. Test-plan operations self-check (plan CRUD, planning doc, case links,
#    plan run, single-case re-run, schedule)
python3 .claude/skills/openapi-bootstrap/plans_selftest.py
```

`scenarios_all.py` builds one real chained scenario per business module (20
modules), referencing api cases (kind=CASE, with auth headers / variable
extraction / assertions), ordered as CRUD/lifecycle chains and executed
in-process with an environment. Failure strategy CONTINUE → one run yields
per-step pass/fail + reasons for every module. Currently all green:
**20/20 modules, 114/114 steps**. There is no "update case" endpoint → each
run deletes the old holder definition (cascading its cases) and rebuilds it.
Persistent resources (project/user/requirement) carry a per-run unique suffix
so repeated runs do not hit unique constraints.

Real constraints confirmed while building (baked into the chains; keep in sync
if backend enums change):
- Resource pool `poolType` ∈ {`Node`,`Kubernetes`} (no LOCAL); with
  `allOrg=false`, `orgIds` must be non-empty.
- Task creation `POST /decomposition/{id}/task` returns `{"taskId": <slug>}`
  (not `id`); points/status endpoints use that taskId.
- Delivery executors `CLAUDE_CODE`/`CODEX` are **synchronous stubs**:
  `POST /delivery` completes in one step, created directly as `DELIVERED`
  (no running/complete async transitions).
- `/runner/probe` and `/runner-agent/{id}/run` need an **online runner agent**
  (502 otherwise); the local chain only covers the management plane.
- Several list endpoints require `?projectId=` (requirement / skill /
  functional-case / case-review).
- Creates return 201, deletes/partial updates return 204 → use the generic
  `ResponseCode < 400` success assertion instead of an exact `StatusIs(200)`.

| Env var | Default | Description |
|------|------|------|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | Backend address |
| `SHEPHERD_USER` / `SHEPHERD_PASS` | `admin` / `s3cret` | Login credentials (`SHEPHERD_ADMIN_PASSWORD`) |
| `SHEPHERD_PROJECT_ID` | auto-resolved | Target project; defaults to the first org's first project, created if missing |

Scripts, environments, definitions, cases and scenarios are all
create-or-reuse by name — repeated runs do not accumulate.
(Pre-existing resource names such as `自举环境` / `自举链路` are kept in
Chinese: they are persisted reuse-by-name keys; renaming them would orphan
existing rows.)

## Flow (7 steps)

1. Login for a token (admin calls for resource setup only)
2. Resolve/create organization → project
3. Fetch this system's OpenAPI (`GET /api-docs/openapi.json`) →
   **idempotent import** (`POST /api/definition/import`; same method+path
   overwrites the spec, no duplicates); sample-check that specs are populated
   and each interface has an assertion-bearing case
4. Create-or-reuse an environment pointing at this host (`baseUrl` = this server)
5. Create-or-reuse 4 **real cases** under the bootstrap-chain definition
6. Create-or-reuse a scenario **referencing** those cases as steps
   (`kind=CASE`), align order, execute with the environment
7. Fetch the report; print per-step ✅/❌ + extracted variables + assertion counts

## This is a real chain, not hardcoded GETs

The scenario **references api cases** (`kind=CASE`) instead of inlining
hardcoded requests. The cases cover POST + GET + auth headers + variable
extraction + cross-step chaining:

```
① login and extract token    POST /auth/login   assert status 200 + contains "token"  → extract token = $.token
② authed list organizations  GET  /organization  header Authorization: Bearer ${token} → extract orgId = $.items[0].id
③ authed list projects by orgId  GET /project?organizationId=${orgId}  (token + orgId both substituted)
④ negative: no token rejected    GET /organization  assert status 401
```

Key execution machinery (see `crates/api-test/src/adapters/{plan,local,pg}.rs`,
`crates/api-runner/src/domain/runner.rs`):
- **CASE steps run in-process**, loading the case's full
  method/url/body/headers/auth/assertions/processors — no resource pool needed
  (inline `REQUEST` steps drop headers, hence CASE).
- **EXTRACT processors** write `$.token` etc. into run variables, passed
  across steps.
- **`${var}` single-brace** substitution applies to url / header values / body
  (note: not `{{}}`).
- The **environment** baseUrl is prefixed onto relative urls; default headers
  fill gaps (same-named case headers win).
- Known limitation: `Variable` assertions cannot see run variables inside a
  scenario (`plan.rs` does not pass vars to assertion evaluation), so this
  script uses `StatusIs`/`BodyContains` instead of `Variable` assertions.

## Test-plan operations self-check

`plans_selftest.py` verifies test-plan operations end-to-end through the HTTP
API: plan create/reuse (plan id kept in the local state file
`.plan_selftest_id`, since there is no plan list/delete endpoint), `PUT`
update round-trip (description/tags/passThreshold), planning doc save + link
sync, case linking (one scenario + one API case), full plan run (all rows
SUCCESS, scenario row carries a reportId whose steps are all SUCCESS),
single-case re-run, schedule create/delete (201/204/404), and case unlink
(204 + list shrinks). Idempotent: running it twice in a row must be fully
green both times.

## Backend importer enhancements (shipped alongside)

`POST /api/definition/import` now:
- parses each operation's `parameters` (query/header/path; required flags
  encoded into the remark), `requestBody` (`$ref`/`allOf` resolved against
  `components` into a bodySchema tree + example) and `responses` (status code
  + schema example) → written into the definition `spec`;
- generates a default case (status code + basic business assertion) for each
  **new** interface;
- is **idempotent**: an existing project+method+path only gets its spec
  overwritten (user-edited cases are preserved); returns
  `{created, updated, skipped}`.

Code: `crates/api-definition/src/domain/import.rs`,
`application/import_api_definitions.rs`, `domain/api_definition.rs` (`with_spec`).

## Exit codes
`0` all green; `1` some step failed; `2` the bootstrap flow itself errored.
