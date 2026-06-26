# Shepherd 架构

Shepherd 是一个用 Rust 构建的 AI 研发监督平台。核心链路是:需求(多版本)→ 任务拆分(DAG)→ 派发给 AI 执行者 → 完整性验证,由编排器自动联动。它建立在一套六边形架构之上:业务逻辑与 IO 解耦,纯逻辑层零 IO 依赖、可纳秒级单测,数据库 / HTTP / 执行器都在 feature 门控的适配器后面。

操作说明见 [README.md](README.md);路线图见 [ROADMAP.md](ROADMAP.md)。

规模:32 个 crate、866 个测试(单元 / 用例 / e2e + 集成),clippy `-D warnings` 零告警,业务规则全程用真实 PostgreSQL 16 验证。

## 1. 六边形分层

每个业务上下文都切成四层,依赖一律向内:

```
adapters     sqlx / axum / reqwest —— 最外圈,碰 IO
   │ 依赖向内
application   用例编排
   │
ports         trait:Repository / Directory / Gateway …
   ▲ 实现
domain        纯业务规则,零依赖
```

依赖铁律:`domain` / `application` / `ports` 不得引入任何 IO 依赖(sqlx / axum / tokio / reqwest),IO 只出现在 `adapters` 的 feature 门控模块里。两道保证:

1. 默认 `cargo build -p <crate>` 不启用任何 IO feature,sqlx/axum 不在依赖图,纯层根本 import 不到(编译期屏障);
2. 每个上下文的 `tests/architecture.rs` 扫描源码,即使开着 feature 也禁止纯层引用 IO crate(兜底)。

收益:从同步 → async → 加 HTTP → 换真实 PG,`domain` / `application` 一行未改;换存储只动组装根:

```rust
// Arc::new(InMemoryUserRepository::new())  // 测试 / 本地
   Arc::new(PgUserRepository::new(pool))     // 生产
```

## 2. Workspace 布局

1 crate = 1 限界上下文;层是 crate 内的模块,IO 适配器是 feature 门控的模块。

| 分组 | crate | 职责 |
|---|---|---|
| 共享 / 接入 | `kernel` | 共享内核(分页、权限 `PermissionSet`),零依赖 |
| | `webauth` | 共享鉴权基元(`AuthUser` / `Session` / `SessionStore` 端口 + axum 提取器) |
| | `mcp` | Model Context Protocol 引擎(JSON-RPC + 工具注册表) |
| | `migrate` | 版本化迁移(`sqlx migrate!`,73 个迁移),schema 单一真源 |
| | `server` | 组装根,唯一服务 bin |
| | `shepherd-cli` | CLI(`shepherd`),封装 REST 全链路 |
| | `agent-runtime` `runner-agent` | 远程执行者 runtime(出站认领 + spawn CLI) |
| 测试管理域 | `system-setting` | 用户 / 组织 / 角色 / 鉴权(本地 + OIDC) |
| | `project` `case` `case-management` `bug` `test-plan` | 项目 / 用例评审 / 功能用例 / 缺陷 / 测试计划 |
| | `api-definition` `api-scenario` | 接口定义 / 用例 / Mock 资产 + 场景编排 |
| | `api-test` `api-runner` `runner` | 接口批量执行 + 原生 HTTP 执行器 + 断言引擎 |
| | `environment` `mock-runtime` `perf` `probe` | 环境 / Mock 服务 / 性能压测 / 拨测探针 |
| | `comment` `follow` | 评论 / 关注人 |
| Shepherd 域 | `requirement` | 需求(多版本:线性快照 + baseline) |
| | `task` | 任务拆分 DAG(依赖、就绪门控、状态机)+ `Planner` 端口 |
| | `design` | 设计阶段(设计稿 → 人审批门 → 修订) |
| | `delivery` | 派发给 AI 执行者 + 机群队列 + 执行审计 |
| | `verification` | 需求↔任务↔实现 双向追溯 + 缺口检测 |
| | `orchestrator` | 跨上下文编排(process manager),纯协调零业务依赖 |
| | `skill` | AI Skill 编排(定义 / 复用 / `compose` 传递展开防环) |

组装根(`server/src/main.rs`)是全工程唯一 `use` 到 `PgXxxRepository` 这类具体类型的地方:连 PG 池 → 跑迁移 → new 各上下文用例 → `Router::merge` + 中间件 → 启动。换存储 / 框架 / 执行器的改动都收敛在这一个文件 + feature 开关里。

## 3. 鉴权与跨模块 RBAC

鉴权是横切关注点:登录在 system-setting,但权限校验要落到每个模块的写端点。把 `AuthUser` / `SessionStore` 留在 system-setting 会让其它上下文够不着、且污染边界,于是抽成共享 crate `webauth`——kernel 之上、各上下文之下:

```
kernel (PermissionSet)
   ▲
webauth (AuthUser / Session / SessionStore 端口 + axum 提取器)
   ▲                              ▲
system-setting (签发会话)   project / case / bug / test-plan (写端点校验)
```

- `AuthUser` 提取器:`impl FromRequestParts<S> where Arc<dyn SessionStore>: FromRef<S>`——任何把会话存储放进 router state 的模块都能直接 `async fn handler(user: AuthUser, …)`;无令牌 / 失效 / 过期 → 401。
- `SessionStore` 端口:`create / get(自动滤过期) / revoke`。生产实现 `PgSessionStore` 落 `ms_session`(跨重启存活);测试用 `webauth::testing::InMemorySessionStore`。
- 按资源 RBAC:写端点统一 `if !user.can("PROJECT", "ADD") { 403 }`,资源 · 动作来自登录时解析的 `PermissionSet`(凭证权限 ∪ 角色权限)。读端点保持开放。

`webauth` 默认 build 不含 axum(提取器在 `http` feature 后),各上下文把它列为可选依赖,§1 的编译期屏障照常成立。

## 4. 主链路:Shepherd 域

```
project ─▶ requirement(多版本) ─▶ task(拆分 DAG)
                                     │ 派发
                                     ▼
                              delivery(AI 执行者:Claude Code / Codex / OpenCode)
                                     │ 交付落终态
                         ┌───────────┴───────────┐  ← orchestrator(纯协调)
                         ▼                       ▼
                  驱动任务生命周期         验证门(judge)+ 自动建链 + 回灌验证
                 (…→Verified 解锁下游)    verification(按验收标准建覆盖链 → 完整性报告)
```

上下文之间零类型依赖,跨上下文只用字符串 id 互引(`requirement_id` / `decomposition_id` / `task_id`)。需要协调的地方收到组装根或 `orchestrator`:

- `orchestrator` 是 process manager,在自有 gateway 端口(`TaskGateway` / `VerificationGateway` / `Judge`)上工作,不依赖任何业务 crate;组装根把这些端口接到 task / verification 的真实服务,并把 delivery 的 `DeliveryObserver` 钩子桥接进来。
- 一次 `dispatch` → 执行者跑(流式 emit 事件)→ 落终态触发 observer → orchestrator:① 把任务推到 Delivered;② 过验证门 judge(通过 → 任务 Verified 解锁下游;不通过 → Failed,缺口保留);③ 裁决记入交付审计。全自动,无需手动流转。

设计阶段(`design`)在拆分前插入:执行者先产设计稿 → 人审批门 → 通过才进任务拆分。

### AI 触点:端口 + 可插拔适配器

三个 AI 触点都是端口,组装根按环境选适配器(默认无需 AI,可升级到真实 LLM):

| 触点 | 端口 | 默认 | 真实 LLM |
|---|---|---|---|
| 拆分 | `task::Planner` | `HeuristicPlanner`(每标准一任务 + 集成) | `LlmPlanner`(`SHEPHERD_LLM_URL`)/ HTTP(`SHEPHERD_PLANNER_URL`) |
| 执行 | `delivery::AgentExecutor` | `EchoAgentExecutor` | `LlmExecutor` / 本地 spawn(`SHEPHERD_AGENT_CMD`)/ 远端(`SHEPHERD_AGENT_URL`)/ 机群(`SHEPHERD_AGENT_FLEET`) |
| 验证门 | `orchestrator::Judge` | `AcceptAllJudge` | `LlmJudge`(fail-closed)/ `RuleJudge` / HTTP(`SHEPHERD_JUDGE_URL`) |

LLM 适配器(`server/src/llm.rs`)统一走 OpenAI 兼容 `chat/completions`,`extract_json` 容忍散文 / 围栏。领域与应用层对“是否用 AI、用哪个模型”完全无感。

### 机群派发

执行者多在内网、无公网入站,所以是出站拉取而非服务端推送:执行者 register / heartbeat → 长轮询认领 → spawn CLI → 流式回传事件 → 落终态。单机用进程内队列;多机配 `SHEPHERD_FLEET_REDIS` 走 Redis Streams 消费组(精确一次认领、终态 ack、持有者掉线后超时回收)。`GET /agent/work/stats` 报各能力的积压 / 在飞 / 最久滞留。详见 [机群设计笔记](docs/remote-agent-runtime-plan.md)。

### 接入层:MCP + CLI

- MCP:组装根注册 15 个 `shepherd_*` 工具,AI 经 Model Context Protocol 直接驱动全链路。Streamable HTTP:`initialize` 签发 `Mcp-Session-Id`、`GET /mcp` SSE、`DELETE` 终止;按工具 RBAC(`tools/list` 过滤、无权 `-32003`)。
- CLI(`shepherd`):`login` / `req` / `decompose` / `task` / `dispatch`(`--skills` 自动 compose)/ `verify` / `skill` / `agent connect`。

## 5. 测试

测试金字塔:

| 层 | 占比 | 用什么测 | 速度 |
|---|---|---|---|
| 领域单元 | ~70% | `#[test]`,纯函数,无 IO | 纳秒~微秒 |
| 用例 | ~20% | 注入内存假实现 / Spy 的 `#[tokio::test]` | 微秒 |
| 适配器集成 | ~8% | `#[ignore]` + 真实 PG 16 | 毫秒~百毫秒 |
| 端到端 | ~2% | `tower::oneshot` 离线打 axum,或真起服务 curl | 毫秒 |

绝大多数非集成测试整体 `finished in <0.1s`;集成测试按需 `cargo test -- --ignored` 跑。

设计手法:

- 构造即校验:`Email` / `PageRequest` / `NewProject` 一旦存在即合法,下游不重复校验。
- 端口隔离 + 假实现:用例只认 trait,注入 `InMemoryXxx`,99% 的测试不碰数据库。
- Spy 端口断言副作用:不仅断言返回值,还断言“该发生的发生了、不该发生的没发生”——如 api-test 的“无可用池时绝不派发”、OIDC 用例的“绝不调用 CFT 校验路径”。
- 横切语义固化进端口 / DB:软删除的 active 语义写进端口契约,并用 PG 部分唯一索引 `UNIQUE(…) WHERE deleted=false` 在 DB 层兜底。

几处用测试钉死的核心逻辑:case 的多人投票聚合状态机、bug 的数据驱动转移图、test-plan 的统计聚合(组状态由子推导)、api-test 的派发前资源解析(无池显式报错且不派发)、task 的 `Decomposition` DAG(无环 + 就绪门控)、skill 的 `compose`(传递展开 + DFS 防环)。

## 6. 现状与未做

已闭环:

- 鉴权(本地 Argon2 + 防账号枚举 + OIDC 飞书 / 企业微信)+ 会话落 PG + 令牌过期 / 登出 + 跨模块按资源 RBAC。
- 主链路全自动联动:需求 → 拆分 → 设计审批门 → 派发 → 验证门,三个 AI 触点均可接真实 LLM。
- 机群派发与回收(进程内 / Redis Streams);MCP 工具 + Skill 编排;版本化迁移(advisory lock,消除并发 DDL 竞争)。
- 原生 HTTP 执行器:`api-runner`(reqwest + 纯函数断言引擎)经 `api-test` 的 local 适配器接成 `TaskDispatcher`,同步跑完直接落终态;组装根默认选它。`TaskDispatcher` 用 `DispatchOutcome` 区分异步(Accepted→RUNNING)与同步(Completed)两类执行者,按需路由。

未做 / 边界:

- JMeter 压测引擎本身(解析脚本、跑压测、回收结果)仍是外部适配器;任务下发已接通,执行节点内部运行与结果回写未做,替换时上层零改动。压测可走 JMeter / Goose。
- 验证门当前以 judge 通过判定 Verified(默认 AcceptAll;配 judge 才严格);delivery 记 attempt + 事件 + 裁决审计,尚未记 agent 逐 token 决策。
- 可观测性(tracing / metrics)、OIDC state CSRF 校验、令牌刷新 / LDAP、其余模块广度端点。
