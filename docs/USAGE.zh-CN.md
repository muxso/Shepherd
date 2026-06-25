# Shepherd — 使用指南

> 适用对象:运行、运维或评估 Shepherd 的人。生产部署(Helm、Terraform、CI/CD、Day-2 运维)见 [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)。English version: [USAGE.md](USAGE.md)。

Shepherd 是一个 AI 研发**监督**平台:AI 写代码,你掌控交付什么。它把需求拆给 AI 执行者去做,并在**两个环节设人工审批门**(设计、验证),把整条链路完整记录在案。

---

## 1. 总览与核心概念

需求在一条固定流水线上流转。两道门是硬步骤——无论 AI 跑多快,人没放行,任何东西都进不了你的主干。

```
你提交一条需求
        │
        ▼
   AI 起草设计  ──►  ⛔ 设计审批门(人工)
        │                  通过 ▼
        │             拆分为任务 DAG
        │                  │ 派发
        ▼                  ▼
                    机群派发 ──► agent-runtime 执行者
                   (拉取 / 长轮询)   (claude / codex / opencode)
                          │ 在独立 git worktree 中运行
                          ▼
                    交付物(diff / PR)
                          │ 裁决
                          ▼
                    ⛔ 验证门(judge + 人工)
                          │ 通过
                          ▼
                    签收(需求满足)
```

| 概念 | 含义 |
|---|---|
| **需求(Requirement)** | 版本化的工作单元(不可变、只追加的版本快照)。 |
| **AI 设计稿** | 由「架构师角色」执行者据需求规格起草设计提案。 |
| **设计审批门** | 人工审阅并通过/驳回设计稿。通过即自动触发任务拆分;未放行则不拆分。 |
| **任务 DAG** | 把需求拆成带依赖与就绪门控的有向无环图。 |
| **机群派发** | 任务入队后,**执行者出站长轮询主动认领**——服务端从不主动推送(见下)。 |
| **交付物** | 执行者产出 diff / PR 引用与变更摘要。 |
| **验证门** | judge(及人工)按验收标准裁决交付物。通过 → 任务 `Verified`,解锁下游任务。 |
| **签收** | 覆盖链满足后,需求签收。 |

### 为什么机群是「拉」而非「被推」

公司里的 AI 工具(Claude Code、Codex……)跑在内网开发机或 CI 上——**无公网入站**。Shepherd 服务端有公网地址。所以服务端无法把任务推给执行者,只能反过来:**执行者出站长轮询主动认领任务**(`GET /agent/work/claim`),通过回调上报进度,并以心跳维持注册。

- **单机** —— 进程内队列即可,无外部依赖。
- **多机** —— 设 `SHEPHERD_FLEET_REDIS` 切到 Redis Streams 消费者组:精确一次认领、终态 ack、执行者死亡后按超时回收重投。

---

## 2. 前置条件

| 用途 | 所需 |
|---|---|
| 快速开始(Docker) | Docker + Docker Compose v2 |
| 源码开发 | Rust(stable,edition 2021;CI 用 `rust:1.86`)、Node.js 18+、一个 PostgreSQL 16 实例 |
| 多机机群 | Redis 7 |
| 真实 AI 执行者 | `git` + agent CLI 在 `PATH` 上(`claude` / `codex` / `opencode`) |

PostgreSQL 必需;服务端**启动时自动跑迁移**。Redis 仅在多机机群时必需。

---

## 3. 快速开始(Docker Compose)

单机看完整链路最快的方式。该编排会拉起 Postgres、Redis、服务端(机群模式)、一台 mock agent-runtime,以及 nginx 后的 web 控制台。

```bash
docker compose -f deploy/docker/docker-compose.yml up --build
```

随后打开:

| URL | 内容 |
|---|---|
| http://localhost:8080 | Web 控制台(nginx 托管的 SPA) |
| http://localhost:8088 | 服务端 API |

**登录**:`admin` / compose 文件里的 `SHEPHERD_ADMIN_PASSWORD`(默认 `change-me-please`——本地试跑之外务必更改)。

说明:
- 内置 `agent-runtime` 以 `AGENT_MOCK=1` 运行,不调真实 CLI 即可认领并「完成」任务——适合冒烟。要跑真实后端,请在装有 CLI 的机器上用源码起一台 agent-runtime(见 §5)。
- 这**不是**生产部署(密钥内联、PG/Redis 本地)。生产请用 Helm/Terraform —— 见 [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)。

拆除(并清空数据库卷):

```bash
docker compose -f deploy/docker/docker-compose.yml down -v
```

---

## 4. 源码开发运行

三个进程:Postgres、服务端、Vite dev 控制台。要真实(或 mock)派发时再加一台 agent-runtime。

### 4.1 Postgres

```bash
docker run -d --name shep-pg \
  -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass -e POSTGRES_DB=mstest \
  -p 55432:5432 postgres:16-alpine
```

### 4.2 服务端

workspace 根设了 `default-members`,故直接 `cargo run` 即可构建并运行 `server` bin。迁移在启动时自动应用。

```bash
DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
SHEPHERD_ADMIN_PASSWORD=s3cret \
cargo run -p server          # → http://localhost:8088
```

> Vite dev 代理默认指向 `http://127.0.0.1:9180`。要对齐,把服务端绑到那里:
> `SHEPHERD_BIND=127.0.0.1:9180 … cargo run -p server`(或用 `SHEPHERD_API=http://127.0.0.1:8088` 让 Vite 指向 :8088)。

取令牌:

```bash
curl -s localhost:8088/auth/login -H 'content-type: application/json' \
  -d '{"username":"admin","password":"s3cret"}'
```

### 4.3 Web 控制台(Vite dev)

```bash
cd web && npm install && npm run dev      # → http://localhost:5173
```

dev 服务器(5173 端口)把所有后端前缀代理到服务端,并按 `Accept` 头区分 SPA 整页导航与 API 调用。

### 4.4 Agent-runtime(执行者)

跑在装有 agent CLI 的机器上(或任意机器配 `AGENT_MOCK=1`)。需要服务端处于**机群模式**:

```bash
# 服务端,机群模式(单机进程内队列)
SHEPHERD_AGENT_FLEET=1 DATABASE_URL=… SHEPHERD_ADMIN_PASSWORD=s3cret cargo run -p server

# 执行者:出站长轮询,认领 CLAUDE_CODE 任务
SHEPHERD_BASE=http://<server>:8088 \
SHEPHERD_ADMIN_PASSWORD=s3cret \
SHEPHERD_CAPS=CLAUDE_CODE \
cargo run -p agent-runtime
```

不接真实 CLI 演示时,加 `AGENT_MOCK=1`。

### 4.5 测试

```bash
cargo test --workspace                 # 全部;非集成测试数秒跑完
cargo test --workspace -- --ignored    # 真库集成测试
cargo clippy --workspace -- -D warnings
```

---

## 5. 配置(环境变量)

服务端所有启动开关收拢为一份 typed 配置(`crates/server/src/config.rs`)。

### 5.1 服务端(`server`)

| 变量 | 默认 | 含义 |
|---|---|---|
| `DATABASE_URL` | `postgres://msuser:mspass@localhost:55432/mstest` | PostgreSQL 连接串(生产**必填**) |
| `SHEPHERD_BIND` | `0.0.0.0:8088` | 主 API 监听地址 |
| `SHEPHERD_ADMIN_PASSWORD` | `admin` | 启动时幂等 upsert 的 admin 密码。**生产必须覆盖。** |
| `SHEPHERD_SESSION_TTL_SECS` | `28800`(8 小时) | 会话令牌有效期 |
| `SHEPHERD_AGENT_FLEET` | — | 存在即启用**机群模式**(派发入队;runtime 出站认领) |
| `SHEPHERD_FLEET_REDIS` | — | 分布式队列/注册表的 Redis URL(多机)。不设 → 进程内(单机) |
| `SHEPHERD_FLEET_REAP_INTERVAL_S` | `15` | 回收器轮询间隔——重投死 runtime 的待处理任务 |
| `SHEPHERD_FLEET_RECLAIM_MS` | `30000` | 判定死 runtime 在飞任务可回收前的容忍期 |
| `SHEPHERD_EXECUTOR_URL` | — | 接口批量运行的远端 JMeter 派发地址(可选) |
| `SHEPHERD_RUNNER` | — | `noop` → 本地无 API runner(批量运行恒 `RUNNING`,仅演示) |
| `SHEPHERD_FEISHU_APP_ID` / `_APP_SECRET` / `_REDIRECT` | — | 飞书 OIDC 登录(id+secret 都配齐才注册 provider) |
| `SHEPHERD_WECOM_CORP_ID` / `_SECRET` / `_REDIRECT` | — | 企业微信 OIDC 登录 |
| `MOCK_BIND` | — | Mock 服务的独立监听地址(可选) |

可插拔 AI 触点还有若干进阶/懒读开关:`SHEPHERD_AGENT_URL` / `SHEPHERD_AGENT_CMD` / `SHEPHERD_AGENT_ASYNC`(执行者路由)、`SHEPHERD_LLM_URL`、`SHEPHERD_PLANNER_URL`、`SHEPHERD_JUDGE_URL`、`SHEPHERD_MAX_REVISIONS`。默认全都无需设置。

### 5.2 Agent-runtime(`agent-runtime`)

| 变量 | 默认 | 含义 |
|---|---|---|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | 长轮询的服务端地址 |
| `SHEPHERD_ADMIN_USER` | `admin` | 登录用户 |
| `SHEPHERD_ADMIN_PASSWORD` | `s3cret` | 登录密码 |
| `SHEPHERD_CAPS` | `CLAUDE_CODE` | 逗号分隔能力——本 runtime 认领哪类任务(如 `CLAUDE_CODE,CODEX`) |
| `AGENT_CONCURRENCY` | `1` | 最大在飞并发(信号量上限) |
| `AGENT_WORKDIR` | `.` | 基础工作目录;每个任务在其下的独立 git worktree 里运行 |
| `RUNTIME_NAME` | `agent-runtime` | 机群注册表中的显示名 |
| `AGENT_MOCK` | — | 存在 → mock 后端(无真实 CLI) |
| `CLAUDE_BIN` | `claude` | Claude CLI 可执行文件 |
| `CODEX_CMD` | `codex exec` | Codex CLI 调用 |
| `OPENCODE_CMD` | `opencode run` | OpenCode CLI 调用 |

---

## 6. Web UI 导览

在控制台登录(`admin` / 你的密码)。页面与上文流水线一一对应:

| 页面 | 你在这里做什么 |
|---|---|
| **Login(登录)** | 本地登录,或(已配置时)OIDC(飞书 / 企业微信)。 |
| **Home(首页)** | 落地仪表盘——接口用例执行汇总。 |
| **Org & Projects(组织与项目)** | 管理组织与项目(下面一切的顶层容器)。 |
| **Project Admin(项目管理)** | 项目级设置。 |
| **Requirements(需求)** | 提交并版本化需求;触发拆分为任务 DAG。 |
| **Task Center(任务中心)** | 任务 DAG——依赖、就绪、派发与交付状态。 |
| **Agents(机群)** | 机群视图——已注册 runtime、各能力积压/在飞(底层 `GET /agent/work/stats`)。 |
| **Skills(技能)** | 定义并组合塑造执行者行为的 AI Skill。 |
| **MCP** | 查看/驱动 MCP 工具面。 |
| **API Definitions / Scenarios / Environments(接口定义/场景/环境)** | 接口测试管理——定义、链式场景、环境。 |
| **Functional Cases / Cases / Review(功能用例/用例/评审)** | 功能测试用例、用例面板、评审。 |
| **Test Plans(测试计划)** | 测试计划与执行。 |
| **Bugs(缺陷)** | 缺陷跟踪。 |
| **Resource Pool / Runner Agents / Perf / Mocks(资源池/原生 runner/压测/Mock)** | 资源池、原生 runner、性能运行、mock runtime。 |
| **Users / User Groups(用户/用户组)** | 用户、角色、RBAC。 |
| **Message Settings(消息设置)** | 通知/消息配置。 |
| **File Management(文件管理)** | 项目文件管理。 |

审批与验证两道门嵌在 Requirements / Task 流程中——设计稿在拆分前审阅,交付物在签收前裁决。

---

## 7. 机群与 agent-runtime 配置

`agent-runtime` 是执行者:并发受限(信号量)、关停时排空在飞任务,且每个任务跑在自己的 git worktree 里,并发任务互不干扰。

### 7.1 单机 vs 多机

| 模式 | 服务端开关 | 队列 |
|---|---|---|
| **单机** | `SHEPHERD_AGENT_FLEET=1` | 进程内内存队列(无外部依赖) |
| **多机** | `SHEPHERD_AGENT_FLEET=1` + `SHEPHERD_FLEET_REDIS=redis://host:6379` | Redis Streams 消费者组——精确一次认领、终态 ack、超时回收 |

多机可跑多个服务端副本与多台 runtime;回收器(`SHEPHERD_FLEET_REAP_INTERVAL_S` / `SHEPHERD_FLEET_RECLAIM_MS`)重投不再心跳的 runtime 上的任务。

### 7.2 注册真实后端 vs mock

runtime 按任务的 `executor` 类型选后端,除非 `AGENT_MOCK=1` 强制走 mock:

| 能力 | 后端 | CLI(覆盖 env) |
|---|---|---|
| `CLAUDE_CODE` | Claude(流式 `stream-json`) | `claude`(`CLAUDE_BIN`) |
| `CODEX` | 通用 CLI | `codex exec`(`CODEX_CMD`) |
| `OPENCODE` | 通用 CLI | `opencode run`(`OPENCODE_CMD`) |
| 任意(配 `AGENT_MOCK=1`) | mock——返回固定输出 | 无 |

真实后端需 `git` 与 CLI 在 `PATH`(或经覆盖 env 指定)。新增后端只需实现一个 `CliAgentBackend`(`async fn execute(prompt, cwd, sink)`)并注册一个枚举变体——见 `crates/agent-runtime/src/backend.rs`。

### 7.3 可观测性

```bash
curl -s localhost:8088/agent/work/stats   # 各能力:ready(积压)、在飞、最久卡住
```

注册/心跳端点(供 runtime 使用,需 `DELIVERY:UPDATE`):`POST /agent/runtime`(注册)、`GET /agent/runtime`(列出)、`POST /agent/runtime/{id}/heartbeat`。

---

## 8. HTTP API

### 8.1 鉴权

```bash
# 登录 → 返回会话令牌
curl -s localhost:8088/auth/login -H 'content-type: application/json' \
  -d '{"username":"admin","password":"s3cret"}'

# 作为 Bearer 令牌使用
curl -s localhost:8088/organization -H 'Authorization: Bearer <token>'
```

会话落 PG(服务端重启存活),按 `SHEPHERD_SESSION_TTL_SECS` 过期。写端点按资源 RBAC 校验;读端点开放。

### 8.2 健康检查

| 端点 | 含义 |
|---|---|
| `GET /healthz` | 存活——进程在跑即 `200 ok`(不查依赖) |
| `GET /readyz` | 就绪——能连通 Postgres 才 `200`,否则 `503`(2s 超时) |

### 8.3 MCP

全链路经 Streamable HTTP 暴露为 MCP 工具,入口 `POST /mcp`(JSON-RPC):`initialize` 签发 `Mcp-Session-Id`,`GET /mcp` 维持 SSE 长连接,`DELETE /mcp` 终止。工具按会话 RBAC 过滤(`tools/list` 隐藏你无权调用的工具)。约十个 `shepherd_*` 工具驱动 需求 → 拆分 → 派发 → 验证。

### 8.4 OpenAPI 与自举(dogfood)

服务端在 `GET /api-docs/openapi.json` 发布自身 OpenAPI。Shepherd 能用自己的接口定义/场景/执行能力测**自己**——把运行中的 OpenAPI 导入为接口定义,构建真实链式场景(登录 → 提取 token → 鉴权调用 → 链式调用,外加负向 401),并进程内执行、逐步给出通过/失败。

```bash
# 单链路自举(登录 → 提取 → 鉴权链 + 负向 401)
python3 .claude/skills/openapi-bootstrap/selftest.py

# 每个业务模块一条 CRUD/生命周期场景,逐模块报告
python3 .claude/skills/openapi-bootstrap/scenarios_all.py
```

两者都遵循 `SHEPHERD_BASE`(默认 `http://127.0.0.1:9180`)与 `SHEPHERD_USER` / `SHEPHERD_PASS`(默认 `admin` / `s3cret`)。详见 `.claude/skills/openapi-bootstrap/SKILL.md`。

---

## 9. 故障排查

| 现象 | 可能原因 / 修复 |
|---|---|
| `/readyz` 返回 503 | Postgres 连不上。检查 `DATABASE_URL` 与 PG 是否在跑;该检查有 2s 超时。 |
| 登录失败 | 密码不对——是 `admin` / `SHEPHERD_ADMIN_PASSWORD`(compose 默认 `change-me-please`,README 开发用 `s3cret`),除非你保留了默认,否则不是字面量 `admin`。 |
| dev 下控制台空白 / API 404 | Vite 代理目标不匹配。dev 代理指向 `:9180`;把服务端绑到那里,或设 `SHEPHERD_API` 为你的服务端 URL。 |
| 任务始终无人认领 | 服务端未开机群模式(`SHEPHERD_AGENT_FLEET=1`)、无 runtime 在线,或能力不匹配——核对 `SHEPHERD_CAPS` 与任务 executor 类型,以及 `GET /agent/work/stats`。 |
| 多机 runtime 无法共享任务 | 各服务端副本未设(或非同一个)`SHEPHERD_FLEET_REDIS` → 各自回落到自己的进程内队列。 |
| 真实 agent 不动作 / spawn 报错 | CLI 不在 `PATH`;设 `CLAUDE_BIN` / `CODEX_CMD` / `OPENCODE_CMD`,或先用 `AGENT_MOCK=1` 验证链路。 |
| 接口批量运行卡在 `RUNNING` 无结果 | 设了 `SHEPHERD_RUNNER=noop`(演示占位)。取消它以走原生 runner。 |
| 新迁移未生效 | 重启服务端——迁移在启动时跑;新迁移文件需重新构建。 |
| OIDC 端点 404 | provider 仅在 id 与 secret 两个 env **都**设置时才注册。 |

---

生产部署、镜像构建、Helm/Terraform、CI/CD 与 Day-2 运维见 **[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)**。
