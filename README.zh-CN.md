# Shepherd 🐑

让 AI 写代码,你来把关。

[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-GPL--2.0-blue.svg)](LICENSE)
[![status](https://img.shields.io/badge/status-experimental-yellow.svg)](#状态)
&nbsp; · &nbsp; 简体中文 | [English](README.md)

Shepherd 是一个还在早期阶段的研发监督平台。出发点不复杂:AI 现在能写代码了,但它不会判断自己有没有真正做完需求,也不会替结果负责。与其再造一个"更聪明的 agent",Shepherd 做的是 agent 外面那一层——把需求拆给 AI 执行机去干,然后在设计和验证两个环节卡上人工审批,过程留痕。

> **状态**:`v0.0.1`,实验性质,我们自己在用(dogfood)。完整闭环能跑通,但离生产可用还有距离,也没有公开 benchmark。别拿它当成熟工具。

<!-- 演示动图待补:见 docs/assets/。放好后取消下一行注释。 -->
<!-- ![演示](docs/assets/demo.gif) -->

## 它怎么运作

一条需求大致这么走:

```
你提需求 → AI 起草设计稿 →(你审批)→ 拆成任务 DAG → 内网执行机认领、开干
                                                              │
你签收 ←(验证门)← 裁决 ← 交付物(diff / PR) ←───────────────┘
```

设计门和验证门是流程里硬性的两步,不是可跳过的提示。AI 跑得再快,东西也得过了门才进主干。这基本就是 Shepherd 想解决的问题:让你能放心地让 AI 大规模干活,因为产出是可审、可追责的。

## 执行机为什么是"出站拉取"

这块是整个项目里我自己觉得比较有意思的设计。

公司里的 AI(Claude Code、Codex 这些)通常跑在内网开发机或 CI 上,这些机器没有公网入口;而服务端有公网。所以服务端没法主动把任务推给执行机,只能反过来——**执行机主动出站,长轮询来认领任务**。很多编排框架默认能反向触达 agent,落到真实内网就卡在这一步。

- 单机:进程内队列就够了,零外部依赖。
- 多机:`SHEPHERD_FLEET_REDIS` 切到 Redis Streams 消费组,保证一个任务只被一台认领、跑完 ack、执行机挂了超时回收。

执行机本身(`agent-runtime`)是纯 Rust:并发用信号量控,退出时排空在途任务,每个任务在独立的 git worktree 里跑互不干扰。`GET /agent/work/stats` 能看到各能力的积压、在飞、最久滞留。想加一种新执行机,实现一个 `CliAgentBackend`(就一个 `execute(prompt, cwd, sink)` 方法)再注册个枚举就行。

```
        (公网)                              (内网,无入站)
 ┌────────────┐  入队    ┌──────────────┐  ←认领─  ┌──────────────────┐
 │  Shepherd  │─────────▶│  WorkQueue   │          │  agent-runtime   │
 │   服务端   │          │ 内存 / Redis │  ─spec→  │  起 claude /     │
 │ (派发/门)  │◀─────────│  Streams 组  │          │  codex / opencode│
 └────────────┘  回调收尾 └──────────────┘ 注册/心跳 └──────────────────┘
   /delivery/{id}/…                                 每任务 git worktree 隔离
```

## 跑起来

```bash
# 1) 一个 Postgres,迁移会在启动时自动施加
docker run -d --name shep-pg \
  -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass -e POSTGRES_DB=mstest \
  -p 55432:5432 postgres:16-alpine

# 2) 起服务(根目录设了 default-members,直接 cargo run)
DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
SHEPHERD_ADMIN_PASSWORD=s3cret \
cargo run                       # → http://localhost:8088
```

```bash
# 登录拿令牌
curl -s localhost:8088/auth/login -H 'content-type: application/json' \
  -d '{"username":"admin","password":"s3cret"}'
```

前端控制台,以及在内网机上起一台执行机:

```bash
cd web && npm install && npm run dev          # Vite 控制台

SHEPHERD_AGENT_FLEET=1 cargo run              # 服务端开机群模式
SHEPHERD_BASE=http://<server>:8088 SHEPHERD_CAPS=CLAUDE_CODE cargo run -p agent-runtime
```

<!-- 控制台截图待补:见 docs/assets/。 -->

想一条命令起全栈?`docker compose -f deploy/docker/docker-compose.yml up --build` 会拉起整套服务(server + agent-runtime + web + Postgres + Redis)——详见[使用指南](docs/USAGE.zh-CN.md)与[部署指南](docs/DEPLOYMENT.zh-CN.md)。

## 里面有什么

每个业务模块是一个独立 crate,按六边形分层:`domain` / `ports` / `application` 是纯逻辑、默认不碰 IO,数据库和 HTTP 都在 `adapters` 里用 feature 开关。`tests/architecture.rs` 会扫源码,纯层一旦引了 sqlx / axum 这类 IO crate 就让构建挂掉——免得分层写着写着被写穿。

已经能用的:鉴权 / RBAC / OIDC(飞书、企业微信)、项目、多版本需求、任务 DAG、设计审批门、机群派发与回收、MCP 工具(`POST /mcp`)、Skill 编排,还有一套测试管理(用例 / 缺陷 / 计划 / 接口与场景测试 / Mock)。

还没做好的:验证门现在用起来偏重,想让它更省事;`shepherd-cli` 还在搭;更细的机群指标(比如认领延迟分布)还没加;更多执行机后端(比如把 OpenHands 接成一台)也在清单上。

<details>
<summary>完整 crate 树</summary>

```
crates/
  kernel/          共享内核(分页 / 权限 PermissionSet)
  webauth/         共享鉴权基元(AuthUser / SessionStore + axum 提取器)
  system-setting/  用户·组织·角色·鉴权(本地 + OIDC)
  project/  requirement/  task/  orchestrator/    项目 · 需求(多版本) · 任务 DAG · 编排
  design/          设计阶段(OpenSpec/BMAD,人审批门)
  delivery/        AI 执行者派发与交付 + 机群队列
  agent-runtime/   纯 Rust 执行机(起 CLI + 事件回流 + worktree 隔离)
  verification/    完整性验证 + 验证门
  mcp/  skill/     MCP 工具 + Skill 编排
  case/ bug/ test-plan/ api-test/ api-scenario/   测试管理
  api-definition/ api-runner/ runner/ probe/ perf/ comment/ follow/ environment/ mock-runtime/ …
  migrate/         版本化迁移(sqlx migrate! + 唯一性守卫)
  server/          组装根 = 唯一 bin 入口(typed 配置 + 领域分组路由)
  shepherd-cli/    CLI(还在搭)
web/               React + antd 前端
```
</details>

<details>
<summary>主要环境变量</summary>

服务端(收拢在 typed `ServerConfig`):

| 变量 | 默认 | 含义 |
|---|---|---|
| `DATABASE_URL` | 本地 mstest | PG 连接串 |
| `SHEPHERD_BIND` | `0.0.0.0:8088` | 主 API 监听 |
| `SHEPHERD_ADMIN_PASSWORD` | `admin` | 启动时幂等 upsert 的 admin 密码 |
| `SHEPHERD_AGENT_FLEET` | — | 设置即开机群模式 |
| `SHEPHERD_FLEET_REDIS` | — | 设置即用 Redis 分布式队列 / 注册表 |
| `SHEPHERD_FEISHU_*` / `SHEPHERD_WECOM_*` | — | OIDC 第三方登录 |

执行机 `agent-runtime`:

| 变量 | 默认 | 含义 |
|---|---|---|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | 服务端地址 |
| `SHEPHERD_CAPS` | `CLAUDE_CODE` | 逗号分隔的能力(认领哪类任务) |
| `AGENT_CONCURRENCY` | `1` | 并发任务上限 |
| `CLAUDE_BIN` / `CODEX_CMD` / `OPENCODE_CMD` | `claude` / `codex exec` / `opencode run` | 各 CLI 调用 |
| `AGENT_MOCK` | — | 设置即用 mock 后端(免真实 CLI) |
</details>

## 文档

- **[使用指南](docs/USAGE.zh-CN.md)**([English](docs/USAGE.md))—— 概念、快速上手、完整配置参考、Web 控制台、机群与执行机配置、HTTP API。
- **[部署与运维](docs/DEPLOYMENT.zh-CN.md)**([English](docs/DEPLOYMENT.md))—— Docker Compose、Kubernetes(Helm,`deploy/helm/shepherd`)、多云 Terraform(`deploy/terraform/{aws,gcp,azure}`)、CI/CD 自动部署。
- **[架构](ARCHITECTURE.md)** · **[路线图](ROADMAP.md)** · **[机群设计笔记](docs/remote-agent-runtime-plan.md)**

## 和别的方案比

如果你在看 AutoGen、CrewAI、OpenHands 这类:它们大多走"自治"路线,让 agent 自己循环到完成,主要拼 benchmark。Shepherd 的重点在治理——人工审批和验证是流程里绕不过去的节点,而不是聊天里随手打断;部署上也是按"中心服务端 + 内网执行机出站拉取"来设计的。

还有个本质区别:那些框架本身就是 agent;Shepherd 不是,它是 agent 上面的监工,底下挂什么 agent 可以换。所以像 OpenHands,对 Shepherd 来说更像是"又一台能挂上来的执行机",而不是竞品。

## 测试

```bash
cargo test --workspace                      # 全量,非集成秒级
cargo test --workspace -- --ignored         # 真库集成测试
```

866 个测试,集成测试连真实 server + PG / Redis / MySQL。除架构守卫外,还有一条迁移唯一性守卫:迁移版本号重号会让 CI 挂(sqlx 对重号会静默丢迁移,我们踩过一次,缺列导致 500)。

更多背景见 [ARCHITECTURE.md](ARCHITECTURE.md)、[ROADMAP.md](ROADMAP.md)、[机群设计](docs/remote-agent-runtime-plan.md)。

## 贡献

欢迎 Issue 和 PR。几条约定:

- 遵循六边形分层:业务逻辑进 `domain` / `application`,IO 进 `adapters`,纯层别引 IO crate(`tests/architecture.rs` 会拦)。
- 带测试;`cargo test --workspace` 和 `cargo clippy --workspace`(`-D warnings`)要全绿。
- 新增迁移用唯一递增的版本号:`NNNN_描述.sql`。
- Commit 说清楚为什么改,不只是改了什么。

## 许可证

GPL-2.0,见 [LICENSE](LICENSE)。
