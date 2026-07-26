# Shepherd

<img src="web/public/logo.svg" alt="Shepherd logo" width="88" align="right" />

Shepherd is an end-to-end AI Software Engineering platform that supervises the complete software delivery lifecycle—not just code generation, but planning, execution, testing, regression, governance, deployment, and continuous knowledge growth.  
AI ROI · human-AI efficiency analytics · software engineering modeling · Task Graph.

[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-GPL--2.0-blue.svg)](LICENSE)
[![status](https://img.shields.io/badge/status-experimental-yellow.svg)](#status)
&nbsp; · &nbsp; [简体中文](README.zh-CN.md) | English

Shepherd is a platform for supervising AI-driven development. AI can write code, but it won't judge whether it actually finished the requirement, and it won't be accountable for the result. Instead of building another "smarter agent," Shepherd sits around the agent: it breaks requirements down for AI executors to work on, puts a human approval step at two points (design and verification), and keeps a record of the whole thing.

<a id="status"></a>
> **Status:** `v0.0.4`, experimental, dogfooded internally. The full loop works, but it isn't production-ready and there's no public benchmark. Don't treat it as a finished tool.

<img src="docs/assets/screenshot-intro.png" alt="Shepherd — scenario run report" width="100%" />

<!-- demo GIF pending: see docs/assets/. Uncomment the next line once it exists. -->
<!-- ![demo](docs/assets/demo.gif) -->

## How it works

A requirement roughly travels like this:

```
you file a requirement → AI drafts a design →(you approve)→ split into a task DAG → internal executors claim and work
                                                                                              │
you sign off ←(verification gate)← adjudication ← deliverable (diff / PR) ←────────────────────┘
```

The design gate and the verification gate are hard steps in the flow, not optional prompts. No matter how fast the AI runs, nothing reaches your main branch until it clears a gate. That's the problem Shepherd is trying to solve: putting AI to work at scale while keeping the output reviewable and accountable.

## Why the fleet pulls instead of being pushed to

The AI tools in a company (Claude Code, Codex, and friends) usually run on internal dev machines or CI — boxes with no public inbound. The server, on the other hand, has a public address. So the server can't push work to an executor; it has to be the other way around: **executors reach out and long-poll to claim work.** Plenty of orchestration frameworks assume they can reach the agent, and that assumption falls apart on a real internal network.

- Single host: an in-process queue is enough, no external dependencies.
- Multiple hosts: set `SHEPHERD_FLEET_REDIS` to switch to Redis Streams consumer groups — exactly-once claim, ack on terminal state, and timeout-based reclaim when an executor dies.

The executor itself (`agent-runtime`) is plain Rust: concurrency is bounded by a semaphore, it drains in-flight tasks on shutdown, and each task runs in its own git worktree so they don't step on each other. `GET /agent/work/stats` shows per-capability backlog, in-flight, and oldest-stuck. Adding a new kind of executor means implementing one `CliAgentBackend` (a single `execute(prompt, cwd, sink)` method) and registering an enum variant.

```
        (public)                            (internal, no inbound)
 ┌────────────┐  enqueue ┌──────────────┐  ←claim─ ┌──────────────────┐
 │  Shepherd  │─────────▶│  WorkQueue   │          │  agent-runtime   │
 │   server   │          │ in-mem/Redis │  ─spec→  │  spawn claude /  │
 │ (dispatch/ │◀─────────│  Streams grp │          │  codex / opencode│
 │   gates)   │ callback └──────────────┘ register/└──────────────────┘
 └────────────┘                           heartbeat per-task worktree isolation
```

## Screenshots

<p float="left">
  <img src="docs/assets/screenshot-en-1.png" width="49%" alt="Shepherd console — overview" />
  <img src="docs/assets/screenshot-en-2.png" width="49%" alt="Shepherd — requirement decomposition, delivery and verification" />
</p>
<p float="left">
  <img src="docs/assets/screenshot-en-3.png" width="49%" alt="Shepherd — API definitions imported from OpenAPI" />
  <img src="docs/assets/screenshot-en-4.png" width="49%" alt="Shepherd — scenario editor with chained case steps" />
</p>
<p float="left">
  <img src="docs/assets/screenshot-en-5.png" width="49%" alt="Shepherd — test plan planning mind-map" />
  <img src="docs/assets/screenshot-en-6.png" width="49%" alt="Shepherd — message center with in-app notifications" />
</p>
<p float="left">
  <img src="docs/assets/screenshot-en-7.png" width="49%" alt="Shepherd — resource pool with online WebSocket runners" />
</p>

## Measuring what AI actually delivered

"How much is AI really helping?" is hard to answer — lines of AI-written code and suggestion acceptance rates measure activity, not outcomes. Shepherd can measure it more honestly, because the acceptance gate is already in the flow: **a task counts as AI-delivered only when the delivery came from an AI executor *and* a human verified it.** Rejected work counts against quality, not against output.

On top of that rule you get, per project and per requirement:

- the AI / human split of delivered tasks and of workload points — each requirement carries an "AI share" you can read as a percentage;
- delivery quality: attempts vs. deliveries vs. failures, and the first-pass rate (delivered and verified on the first try);
- a contribution-calendar view of daily deliveries, AI and human side by side.

It measures shipped outcomes rather than keystrokes. The numbers come out smaller and more boring than "90% of our code is AI-written" claims — which is the point.

## Running it

```bash
# 1) A Postgres; migrations are applied automatically on startup
docker run -d --name shep-pg \
  -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass -e POSTGRES_DB=mstest \
  -p 55432:5432 postgres:16-alpine

# 2) Start the server (the root sets default-members, so plain `cargo run` works)
DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
SHEPHERD_ADMIN_PASSWORD=s3cret \
cargo run                       # → http://localhost:8088
```

```bash
# Log in for a token
curl -s localhost:8088/auth/login -H 'content-type: application/json' \
  -d '{"username":"admin","password":"s3cret"}'
```

The web console, and starting an executor on an internal machine:

```bash
cd web && npm install && npm run dev          # Vite console

SHEPHERD_AGENT_FLEET=1 cargo run              # server in fleet mode
SHEPHERD_BASE=http://<server>:8088 SHEPHERD_CAPS=CLAUDE_CODE cargo run -p agent-runtime
```

<!-- console screenshot pending: see docs/assets/. -->

Prefer one command? `docker compose -f deploy/docker/docker-compose.yml up --build` brings up the whole stack (server + agent-runtime + web + Postgres + Redis) — see the [usage guide](docs/USAGE.md) and [deployment guide](docs/DEPLOYMENT.md).

## What's in here

Each business module is its own crate, laid out hexagonally: `domain` / `ports` / `application` are pure logic with no IO by default, while the database and HTTP live in `adapters` behind feature flags. `tests/architecture.rs` scans the source and fails the build if a pure layer ever imports an IO crate like sqlx or axum — that keeps the layering from quietly eroding over time.

Working today: auth / RBAC / OIDC (Feishu, WeCom), projects, versioned requirements, the task DAG, the design approval gate, fleet dispatch and reclaim, human-AI delivery metrics (per-project and per-requirement AI share), MCP tools (`POST /mcp`), Skill orchestration, in-app notifications plus Feishu / DingTalk / WeCom webhook robots, WebSocket remote runners with resource pools (live run streaming, capability tags), and a test-management suite (cases / bugs / plans / API and scenario tests / Mock) — test plans get a planning mind-map, async case runs and plan-scoped bugs.

Not done yet: the verification gate is heavier to use than it should be; `shepherd-cli` is half-built; finer fleet metrics (e.g. claim-latency distribution) aren't there; and more executor backends (such as wiring OpenHands in as one) are on the list.

<details>
<summary>Full crate tree</summary>

```
crates/
  kernel/          shared kernel (pagination / PermissionSet)
  webauth/         shared auth primitives (AuthUser / SessionStore + axum extractors)
  system-setting/  users · orgs · roles · auth (local + OIDC)
  project/  requirement/  task/  orchestrator/    project · requirements (versioned) · task DAG · orchestration
  design/          design stage (OpenSpec/BMAD, human approval gate)
  delivery/        AI executor dispatch & delivery + fleet queue
  agent-runtime/   pure-Rust executor (CLI spawn + event streaming + worktree isolation)
  verification/    integrity verification + verification gate
  mcp/  skill/     MCP tools + Skill orchestration
  case/ bug/ test-plan/ api-test/ api-scenario/   test management
  api-definition/ api-runner/ runner/ probe/ perf/ comment/ follow/ environment/ mock-runtime/ …
  migrate/         versioned migrations (sqlx migrate! + uniqueness guard)
  server/          composition root = the single binary (typed config + domain-grouped routes)
  shepherd-cli/    CLI (still being built)
web/               React + antd frontend
```
</details>

<details>
<summary>Main environment variables</summary>

Server (consolidated into a typed `ServerConfig`):

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | local mstest | PG connection string |
| `SHEPHERD_BIND` | `0.0.0.0:8088` | main API listen address |
| `SHEPHERD_ADMIN_PASSWORD` | `admin` | admin password upserted idempotently on boot |
| `SHEPHERD_AGENT_FLEET` | — | set to enable fleet mode |
| `SHEPHERD_FLEET_REDIS` | — | set to use a Redis distributed queue / registry |
| `SHEPHERD_FEISHU_*` / `SHEPHERD_WECOM_*` | — | OIDC third-party login |

Executor `agent-runtime`:

| Variable | Default | Meaning |
|---|---|---|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | server address |
| `SHEPHERD_CAPS` | `CLAUDE_CODE` | comma-separated capabilities (which tasks to claim) |
| `AGENT_CONCURRENCY` | `1` | max concurrent tasks |
| `CLAUDE_BIN` / `CODEX_CMD` / `OPENCODE_CMD` | `claude` / `codex exec` / `opencode run` | each CLI invocation |
| `AGENT_MOCK` | — | set to use the mock backend (no real CLI) |
</details>

## Documentation

- **[Usage guide](docs/USAGE.md)** ([中文](docs/USAGE.zh-CN.md)) — concepts, quick start, the full configuration reference, the web console, fleet & executor setup, and the HTTP API.
- **[Deployment & ops](docs/DEPLOYMENT.md)** ([中文](docs/DEPLOYMENT.zh-CN.md)) — Docker Compose, Kubernetes via Helm (`deploy/helm/shepherd`), multi-cloud Terraform (`deploy/terraform/{aws,gcp,azure}`), and CI/CD auto-deploy.
- **[Install agent-runtime](docs/INSTALL.md)** — Homebrew, Windows (Scoop / PowerShell), Linux binary, and registering to a server.
- **[Running AI executors](docs/EXECUTORS.md)** ([中文](docs/EXECUTORS.zh-CN.md)) — Claude Code / Codex / OpenCode / CodeBuddy behind `agent-runtime`.

## How it compares

**vs. agent harnesses — Claude Code, Codex, OpenCode, OpenHands, Aider.** A harness wraps an LLM and runs the agentic loop (tools, context, turn by turn) to get one task done well. Shepherd is the layer *above* that: it doesn't run the loop — it decides what the tasks are, dispatches them, gates them on human approval, and verifies they were actually done. The harness is a swappable executor: `agent-runtime` literally shells out to `claude` / `codex` / `opencode` and streams their events back. OpenCode isn't a competitor — it's one of the executors you hang on Shepherd's fleet (`OPENCODE_CMD=opencode run`).

**vs. multi-agent frameworks — AutoGen, CrewAI.** They take the autonomy route: let agents loop until done, compete on benchmarks. Shepherd focuses on governance — approval and verification are steps you can't route around, not prompts you interrupt in a chat, and the deployment model is a central server plus internal executors that pull work outbound. Those frameworks *are* the agent; Shepherd is the supervisor above swappable agents.

**vs. personal agent orchestrators — Gas Town (and Beads).** Gas Town runs a fleet of coding agents on your own machine and keeps them busy around the clock: workers pick up whatever lands on their hook, a mayor coordinates across repos, merges get serialized. It's built to maximize one developer's throughput, and it's good at that. Shepherd tackles the same problem from the other end: it governs what a team ships — executors authenticate with API keys, every delivery attempt leaves a record, and the acceptance gate can't be routed around. The work ledgers are close cousins (beads live in git, tasks live in Postgres), so the two compose rather than compete: a Gas Town rig can hang off Shepherd's fleet as one more executor, and Shepherd supplies the acceptance and accounting layer a local factory doesn't have.

One concrete boundary: Shepherd speaks MCP as a *server* (it exposes `shepherd_*` tools so an agent can drive the requirement→verify lifecycle), while coding agents like OpenCode speak MCP as a *client*. They compose in both directions rather than overlap.

## Tests

```bash
cargo test --workspace                      # everything; non-integration runs in seconds
cargo test --workspace -- --ignored         # real-database integration tests
```

866 tests; the integration ones run against a real server + PG / Redis / MySQL. Besides the architecture guard, duplicate migration version numbers are rejected at startup and in CI (sqlx would otherwise silently drop one).

## Contributing

Issues and PRs welcome. A few conventions:

- Follow the hexagonal layering: business logic in `domain` / `application`, IO in `adapters`, and no IO crates in the pure layers (`tests/architecture.rs` will catch it).
- Include tests; `cargo test --workspace` and `cargo clippy --workspace` (`-D warnings`) need to be green.
- Enable the local quality gate once per clone: `git config core.hooksPath .githooks` — the pre-commit hook runs fmt/clippy/build (and web tsc/build) so CI failures surface before you push.
- New migrations use a unique, increasing version number: `NNNN_description.sql`.
- Write commit messages that explain *why*, not just what changed.

## License

GPL-2.0, see [LICENSE](LICENSE).

## Community

Questions or want to chat? Join us:

- **WeChat group — Shepherd Human-AI Collaboration (人机协同 交流群):** scan the QR code to join.

<img src="docs/assets/wechat-group.jpg" alt="Shepherd Human-AI Collaboration WeChat group" width="240" />
