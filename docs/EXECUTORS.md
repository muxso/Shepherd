# Shepherd — Running AI Executors

How to run each supported AI executor (Claude Code / Codex / OpenCode / CodeBuddy)
behind `agent-runtime`. For fleet architecture and server-side setup see
[USAGE.md §7](./USAGE.md); for image builds and deployment see [DEPLOYMENT.md](./DEPLOYMENT.md).

- [How dispatch reaches a CLI](#how-dispatch-reaches-a-cli)
- [Common runtime configuration](#common-runtime-configuration)
- [Claude Code](#claude-code)
- [Codex](#codex)
- [OpenCode](#opencode)
- [CodeBuddy](#codebuddy)
- [One runtime, multiple executors](#one-runtime-multiple-executors)
- [Mock executor (no CLI)](#mock-executor-no-cli)
- [Windows](#windows)
- [Docker and host CLIs](#docker-and-host-clis)
- [Shared checkout, different branches](#shared-checkout-different-branches)
- [Dispatching to a specific executor](#dispatching-to-a-specific-executor)
- [Troubleshooting](#troubleshooting)

---

## How dispatch reaches a CLI

Every delivery attempt carries an `executor` kind (`CLAUDE_CODE` / `CODEX` /
`OPENCODE` / `CODEBUDDY`). The server enqueues the task per kind; an
`agent-runtime` long-polls with its `SHEPHERD_CAPS` and only claims kinds it
declared. On claim, the runtime spawns the matching CLI in a dedicated git
worktree, snapshots the resulting changes as a commit, and reports back.

Capability isolation is strict: a runtime with `SHEPHERD_CAPS=CODEBUDDY` never
receives `CLAUDE_CODE` work, and vice versa. Backlog per kind is visible at
`GET /agent/work/stats`.

## Common runtime configuration

The machine running `agent-runtime` needs `git` plus the CLI(s) on `PATH`,
each already logged in (the runtime does not manage CLI auth).

```bash
SHEPHERD_BASE=http://<server>:8088 \
SHEPHERD_ADMIN_PASSWORD=… \
SHEPHERD_CAPS=<KIND[,KIND…]> \
RUNTIME_NAME=$(hostname) \
AGENT_WORKDIR=/path/to/target/repo \
./agent-runtime
```

| Env | Default | Meaning |
|---|---|---|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | Server base URL (outbound only; no inbound port needed) |
| `SHEPHERD_ADMIN_USER` / `SHEPHERD_ADMIN_PASSWORD` | `admin` / `s3cret` | Login used to register and claim |
| `SHEPHERD_CAPS` | `CLAUDE_CODE` | Comma-separated executor kinds this runtime claims |
| `RUNTIME_NAME` | `agent-runtime` | Display name in the fleet registry |
| `AGENT_WORKDIR` | `.` | Git repo the tasks operate on |
| `AGENT_BASE_REF` | *(repo HEAD)* | Git ref tasks branch from (e.g. `origin/main`) |
| `AGENT_CONCURRENCY` | `1` | Max tasks in flight |
| `AGENT_TASK_TIMEOUT_SECS` | `1800` | Per-task CLI timeout |

## Claude Code

The only streaming backend: runs `claude -p --output-format stream-json`,
parses tool-use events into the delivery decision log as they happen, and runs
with `--permission-mode acceptEdits`.

```bash
# CLI present and logged in?
claude --version

SHEPHERD_CAPS=CLAUDE_CODE AGENT_WORKDIR=/repo … ./agent-runtime
```

Override the binary path with `CLAUDE_BIN=/opt/claude/bin/claude` if it is not
on `PATH`.

## Codex

Generic (non-streaming) backend: one CLI invocation per task, stdout captured
as the result summary.

```bash
codex --version

SHEPHERD_CAPS=CODEX AGENT_WORKDIR=/repo … ./agent-runtime
```

Default invocation is `codex exec "<prompt>"`; override the whole command with
`CODEX_CMD` (e.g. `CODEX_CMD="codex exec --full-auto"`).

## OpenCode

Generic backend, same shape as Codex.

```bash
opencode --version

SHEPHERD_CAPS=OPENCODE AGENT_WORKDIR=/repo … ./agent-runtime
```

Default invocation is `opencode run "<prompt>"`; override with `OPENCODE_CMD`.

## CodeBuddy

Generic backend. The default invocation is
`codebuddy -p --permission-mode acceptEdits "<prompt>"` — the permission flag
matters: in plain `-p` print mode CodeBuddy refuses file edits (its Write/Bash
tools wait for an approval that never comes headless) and the attempt delivers
with no code change.

```bash
codebuddy --version

SHEPHERD_CAPS=CODEBUDDY AGENT_WORKDIR=/repo … ./agent-runtime
```

Override with `CODEBUDDY_CMD`, e.g. widen permissions for tasks that need to
run shell commands: `CODEBUDDY_CMD="codebuddy -p --permission-mode bypassPermissions"`.

## One runtime, multiple executors

A single runtime can claim several kinds — list them all and make sure every
corresponding CLI is installed and logged in:

```bash
SHEPHERD_CAPS=CLAUDE_CODE,CODEBUDDY … ./agent-runtime
```

Run one runtime per machine seat; scale out by starting more runtimes (any
mix of capabilities) against the same server. With
`SHEPHERD_FLEET_REDIS` set on the server, runtimes on different hosts share
one queue.

## Mock executor (no CLI)

`AGENT_MOCK=1` makes the runtime claim any of its declared kinds and return
canned output without spawning a CLI — useful to smoke-test the dispatch loop
before installing real CLIs.

```bash
AGENT_MOCK=1 SHEPHERD_CAPS=CLAUDE_CODE,CODEX,OPENCODE,CODEBUDDY … ./agent-runtime
```

## Windows

The runtime runs natively on Windows (build with the MSVC toolchain:
`cargo build --release -p agent-runtime`). Two platform notes:

- npm-installed CLIs (`claude` / `codebuddy` / `opencode`) are `<name>.cmd`
  shims on Windows. The runtime resolves a bare program name to the shim
  automatically when no `<name>.exe` is found on `PATH` (native binaries like
  `codex.exe` take precedence); explicit paths via `CLAUDE_BIN` / `*_CMD` are
  used as-is.
- On task timeout only the direct CLI process is terminated — Windows has no
  process groups, so children the CLI spawned may linger.

## Docker and host CLIs

A Linux container **cannot execute the host's Windows (or macOS) CLI
binaries** — mounting `claude.cmd` / `codebuddy.exe` into the container does
not work. The workable pattern is the reverse split:

- bake the **Linux** CLIs into the image, and
- mount only the host's CLI **credential/config directories**, so logins
  survive image rebuilds.

```dockerfile
FROM shepherd-agent-runtime
USER root
RUN apt-get update && apt-get install -y --no-install-recommends nodejs npm \
 && npm install -g @anthropic-ai/claude-code @tencent-ai/codebuddy-code \
 && rm -rf /var/lib/apt/lists/*
USER shepherd
```

```yaml
  agent-runtime:
    volumes:
      - ~/.claude:/home/shepherd/.claude          # CLI login state
      - ~/.codebuddy:/home/shepherd/.codebuddy
      - /path/to/repo:/work                       # AGENT_WORKDIR=/work
```

This works the same from a Windows host — Docker Desktop mounts Windows paths
(`C:\Users\me\.claude`) into Linux containers; it is the CLI *binary* that
must be the Linux build, not the config.

## Shared checkout, different branches

Each task runs in a **detached git worktree**, so the base repo's checked-out
branch is never switched and its working tree is never touched — a runtime
(native or containerized) can safely share the same clone a developer works
in, even on a different branch. Two things to know:

- By default the task base is whatever `HEAD` the shared clone currently has
  checked out — it follows the developer's branch switches. Pin
  `AGENT_BASE_REF=origin/main` (or a branch/tag/SHA) to make task bases
  deterministic. Remote-tracking refs resolve to their last-fetched position;
  fetch to move them.
- Worktrees live under the runtime's temp dir. When the runtime is a
  container, the recorded paths are container paths, so `git worktree list`
  on the host may show stale entries — harmless; the runtime prunes them
  before each task.

## Dispatching to a specific executor

Pick the executor at dispatch time — per task, not per server:

```bash
# HTTP
curl -X POST $BASE/delivery -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"decompositionId":"d1","taskId":"t1","title":"…","executor":"CODEBUDDY"}'

# CLI
shepherd dispatch --decomp d1 --task t1 --executor CODEBUDDY
# or bind a default once: shepherd agent connect --kind codebuddy
```

The MCP tool `shepherd_dispatch_delivery` accepts the same four kinds.

## Troubleshooting

| Symptom | Check |
|---|---|
| Task stays queued (`ready` grows) | No online runtime declares that kind — compare `SHEPHERD_CAPS` with the task's `executor`, then `GET /agent/runtime` for liveness |
| Attempt delivers with "no code change" | CLI ran but refused edits (permission mode) or the prompt didn't ask for file changes — read the deliverable summary, it contains the CLI's own output |
| Spawn error in events | CLI not on `PATH` inside the runtime's environment — set `CLAUDE_BIN` / `CODEX_CMD` / `OPENCODE_CMD` / `CODEBUDDY_CMD` |
| Attempt fails at timeout | Raise `AGENT_TASK_TIMEOUT_SECS`; generic backends get killed (process group) at the deadline |
