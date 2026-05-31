# MeterSphere Rust 重构:整体架构 · TDD 方法论 · 迁移路线

> 本文是六个阶段增量重构的收尾综述,面向团队评审。代码见同目录各 crate,
> 操作性说明(怎么跑、怎么起服务)见 `README.md`,本文聚焦**为什么这样设计**。

当前规模:**11 个 crate(1 上下文 1 crate + 1 个共享 `webauth`,无 `ms-` 前缀)/ 242 个单元·用例·e2e 测试 + 真库集成测试**,
覆盖 6 个业务模块(system-setting / project / case / bug / test-plan / api-test)。
**鉴权(本地 + OIDC 飞书/企业微信)与跨模块 RBAC 已闭环**:每个写端点都经 `webauth::AuthUser` 提取器做按资源权限校验(见 §2.4)。

---

## 一、为什么重构(问题陈述)

原 Java 版是 Spring Boot 模块化单体,约 2736 个 Java 文件、227 个测试。核心痛点不在语言,在**结构**:

1. **业务逻辑与 IO 长在一起**:逻辑写在 `Service` 里直接 `@Resource` 注入 MyBatis Mapper,
   不开数据库就没法测,被迫写慢的集成测试。
2. **测试金字塔倒挂**:227 个测试几乎全是 `@SpringBootTest + @Sql + MockMvc` 的重量级集成测试,
   还带 `@TestMethodOrder` 让测试相互依赖——不能单独跑、不能并行、改一处崩一片。
3. **隐式行为埋在切面**:权限判断、用户创建途径校验(CFT/Liber SDK)等横切逻辑藏在 AOP 里,
   出问题就是运行时 500,且几乎无法单测(如 OIDC 登录后 500 那条 quirk)。

**重构的价值主张**:用测试反推出一个 **IO 与逻辑解耦**的架构,把倒挂的金字塔翻正,
把隐式行为变成显式、可回归的契约。

---

## 二、整体架构:六边形 + Strangler

### 2.1 分层(每个业务模块都这样切)

```
        ┌─────────────────────────────────────────────┐
        │  adapters   (sqlx / axum)   —— 最外圈,碰 IO  │
        │     │ 依赖向内                                │
        │     ▼                                         │
        │  application  (用例编排)                       │
        │     │                                         │
        │     ▼                                         │
        │  ports   (trait:Repository / Directory ...)  │
        │     ▲                                         │
        │     │ 实现                                     │
        │  domain   (纯业务规则,零依赖)                  │
        └─────────────────────────────────────────────┘
```

**依赖铁律**:`domain` / `application` / `ports` 三层**不得引入任何 IO 依赖**
(sqlx / axum / tokio / reqwest);IO 只允许出现在 `adapters` 的 feature 门控模块里。
保证有两道:① 默认 build 不启用任何 IO feature → sqlx/axum 不在依赖图,纯层 import 不到(编译期);
② 每个 context 的 `tests/architecture.rs` 源码扫描,feature 开着也禁止纯层引用 IO crate(兜底)。

### 2.2 Workspace 物理布局

**1 crate = 1 限界上下文**(无 `ms-` 前缀;workspace 内部不发布,无需前缀防冲突)。
层是 crate 内的**模块**;IO 适配器是 **cargo feature 门控**的模块。

| crate | 角色 | 关键点 |
|---|---|---|
| `kernel` | 共享内核(分页、权限 `PermissionSet`) | 零依赖 |
| `webauth` | 共享鉴权基元(`AuthUser` / `Session` / `SessionStore` 端口 + axum 提取器) | 仅依赖 kernel;`http` feature 门控 axum 提取器,`test-util` 提供内存会话存储 |
| `<context>`(system-setting/project/case/bug/test-plan/api-test) | 一个限界上下文 | `src/{domain,ports,application,adapters}`;`adapters::pg`(feature=pg)、`adapters::http`(feature=http)等门控 |
| `api-runner` | 原生 HTTP 执行器 + 纯函数断言引擎 | 可独立复用的库 |
| `migrate` | 版本化迁移(`sqlx migrate!`) | schema 单一真源 |
| `server` | **组装根**:唯一认识具体类型、拼装、起服务 | 唯一 bin |

**纯净如何保证**(收敛 crate 数的同时不丢编译屏障):
- 默认 `cargo build -p <context>` **不启用任何 IO feature** → sqlx/axum 不在依赖图,纯层 import 不到(编译期屏障);
- feature 开启后,每个 context 的 `tests/architecture.rs` 源码扫描禁止 `domain/ports/application` 引用 IO crate(兜底);
- 错误类型一律 `thiserror` 派生 `Error`+`Display`。

> **组装根**(`server/src/main.rs`)是全工程**唯一** `use` 到 `PgXxxRepository` 这类具体类型的地方:
> 连一个共享 PG 池 → 跑迁移 → new 各上下文用例 → `Router::merge` + 生产中间件 → 启动。
> 想换存储/框架/执行器(JMeter↔原生 runner),改动收敛在这一个文件 + feature 开关。

> **重构历史**:初版曾按"层 × 模块"拆成 24 个 `ms-` 前缀 crate(每上下文 core/-pg/-http 三个)。
> 那是过度工程 + 命名冗余;现收敛为 10 个无前缀 crate(1 上下文 1 crate + feature 门控),
> 更符合 Rust 社区惯例,同时保住"纯层不碰 IO"的保证(默认 build 屏障 + 架构测试)。

### 2.3 已兑现的回报

整个过程反复验证了一件事:**从同步 → async → 加 HTTP 层 → 换真实 PG 存储,
`domain`/`application` 的业务逻辑一行未改**。换库只动组装根一行:

```rust
// Arc::new(InMemoryUserRepository::new())   // 测试 / 本地
   Arc::new(PgUserRepository::new(pool))      // 生产
```

### 2.4 鉴权与跨模块 RBAC(`webauth` 共享 crate)

鉴权是**横切关注点**:登录在 system-setting,但权限校验要落到*每个*模块的写端点。
若把 `AuthUser` / `SessionStore` 留在 system-setting,其余上下文就够不着(且让它们依赖 system-setting
会污染上下文边界)。于是抽出第 11 个 crate `webauth`——kernel 之上、各上下文之下的**共享鉴权基元**:

```
   kernel (权限模型 PermissionSet)
      ▲
   webauth (AuthUser / Session / SessionStore 端口 + axum 提取器)
      ▲                                    ▲
 system-setting (登录签发会话)      project / case / bug / test-plan (写端点校验)
```

- **`AuthUser` 提取器**:`impl FromRequestParts<S> where Arc<dyn SessionStore>: FromRef<S>`——
  任何把会话存储放进 router state 的模块都能直接 `async fn handler(user: AuthUser, ...)`,
  无令牌→401、令牌失效/过期→401。提取器必须住在 `AuthUser` 的定义处(孤儿规则),故落在 `webauth`。
- **`SessionStore` 端口**:`create / get(自动滤过期) / revoke`。生产实现 `PgSessionStore`(system-setting)
  落 `ms_session`;测试用 `webauth::testing::InMemorySessionStore`(`test-util` feature,无过期、令牌自增可复现)。
- **重导出零改动**:system-setting 原本自有 `AuthUser/Session/SessionStore`,迁移时改为
  `pub use webauth::{...}`,内部上百处 use-site **一行不动**;`PgSessionStore` 实现的是重导出后的同一 trait。
- **按资源 RBAC**:写端点统一 `if !user.can("PROJECT", "ADD") { 403 }`。资源·动作来自登录时
  解析的 `PermissionSet`(凭证权限 ∪ 授权角色权限)。读端点保持开放(本期只锁写端点)。

| 模块 | 受保护写端点 | 所需权限 |
|---|---|---|
| system-setting | 用户/组织/角色/用户-角色 全 CRUD | `SYSTEM_USER` / `ORGANIZATION` / `USER_ROLE` 各 `ADD/UPDATE/DELETE` |
| project | `POST /project` | `PROJECT:ADD` |
| case | `POST /case-review/{r}/{c}` | `CASE_REVIEW:REVIEW` |
| bug | `POST /bug` · `POST /bug/{id}/status` | `BUG:ADD` · `BUG:UPDATE` |
| test-plan | `POST /test-plan` | `TEST_PLAN:ADD` |

**组装根**注入同一个 `PgSessionStore` 给所有模块 router(`sessions.clone()`),全栈共用一套会话。
真库 e2e 验证过跨模块按资源判定生效:admin 持 `PROJECT` 权限但无 `BUG` 权限时,建项目 201、建缺陷 403。

> **纯净不破**:`webauth` 默认 build 不含 axum(提取器在 `http` feature 后);各上下文把它列为
> **可选依赖**,仅 `http` feature 拉入。默认 `cargo build -p project` 的依赖图里仍无 axum/sqlx/webauth,
> §2.1 的编译期屏障 + 架构测试照常成立。

---

## 三、TDD 方法论

### 3.1 红 - 绿 - 重构,但分层施测

测试金字塔被翻正(对比 Java 版倒挂):

| 层 | 占比 | 用什么测 | 速度 |
|---|---|---|---|
| 领域单元测试 | ~70% | `#[test]`,纯函数,无 IO | 纳秒~微秒 |
| 用例测试 | ~20% | 注入**内存假实现 / Spy** 的 `#[tokio::test]` | 微秒 |
| 适配器集成测试 | ~8% | `#[ignore]` + testcontainers/真实 PG 16 | 毫秒~百毫秒 |
| 端到端 | ~2% | `tower::oneshot` 离线打 axum;或真起服务 curl | 毫秒 |

绝大多数(~230 个)非集成测试整体 **`finished in <0.1s`**;十余个真库集成测试按需 `-- --ignored` 跑。

### 3.2 关键设计手法

- **构造即校验,非法状态造不出来**:`Email` / `PageRequest` / `NewProject` 一旦存在即合法,
  下游无需重复校验(make illegal states unrepresentable)。
- **端口隔离 + 假实现**:用例只认 trait,测试注入 `InMemoryXxx`,**99% 的测试不碰数据库**。
- **Spy 端口断言副作用**:不仅断言返回值,还断言"**该发生的发生了、不该发生的没发生**"——
  如 api-test 的"无可用池时**绝不派发**"、OIDC 用例的"**绝不调用 CFT 校验路径**(`validated_calls==0`)"。
  这对重 IO 模块(JMeter/K8s/Kafka)尤其关键:核心规则完全脱离真实执行器即可测。
- **消除时间依赖**:评审历史按时序给入,"最新结论 = 序列靠后者",聚合**完全确定性**、可穷举。
- **横切语义固化进端口/DB**:软删除的 active 语义写进端口契约(`find_active_by_name`),
  并用 PG **部分唯一索引** `UNIQUE(...) WHERE deleted=false` 在 DB 层兜底(MySQL 做不到这么干净)。

### 3.3 用 TDD 啃下的"硬骨头"——四类核心逻辑

这些都是 Java 版埋在 DTO/Service/MyBatis/AOP 里、几乎无法单测的部分,现在各被毫秒级用例钉死:

| 模块 | 逻辑类型 | 代表用例 |
|---|---|---|
| case | **多人投票聚合状态机** | 会签任一不通过→UnPass、改票最新覆盖、建议不计、SYSTEM 不计票 |
| bug | **数据驱动转移图** | 配置化状态流、跳级拒绝(NEW⇏CLOSED)、脏边丢弃、非法流转状态不变 |
| test-plan | **统计聚合** | 状态三态+归档、通过率/执行率(0除+四舍五入)、组状态由子推导 |
| api-test | **派发前资源解析** | 池解析优先级、无池显式报错且不派发(原 500 前移) |

### 3.4 把"踩坑经验"变成回归契约

memory 里两条真实 quirk 被固化为**真库端到端验证过的契约**:

- **batch-run 池解析**:Java 版任务下发后才在资源池查询处 500;现在入口即区分
  "未配置池→400 / 池不可用→409",且"不派发"由 Spy 保证。
- **OIDC 绕过 CFT**:Java 版 OIDC 登录后展示名查询 500;现在 `GET /system/user/names`
  走直查旁路稳定 200,且 `never_touches_the_cft_validated_path` 从契约上禁止回退到被拦截路径。

---

## 四、技术栈映射(为可测性而选)

| Java | Rust | 选型理由 |
|---|---|---|
| Spring MVC | **axum** + tower | handler 是普通 async fn,可脱离 server 用 `oneshot` 直测 |
| Spring DI / `@Resource` | 构造注入 + `Arc<dyn Trait>` | 测试注入假实现,无需容器 |
| MyBatis | **sqlx**(运行期 query) | 不需编译期连库;Repo trait 可 mock |
| Jackson | **serde** | 序列化是纯函数 |
| JUnit + MockMvc + @Sql | `cargo test` + testcontainers + `tower::oneshot` | 单测纳秒级,集成测才碰真 PG |
| MySQL | **PostgreSQL 16** | `jsonb`、CTE、`RETURNING`、部分唯一索引;sqlx 对 PG 支持最成熟 |

---

## 五、迁移路线(Strangler Fig + 一次性切库)

### 5.1 数据库引擎决策的连带影响

选 PG 而非沿用 MySQL,收益是 `jsonb`/CTE/`RETURNING`/部分唯一索引;
**代价必须正视**:换引擎后**无法与 Java 共用同一套活库**做"逐模块切流量"。
因此迁移策略调整为:

```
   现网 MySQL ──pgloader一次性迁移──▶ PG
        │                              │
        └──── 短暂双跑(影子流量)对比响应一致性 ────┘
                          │
                   一次性 cutover 切换
```

### 5.2 模块推进顺序(已按此完成 6 个)

按"契约清晰度 + JMeter 耦合度"从外围啃起:

```
阶段1 system-setting  (地基:用户/权限)        ✅ 全链路
阶段2 project         (软删除 + 部分唯一索引)   ✅ 全链路
阶段3 case            (投票聚合状态机)          ✅ 全链路
阶段4 bug             (配置化转移图)            ✅ 全链路
阶段5 test-plan       (统计聚合)                ✅ 全链路
阶段6 api-test        (批量运行池解析 quirk)    ✅ 全链路(执行器为边界实现)
```

每个模块都走完整竖切:domain→ports→application→PG→HTTP→组装根,真库端到端验证。

### 5.3 诚实的边界:还没做的

- **api-test 的 JMeter 执行引擎本身**(解析脚本、跑压测、回收结果)仍是护城河,非数月可复刻。
  但**任务下发**已接通:`PgBatchReportExecutor` 落 PENDING 报告后,经 `TaskDispatcher` 出站端口
  把任务 HTTP POST 给执行节点(`api-test` 的 jmeter 适配器,对应 Java
  `MsHttpClient.batchRunApi`),据结果置 `RUNNING` / `DISPATCH_FAILED`。已真库 + stub 执行节点
  端到端验证:`reportId` 透传到节点、成功转 RUNNING、节点不可达则 500 + DISPATCH_FAILED(不卡 PENDING)。
  **真正未做的**是执行节点内部的运行与结果回写(report 回调更新),替换时上层零改动。
- **不一定要 JMeter,且已走通**:原生 Rust runner `api-runner`(reqwest + 纯函数断言引擎
  StatusIs/BodyContains/HeaderEquals/JsonFieldEquals)经 `api-test` 的 local 适配器
  接成 `TaskDispatcher`:取 `ms_api_case`(`PgCaseSpecSource`,断言存 JSONB)→ reqwest 就地跑 → 聚合 →
  `DispatchOutcome::Completed{status}`(同步跑完,报告直接最终态)。组装根按 `MS_RUNNER=local` 选它,
  真库 + stub 目标端到端验证:全过→SUCCESS、含失败→ERROR。`TaskDispatcher` 用 `DispatchOutcome` 区分
  **异步 JMeter(Accepted→RUNNING)** 与 **同步原生(Completed)**,两者并存按需路由;压测留给 JMeter/Goose。
  - **per-case 明细**:每个用例结果(SUCCESS/ERROR + 失败原因)经 `CaseResultSink` 写入 `ms_api_case_result`(JSONB)。
  - **并发执行**:PARALLEL 用 `buffer_unordered` 按并发上限并行,SERIAL 顺序;真库验证 5×0.3s 用例总耗时≈0.6s(非 1.5s)。
- ~~schema 仍是裸 `CREATE TABLE IF NOT EXISTS`~~ ✅ 已换成 **sqlx `migrate!` 版本化迁移**
  (`crates/migrate/migrations/*.sql` 单一真源;生产与集成测试都调 `migrate::run`,
  记录在 `_sqlx_migrations`;迁移器持 advisory lock,顺带消除并发 DDL 竞争)。
- ~~认证/会话中间件、完整 RBAC 鉴权落地到 HTTP~~ ✅ 已完成(见 §2.4):本地登录(Argon2 + 防账号枚举)
  + OIDC(飞书/企业微信)+ 会话落 PG(跨重启存活)+ 令牌过期/登出 + 跨模块按资源 RBAC,
  抽出共享 `webauth` crate 承载提取器与会话端口,真库 e2e 验证。
- **未做**:可观测性(tracing/metrics)、OIDC state CSRF 校验、其余模块(dashboard 等)广度端点。

### 5.4 剩余路线

1. ✅ ~~`PgBatchReportExecutor` 接真实 JMeter 下发~~(HTTP `TaskDispatcher`,`api-test`,
   真库 + stub 节点端到端验证);待补:执行节点内 JMeter 运行 + 结果回写报告。
2. ✅ ~~sqlx `migrate!` 版本化迁移;pgloader 迁移脚本~~(`crates/migrate` + `migration/`);
   待补:迁移后影子双跑校验工具。
3. ✅ ~~鉴权 + 把权限内核(`kernel::permission`)接到 HTTP~~(共享 `webauth` crate:
   `AuthUser` 提取器 + `SessionStore` 端口;本地 + OIDC 飞书/企业微信登录,会话落 PG,
   跨模块写端点按资源 RBAC,真库 e2e 验证);待补:OIDC state CSRF 校验、令牌刷新、LDAP。
4. 可观测性:`tracing` + OpenTelemetry。
5. 继续推进剩余模块广度端点。

---

## 六、风险与权衡(给评审者)

| 风险 | 缓解 |
|---|---|
| JMeter 引擎复刻成本极高 | 短期把执行阶段当外部 adapter(保留 Java 执行器 / gRPC 调用),Rust 只重写编排+断言+报告 |
| MySQL→PG 不能长期双写 | 一次性迁移 + 影子双跑对比,而非长期并存 |
| 不追求 100% 行为等价 | 这是清理隐式行为债的机会;用测试用例重新定义"正确行为"(如把 500 前移为 400/409) |
| 集成测试并发跑 DDL 偶发竞争 | `cargo test -- --ignored --test-threads=1` |
| 生成的 448 个 MyBatis 实体 | 不手抄;按 sqlx 实际用到的字段声明,顺手砍历史冗余 |

---

## 七、一句话总结

这次重构的本质不是"把 Java 翻译成 Rust",而是**用测试驱动出一个 IO 与逻辑解耦的架构**:
让最复杂的业务规则(三类状态机)和最隐蔽的踩坑经验(两条 quirk)都变成毫秒级、可回归、
真库验证过的契约——而这些,恰恰是 Java 版当初最难测、最容易再次踩坑的地方。
