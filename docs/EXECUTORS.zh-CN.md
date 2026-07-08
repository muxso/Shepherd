# Shepherd — AI 执行者运行指南

介绍如何在 `agent-runtime` 下运行各家 AI 执行者(Claude Code / Codex / OpenCode /
CodeBuddy)。机群架构与服务端配置见 [USAGE.zh-CN.md §7](./USAGE.zh-CN.md),
镜像构建与部署见 [DEPLOYMENT.zh-CN.md](./DEPLOYMENT.zh-CN.md)。

- [派发如何到达 CLI](#派发如何到达-cli)
- [runtime 通用配置](#runtime-通用配置)
- [Claude Code](#claude-code)
- [Codex](#codex)
- [OpenCode](#opencode)
- [CodeBuddy](#codebuddy)
- [一个 runtime 承接多种执行者](#一个-runtime-承接多种执行者)
- [mock 执行者(无需 CLI)](#mock-执行者无需-cli)
- [指定执行者派发](#指定执行者派发)
- [排障](#排障)

---

## 派发如何到达 CLI

每个交付尝试都带 `executor` 类型(`CLAUDE_CODE` / `CODEX` / `OPENCODE` /
`CODEBUDDY`)。服务端按类型入队;`agent-runtime` 以 `SHEPHERD_CAPS` 长轮询,
只认领自己声明过的类型。认领后 runtime 在独立 git worktree 中拉起对应 CLI,
把产生的改动快照成 commit 并回报。

能力隔离是严格的:`SHEPHERD_CAPS=CODEBUDDY` 的 runtime 永远拿不到
`CLAUDE_CODE` 的任务,反之亦然。各类型积压可看 `GET /agent/work/stats`。

## runtime 通用配置

运行 `agent-runtime` 的机器需要 `git` 和对应 CLI 在 `PATH` 上,且 CLI 已完成
登录(runtime 不管理 CLI 的认证)。

```bash
SHEPHERD_BASE=http://<server>:8088 \
SHEPHERD_ADMIN_PASSWORD=… \
SHEPHERD_CAPS=<KIND[,KIND…]> \
RUNTIME_NAME=$(hostname) \
AGENT_WORKDIR=/path/to/target/repo \
./agent-runtime
```

| 环境变量 | 默认 | 含义 |
|---|---|---|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | 服务端地址(仅出站,不需要入站端口) |
| `SHEPHERD_ADMIN_USER` / `SHEPHERD_ADMIN_PASSWORD` | `admin` / `s3cret` | 注册与认领用的登录凭据 |
| `SHEPHERD_CAPS` | `CLAUDE_CODE` | 本 runtime 认领的执行者类型,逗号分隔 |
| `RUNTIME_NAME` | `agent-runtime` | 机群注册表中的显示名 |
| `AGENT_WORKDIR` | `.` | 任务操作的 git 仓库 |
| `AGENT_CONCURRENCY` | `1` | 最大并发任务数 |
| `AGENT_TASK_TIMEOUT_SECS` | `1800` | 单任务 CLI 超时 |

## Claude Code

唯一的流式后端:跑 `claude -p --output-format stream-json`,把工具调用事件
实时解析进交付决策日志,并带 `--permission-mode acceptEdits`。

```bash
# CLI 在且已登录?
claude --version

SHEPHERD_CAPS=CLAUDE_CODE AGENT_WORKDIR=/repo … ./agent-runtime
```

二进制不在 `PATH` 时用 `CLAUDE_BIN=/opt/claude/bin/claude` 指定。

## Codex

通用(非流式)后端:每任务一次 CLI 调用,stdout 作为结果摘要。

```bash
codex --version

SHEPHERD_CAPS=CODEX AGENT_WORKDIR=/repo … ./agent-runtime
```

默认调用 `codex exec "<prompt>"`;整条命令可用 `CODEX_CMD` 覆盖
(如 `CODEX_CMD="codex exec --full-auto"`)。

## OpenCode

通用后端,形态同 Codex。

```bash
opencode --version

SHEPHERD_CAPS=OPENCODE AGENT_WORKDIR=/repo … ./agent-runtime
```

默认调用 `opencode run "<prompt>"`;用 `OPENCODE_CMD` 覆盖。

## CodeBuddy

通用后端。默认调用
`codebuddy -p --permission-mode acceptEdits "<prompt>"` —— 权限参数是关键:
纯 `-p` 打印模式下 CodeBuddy 会拒绝改文件(Write/Bash 工具等待一个 headless
下永远不会来的审批),导致交付"无代码变动"。

```bash
codebuddy --version

SHEPHERD_CAPS=CODEBUDDY AGENT_WORKDIR=/repo … ./agent-runtime
```

用 `CODEBUDDY_CMD` 覆盖,例如任务需要跑 shell 时放宽权限:
`CODEBUDDY_CMD="codebuddy -p --permission-mode bypassPermissions"`。

## 一个 runtime 承接多种执行者

单个 runtime 可以声明多种类型——全部列出,并保证对应 CLI 都装好且已登录:

```bash
SHEPHERD_CAPS=CLAUDE_CODE,CODEBUDDY … ./agent-runtime
```

每台机器(一套 CLI 登录态)跑一个 runtime;横向扩展就是对同一服务端多起几个
runtime(能力任意组合)。服务端设了 `SHEPHERD_FLEET_REDIS` 时,跨机 runtime
共享同一个队列。

## mock 执行者(无需 CLI)

`AGENT_MOCK=1` 让 runtime 认领其声明的任意类型并返回固定输出,不拉起真实
CLI——适合在装 CLI 之前先冒烟验证派发链路。

```bash
AGENT_MOCK=1 SHEPHERD_CAPS=CLAUDE_CODE,CODEX,OPENCODE,CODEBUDDY … ./agent-runtime
```

## 指定执行者派发

执行者在派发时按任务指定,不是服务端全局配置:

```bash
# HTTP
curl -X POST $BASE/delivery -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"decompositionId":"d1","taskId":"t1","title":"…","executor":"CODEBUDDY"}'

# CLI
shepherd dispatch --decomp d1 --task t1 --executor CODEBUDDY
# 或先绑定默认:shepherd agent connect --kind codebuddy
```

MCP 工具 `shepherd_dispatch_delivery` 同样接受这四种类型。

## 排障

| 现象 | 排查 |
|---|---|
| 任务一直排队(`ready` 增长) | 没有在线 runtime 声明该类型——核对 `SHEPHERD_CAPS` 与任务 `executor`,再看 `GET /agent/runtime` 的在线状态 |
| 交付显示"无代码变动" | CLI 跑了但拒绝改文件(权限模式),或提示词本身没要求改文件——看交付摘要,里面是 CLI 的原始输出 |
| 事件里报 spawn 错误 | CLI 不在 runtime 进程的 `PATH` 上——设 `CLAUDE_BIN` / `CODEX_CMD` / `OPENCODE_CMD` / `CODEBUDDY_CMD` |
| 任务超时失败 | 调大 `AGENT_TASK_TIMEOUT_SECS`;通用后端到点会被整进程组杀掉 |
