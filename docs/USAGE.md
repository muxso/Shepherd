# Shepherd — Usage Guide

> Audience: anyone running, operating, or evaluating Shepherd. For production deployment (Helm, Terraform, CI/CD, day-2 ops) see [DEPLOYMENT.md](DEPLOYMENT.md). 简体中文版见 [USAGE.zh-CN.md](USAGE.zh-CN.md).

Shepherd is an AI-development *supervision* platform: AI writes the code, you stay in charge of what ships. It breaks a requirement down for AI executors to work on, puts a **human approval gate at two points** (design and verification), and keeps a record of the whole loop.

---

## 1. Overview & core concepts

A requirement travels through a fixed pipeline. The two gates are hard steps — however fast the AI runs, nothing reaches your main branch until a human clears a gate.

```
you file a requirement
        │
        ▼
   AI drafts a design  ──►  ⛔ DESIGN APPROVAL GATE (human)
        │                        approve ▼
        │                   split into a task DAG
        │                        │ dispatch
        ▼                        ▼
                          FLEET DISPATCH ──► agent-runtime executor
                          (pull / long-poll)   (claude / codex / opencode / codebuddy)
                                 │ runs in a git worktree
                                 ▼
                          deliverable (diff / PR)
                                 │ adjudication
                                 ▼
                          ⛔ VERIFICATION GATE (judge + human)
                                 │ pass
                                 ▼
                          sign-off (requirement satisfied)
```

| Concept | What it is |
|---|---|
| **Requirement** | A versioned unit of work (immutable, append-only version snapshots). |
| **AI design draft** | An architect-role agent drafts a design proposal from the requirement spec. |
| **Design approval gate** | A human reviews and approves/rejects the draft. Approval auto-triggers task breakdown. Nothing is split until this clears. |
| **Task DAG** | The requirement is decomposed into a directed acyclic graph of tasks with dependencies and readiness gating. |
| **Fleet dispatch** | Tasks are enqueued; executors **reach out and long-poll to claim work** — the server never pushes (see below). |
| **Deliverable** | The executor produces a diff / PR reference plus a summary. |
| **Verification gate** | A judge (and human) adjudicates the deliverable against acceptance criteria. Pass → task `Verified`, downstream tasks unlock. |
| **Sign-off** | When the coverage chain is satisfied the requirement is signed off. |

### Why the fleet pulls instead of being pushed to

Company AI tools (Claude Code, Codex, …) run on internal dev machines or CI — boxes with **no public inbound**. The Shepherd server has a public address. So the server can't push work to an executor; it has to be the other way around: **executors reach out and long-poll to claim work** (`GET /agent/work/claim`), report progress via callbacks, and heartbeat to stay registered.

- **Single host** — an in-process queue is enough; no external dependency.
- **Multiple hosts** — set `SHEPHERD_FLEET_REDIS` to switch to Redis Streams consumer groups: exactly-once claim, ack on terminal state, and timeout-based reclaim when an executor dies.

---

## 2. Prerequisites

| For | You need |
|---|---|
| Quick start (Docker) | Docker + Docker Compose v2 |
| From-source dev | Rust (stable, edition 2021; CI uses `rust:1.86`), Node.js 18+, a PostgreSQL 16 instance |
| Multi-host fleet | Redis 7 |
| Real AI executors | `git` plus the agent CLIs on `PATH` (`claude` / `codex` / `opencode` / `codebuddy`) |

PostgreSQL is required; the server **auto-applies migrations on startup**. Redis is required **only** for the multi-host fleet.

---

## 3. Quick start (Docker Compose)

The fastest way to see the full loop on a single host. This stack runs Postgres, Redis, the server (in fleet mode), one mock agent-runtime, and the web console behind nginx.

```bash
docker compose -f deploy/docker/docker-compose.yml up --build
```

Then open:

| URL | What |
|---|---|
| http://localhost:8080 | Web console (nginx-served SPA) |
| http://localhost:8088 | Server API |

**Log in** with `admin` / the `SHEPHERD_ADMIN_PASSWORD` set in the compose file (default `change-me-please` — change it for anything beyond a local trial).

Notes:
- The bundled `agent-runtime` runs with `AGENT_MOCK=1`, so it claims and "completes" tasks without invoking a real CLI — ideal for a smoke test. To exercise real backends, run an agent-runtime from source (§5) on a machine that has the CLIs.
- This is **not** a production deployment (inline secrets, local PG/Redis). For production use Helm/Terraform — see [DEPLOYMENT.md](DEPLOYMENT.md).

Tear down (and wipe the DB volume):

```bash
docker compose -f deploy/docker/docker-compose.yml down -v
```

---

## 4. From-source dev run

Three processes: Postgres, the server, the Vite dev console. Add an agent-runtime when you want real (or mock) dispatch.

### 4.1 Postgres

```bash
docker run -d --name shep-pg \
  -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass -e POSTGRES_DB=mstest \
  -p 55432:5432 postgres:16-alpine
```

### 4.2 Server

The workspace root sets `default-members`, so a plain `cargo run` builds and runs the `server` binary. Migrations apply automatically on boot.

```bash
DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
SHEPHERD_ADMIN_PASSWORD=s3cret \
cargo run -p server          # → http://localhost:8088
```

> The Vite dev proxy targets `http://127.0.0.1:9180` by default. To match it, bind the server there:
> `SHEPHERD_BIND=127.0.0.1:9180 … cargo run -p server` (or point Vite at :8088 with `SHEPHERD_API=http://127.0.0.1:8088`).

Get a token:

```bash
curl -s localhost:8088/auth/login -H 'content-type: application/json' \
  -d '{"username":"admin","password":"s3cret"}'
```

### 4.3 Web console (Vite dev)

```bash
cd web && npm install && npm run dev      # → http://localhost:5173
```

The dev server (port 5173) proxies all backend prefixes to the server, disambiguating SPA navigations from API calls by the `Accept` header.

### 4.4 Agent-runtime (executor)

Run on the machine that has the agent CLIs (or anywhere with `AGENT_MOCK=1`). It needs the server in **fleet mode**:

```bash
# Server, in fleet mode (single-host in-process queue)
SHEPHERD_AGENT_FLEET=1 DATABASE_URL=… SHEPHERD_ADMIN_PASSWORD=s3cret cargo run -p server

# Executor: outbound long-poll, claims CLAUDE_CODE tasks
SHEPHERD_BASE=http://<server>:8088 \
SHEPHERD_ADMIN_PASSWORD=s3cret \
SHEPHERD_CAPS=CLAUDE_CODE \
cargo run -p agent-runtime
```

For a demo without any real CLI, add `AGENT_MOCK=1`.

### 4.5 Tests

```bash
cargo test --workspace                 # everything; non-integration runs in seconds
cargo test --workspace -- --ignored    # real-database integration tests
cargo clippy --workspace -- -D warnings
```

---

## 5. Configuration (environment variables)

All server start-up switches are consolidated into a typed config (`crates/server/src/config.rs`).

### 5.1 Server (`server`)

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | `postgres://msuser:mspass@localhost:55432/mstest` | PostgreSQL connection string (**required** in prod) |
| `SHEPHERD_BIND` | `0.0.0.0:8088` | Main API listen address |
| `SHEPHERD_ADMIN_PASSWORD` | `admin` | Admin password, idempotently upserted on boot. **MUST override in prod.** |
| `SHEPHERD_SESSION_TTL_SECS` | `28800` (8h) | Session token lifetime |
| `SHEPHERD_AGENT_FLEET` | — | Presence enables **fleet mode** (dispatch enqueues; runtimes claim outbound) |
| `SHEPHERD_FLEET_REDIS` | — | Redis URL for the distributed queue/registry (multi-host). Omit → in-process (single-host) |
| `SHEPHERD_FLEET_REAP_INTERVAL_S` | `15` | Reaper poll interval — requeues pending work from dead runtimes |
| `SHEPHERD_FLEET_RECLAIM_MS` | `30000` | Grace period before a dead runtime's in-flight work is reclaimed |
| `SHEPHERD_EXECUTOR_URL` | — | Remote JMeter dispatch endpoint for API batch-run (optional) |
| `SHEPHERD_RUNNER` | — | `noop` → no local API runner (batch-run stays `RUNNING`, demo only) |
| `SHEPHERD_FEISHU_APP_ID` / `_APP_SECRET` / `_REDIRECT` | — | Feishu OIDC login (provider registered only when id+secret both set) |
| `SHEPHERD_WECOM_CORP_ID` / `_SECRET` / `_REDIRECT` | — | WeCom OIDC login |
| `MOCK_BIND` | — | Optional separate listen address for the Mock server |

Advanced/lazy-read switches also exist for the pluggable AI touchpoints — `SHEPHERD_AGENT_URL` / `SHEPHERD_AGENT_CMD` / `SHEPHERD_AGENT_ASYNC` (executor routing), `SHEPHERD_LLM_URL`, `SHEPHERD_PLANNER_URL`, `SHEPHERD_JUDGE_URL`, `SHEPHERD_MAX_REVISIONS`. Defaults need none of them.

### 5.2 Agent-runtime (`agent-runtime`)

| Variable | Default | Meaning |
|---|---|---|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | Server address to long-poll |
| `SHEPHERD_ADMIN_USER` | `admin` | Login user |
| `SHEPHERD_ADMIN_PASSWORD` | `s3cret` | Login password |
| `SHEPHERD_CAPS` | `CLAUDE_CODE` | Comma-separated capabilities — which task kinds this runtime claims (e.g. `CLAUDE_CODE,CODEX`) |
| `AGENT_CONCURRENCY` | `1` | Max concurrent in-flight tasks (semaphore-bounded) |
| `AGENT_WORKDIR` | `.` | Base working directory; each task runs in its own git worktree under it |
| `RUNTIME_NAME` | `agent-runtime` | Display name in the fleet registry |
| `AGENT_MOCK` | — | Presence → mock backend (no real CLI) |
| `CLAUDE_BIN` | `claude` | Claude CLI binary |
| `CODEX_CMD` | `codex exec` | Codex CLI invocation |
| `OPENCODE_CMD` | `opencode run` | OpenCode CLI invocation |
| `CODEBUDDY_CMD` | `codebuddy -p --permission-mode acceptEdits` | CodeBuddy CLI invocation |

---

## 6. The web UI tour

Log in at the console (`admin` / your password). Pages map to the pipeline above:

| Page | What you do there |
|---|---|
| **Login** | Local login, or OIDC (Feishu / WeCom) if configured. |
| **Home** | Landing dashboard — API case execution summary. |
| **Org & Projects** | Manage organizations and projects (the top-level container for everything below). |
| **Project Admin** | Per-project settings. |
| **Requirements** | File and version requirements; trigger breakdown into the task DAG. |
| **Task Center** | The task DAG — dependencies, readiness, dispatch and delivery status. |
| **Agents** | The fleet view — registered runtimes, per-capability backlog / in-flight (backed by `GET /agent/work/stats`). |
| **Skills** | Define and compose AI Skills that shape executor behaviour. |
| **MCP** | Inspect / drive the MCP tool surface. |
| **API Definitions / Scenarios / Environments** | API test management — definitions, chained scenarios, environments. |
| **Functional Cases / Cases / Review** | Functional test cases, case panels, review. |
| **Test Plans** | Test plans and execution. |
| **Bugs** | Defect tracking. |
| **Resource Pool / Runner Agents (Agents) / Perf / Mocks** | Resource pools, native runners, performance runs, the mock runtime. |
| **Users / User Groups** | Users, roles, RBAC. |
| **Message Settings** | Notification / message configuration. |
| **File Management** | Project file management. |

Approval and verification gates surface inside the Requirements / Task flow — design proposals are reviewed before breakdown, deliverables before sign-off.

---

## 7. Fleet & agent-runtime setup

The `agent-runtime` is the executor: bounded concurrency (semaphore), drains in-flight tasks on shutdown, and each task runs in its own git worktree so concurrent tasks don't collide.

### 7.1 Single-host vs multi-host

| Mode | Server flags | Queue |
|---|---|---|
| **Single-host** | `SHEPHERD_AGENT_FLEET=1` | In-process in-memory queue (no external dep) |
| **Multi-host** | `SHEPHERD_AGENT_FLEET=1` + `SHEPHERD_FLEET_REDIS=redis://host:6379` | Redis Streams consumer groups — exactly-once claim, ack on terminal state, timeout reclaim |

Multi-host lets you run multiple server replicas and many runtimes; the reaper (`SHEPHERD_FLEET_REAP_INTERVAL_S` / `SHEPHERD_FLEET_RECLAIM_MS`) requeues work from runtimes that stop heartbeating.

### 7.2 Registering real backends vs mock

The runtime picks a backend per task by its `executor` kind, unless `AGENT_MOCK=1` forces the mock:

| Capability | Backend | CLI (override env) |
|---|---|---|
| `CLAUDE_CODE` | Claude (streaming `stream-json`) | `claude` (`CLAUDE_BIN`) |
| `CODEX` | generic CLI | `codex exec` (`CODEX_CMD`) |
| `OPENCODE` | generic CLI | `opencode run` (`OPENCODE_CMD`) |
| `CODEBUDDY` | generic CLI | `codebuddy -p --permission-mode acceptEdits` (`CODEBUDDY_CMD`) |
| any (with `AGENT_MOCK=1`) | mock — returns canned output | none |

Real backends need `git` and the CLI on `PATH` (or pointed at via the override env). Per-executor run recipes (login, permission modes, dispatch examples) are in [EXECUTORS.md](./EXECUTORS.md). Adding a new backend means implementing one `CliAgentBackend` (`async fn execute(prompt, cwd, sink)`) and registering an enum variant — see `crates/agent-runtime/src/backend.rs`.

### 7.3 Observability

```bash
curl -s localhost:8088/agent/work/stats   # per-capability: ready (backlog), in-flight, oldest-stuck
```

Registry/heartbeat endpoints (used by runtimes, require `DELIVERY:UPDATE`): `POST /agent/runtime` (register), `GET /agent/runtime` (list), `POST /agent/runtime/{id}/heartbeat`.

---

## 8. HTTP API

### 8.1 Auth

```bash
# Login → returns a session token
curl -s localhost:8088/auth/login -H 'content-type: application/json' \
  -d '{"username":"admin","password":"s3cret"}'

# Use it as a Bearer token
curl -s localhost:8088/organization -H 'Authorization: Bearer <token>'
```

Sessions are PG-backed (survive a server restart) and expire after `SHEPHERD_SESSION_TTL_SECS`. Write endpoints enforce per-resource RBAC; reads are open.

### 8.2 Health

| Endpoint | Meaning |
|---|---|
| `GET /healthz` | Liveness — `200 ok` while the process is up (no dependency check) |
| `GET /readyz` | Readiness — `200` when Postgres is reachable, else `503` (2s timeout) |

### 8.3 MCP

The full pipeline is exposed as MCP tools over Streamable HTTP at `POST /mcp` (JSON-RPC): `initialize` issues an `Mcp-Session-Id`, `GET /mcp` holds an SSE stream, `DELETE /mcp` terminates. Tools are RBAC-filtered per session (`tools/list` hides tools you can't call). About ten `shepherd_*` tools drive requirements → breakdown → dispatch → verification.

### 8.4 OpenAPI & self-bootstrap (dogfood)

The server publishes its own OpenAPI at `GET /api-docs/openapi.json`. Shepherd can test **itself** through its own API-definition / scenario / execution machinery — import the live OpenAPI as API definitions, build real chained scenarios (login → extract token → authenticated calls → chained call, plus a negative 401), and execute them in-process with per-step pass/fail.

```bash
# Single chained self-bootstrap (login → extract → authed chain + negative 401)
python3 .claude/skills/openapi-bootstrap/selftest.py

# One CRUD/lifecycle scenario per business module, with a per-module report
python3 .claude/skills/openapi-bootstrap/scenarios_all.py
```

Both honour `SHEPHERD_BASE` (default `http://127.0.0.1:9180`) and `SHEPHERD_USER` / `SHEPHERD_PASS` (default `admin` / `s3cret`). See `.claude/skills/openapi-bootstrap/SKILL.md` for details.

---

## 9. Troubleshooting

| Symptom | Likely cause / fix |
|---|---|
| `/readyz` returns 503 | Postgres unreachable. Check `DATABASE_URL` and that PG is up; the check has a 2s timeout. |
| Login fails | Wrong password — it's `admin` / `SHEPHERD_ADMIN_PASSWORD` (compose default `change-me-please`, README dev `s3cret`), not the literal `admin` unless you left the default. |
| Web console shows blank / API 404 in dev | Vite proxy target mismatch. The dev proxy points at `:9180`; bind the server there or set `SHEPHERD_API` to your server URL. |
| Tasks never get claimed | Server not in fleet mode (`SHEPHERD_AGENT_FLEET=1`), no runtime online, or capability mismatch — check `SHEPHERD_CAPS` vs the task's executor kind, and `GET /agent/work/stats`. |
| Multi-host runtimes can't share work | `SHEPHERD_FLEET_REDIS` not set (or not the same Redis) on all server replicas → each falls back to its own in-process queue. |
| Real agent does nothing / errors spawning | CLI not on `PATH`; set `CLAUDE_BIN` / `CODEX_CMD` / `OPENCODE_CMD` / `CODEBUDDY_CMD`, or run with `AGENT_MOCK=1` to confirm the loop. |
| API batch-run stuck `RUNNING` with no results | `SHEPHERD_RUNNER=noop` is set (demo placeholder). Unset it to use the native runner. |
| New migration not applied | Restart the server — migrations run on boot; a new migration file needs a rebuild. |
| OIDC endpoint 404 | The provider is only registered when **both** id and secret env vars are set. |

---

For production deployment, image building, Helm/Terraform, CI/CD and day-2 operations, see **[DEPLOYMENT.md](DEPLOYMENT.md)**.
