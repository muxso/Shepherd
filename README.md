# Shepherd 🐑✨

> **AI 研发监督者 —— 让 AI Agent 为你工作,你来确保需求完整实现**

Shepherd 是一个用 **Rust** 构建的下一代研发管理平台。它不仅提供项目管理、需求拆解与任务分配能力,更核心的定位是:**成为你与 AI 编程助手(Claude Code、Codex 等)之间的监管与验证层**。

## 🎯 核心理念

AI 很擅长写代码,但它不会自己判断"是否完成了需求"。Shepherd 充当 **牧羊人** 的角色:

- **引领**:将需求拆解为 AI 可执行的任务
- **监管**:通过 MCP 协议与 AI Skill 追踪 AI Agent 的一举一动
- **验证**:自动检查需求与实现之间的完整性缺口

## ✨ 核心功能

| 模块 | 能力 |
|------|------|
| **项目管理** | 项目/组织/成员管理,支持多项目并行 |
| **需求管理** | 需求拆解、依赖关系、优先级排序、**多版本(线性快照 + baseline)** |
| **任务拆分** | 自动/手动将需求拆分为可分配给 AI 的**可独立交付**的任务 DAG |
| **AI Agent 监管** | 对接 Claude Code、Codex,追踪执行过程、代码变更、决策日志 |
| **完整性验证** | 需求 ←→ 任务 ←→ 代码实现 的双向追溯与缺口检测 |
| **MCP 集成** | 支持 Model Context Protocol,让 AI 与你定义的上下文工具交互 |
| **AI Skill 编排** | 定义、复用和组合 AI Skill,规范 AI 行为 |

## 🧱 技术特点

- **🚀 Rust 原生**:高性能、低内存占用、安全可靠
- **🔌 开放架构**:`AgentExecutor` 端口背后支持多种 AI Agent(Claude Code、Codex…),本地子进程与远端 Agent API 双适配器,按环境/任务路由
- **📊 可观测性**:完整的 AI 执行日志与审计追踪
- **🔗 API First**:RESTful API + Webhook 回调,便于集成到现有 DevOps 流程
- **🧪 TDD 驱动**:六边形架构 + 1 crate = 1 限界上下文;`domain/ports/application` 纯层零 IO 依赖(编译期屏障 + 架构守卫),业务规则全程真库验证

## 🎯 典型使用场景

1. **PM/技术负责人**:在 Shepherd 中编写需求 → 拆解任务 → 分发给 AI Agent
2. **AI 执行**:Claude Code / Codex 接管任务,生成代码
3. **Shepherd 监管**:实时记录 AI 的每一步决策、文件变更、测试结果
4. **自动验证**:对比"需求描述"与"AI 产出",给出完整性报告
5. **人工补全**:标记缺口,手动补充或重新指派 AI

```
需求(多版本) ──拆分──▶ 任务 DAG(可独立交付) ──派发──▶ AI 执行者(Claude Code/Codex)
     ▲                                                              │
     └────────────── 完整性验证(需求 ←→ 实现 缺口检测)◀── 交付物(diff/PR)
```

---

## 🏗️ 快速开始

### 现在就能跑(REST 服务)

唯一二进制入口是组装根 `crates/server`(已设 `default-members`):

```bash
# 启动服务(需要一个 PostgreSQL;迁移在启动时自动施加)
DATABASE_URL=postgres://user:pass@localhost:5432/shepherd \
MS_ADMIN_PASSWORD=changeme \
cargo run

cargo run -- --migrate-only      # 只建表退出
```

```bash
# 登录拿令牌
curl -s localhost:3000/auth/login -H 'content-type: application/json' \
  -d '{"username":"admin","password":"changeme"}'      # → {"token":"..."}

# 需求(多版本)→ 拆分任务 DAG → 派发给 Claude Code
curl ... -X POST /requirement              -d '{"projectId":"p1","title":"用户登录","acceptanceCriteria":["正确凭证登录"]}'
curl ... -X POST /decomposition            -d '{"requirementId":"<rid>","requirementVersion":1}'
curl ... -X POST /decomposition/<did>/task -d '{"title":"实现登录API","dependencies":[]}'
curl ... -X POST /delivery                 -d '{"decompositionId":"<did>","taskId":"t1","title":"实现登录API","executor":"CLAUDE_CODE"}'
```

执行者按环境变量路由(同一 `AgentExecutor` 端口背后切换):

| 环境变量 | 执行者适配器 | 语义 |
|---|---|---|
| `SHEPHERD_AGENT_URL` | 远端 Agent SDK/API(`exec-http`) | 异步,`Accepted{runId}` → 回调收尾 |
| `SHEPHERD_AGENT_CMD` | 本地 spawn `claude`/`codex`(`exec-local`) | 同步,headless 跑完返回交付物 |
| (都不设) | `EchoAgentExecutor` 桩 | 无真实 agent,回显(本地/演示) |

### 规划中的 CLI(目标体验)

```bash
cargo install shepherd-cli
shepherd init my-project
shepherd req add "实现用户登录功能"
shepherd req breakdown --ai
shepherd agent connect --type claude-code
shepherd task run --task-id xxx --assign-to claude-code
shepherd verify --req-id xxx
```

> `shepherd-cli` 与 `verify` / `--ai 自动拆分` 为路线图项,见下方实现现状。

---

## 📍 实现现状

诚实对照愿景与已落地代码(每个上下文都走完 domain→ports→application→PG→HTTP→组装根,真库 e2e 验证;**316 个测试**):

| 能力 | crate | 状态 |
|---|---|---|
| 鉴权 / 会话 / RBAC(本地 + OIDC 飞书·企业微信) | `system-setting` + `webauth` | ✅ |
| 项目 / 组织 / 角色 / 用户管理(全 CRUD + RBAC) | `system-setting` `project` | ✅ |
| 需求管理(多版本:线性快照 + baseline + 验收标准) | `requirement` | ✅ |
| 任务拆分(DAG、依赖、就绪门控、可独立交付) | `task` | ✅ |
| AI Agent 派发与交付记录(Claude Code/Codex,本地+远端双适配器) | `delivery` | ✅ |
| **完整性验证(需求 ←→ 任务 ←→ 实现 缺口检测)** | `verification` | ⬜ 规划中 |
| **MCP 集成 / AI Skill 编排 / 执行决策日志审计** | — | ⬜ 规划中 |
| **shepherd-cli** | — | ⬜ 规划中 |

> 测试管理域(`case`/`bug`/`test-plan`/`api-test`,源自 MeterSphere 重构)与上述上下文并存于同一 workspace,可作为质量/回归能力复用。
> **整体架构 · TDD 方法论 · 演进路线见 [ARCHITECTURE.md](ARCHITECTURE.md)**;剩余工作量与风险清单见 [ROADMAP.md](ROADMAP.md)。

## 🗂️ 结构(14 个 crate)

```
crates/
  kernel/          共享内核(分页 / 权限 PermissionSet)
  webauth/         共享鉴权基元(AuthUser / SessionStore + axum 提取器)
  system-setting/  用户·组织·角色·鉴权(本地 + OIDC)  ┐
  project/         项目                                │ 每个 = 1 限界上下文:
  requirement/     需求(多版本)                       │ src/{domain,ports,application,adapters}
  task/            任务拆分 DAG                         │ adapters::pg(feature=pg)/http(feature=http)
  delivery/        AI 执行者派发与交付                  │ tests/architecture.rs 守卫纯层不碰 IO
  case/            用例评审                             │
  bug/             缺陷                                 │ (delivery 另含 exec-local / exec-http
  test-plan/       测试计划                             │  两个执行者适配器,端口后切换)
  api-test/        接口批量执行                         ┘
  api-runner/      原生 HTTP 执行器 + 纯函数断言引擎(可复用库)
  migrate/         版本化迁移(sqlx migrate!)
  server/          ★ 组装根 = 唯一 bin 入口
```

每个上下文 crate:`domain`/`ports`/`application` 为纯模块(默认 build 无 IO 依赖,编译期屏障);
`adapters::pg`/`http` 等是 feature 门控模块;`tests/architecture.rs` 源码扫描兜底禁止纯层引用 IO crate。
错误类型一律 `thiserror` 派生 `Error`+`Display`。

## 🔌 程序入口

根目录无 `src/main.rs`(Cargo 工作区);唯一 bin 入口是组装根 **`crates/server/src/main.rs`**
(产出 `server` 可执行文件)。`cargo run` 即启动(已设 default-members)。

```bash
cargo run                      # 根目录直接启动入口
cargo run -- --migrate-only    # 只建表退出
cargo test --workspace         # 全量测试(非集成测试秒级)
DATABASE_URL=... cargo test --workspace -- --ignored   # 真库集成测试
```
