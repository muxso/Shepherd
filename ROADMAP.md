# Shepherd 路线图 · 剩余工作量与风险清单

> 现状:**Shepherd 主链路与 AI 接入层已闭环**(见 ARCHITECTURE),32 crate / 866 测试 /
> clippy 零告警 / 全程真实数据库验证。本文分四块:**§0 已交付**、**§A Shepherd 收尾**、
> **§B 测试管理域广度**(对标成熟测试平台的端点覆盖)、**§C 难点 / 生产化**。
>
> **估算口径:** 数字是**数量级**,非承诺;单位 **dev-week(人周)**。薄 CRUD 约 10–15 端点/周,
> 逻辑/集成重约 3–6 端点/周。

---

## §0 已交付(Shepherd 核心 + 地基)

- **地基(测试管理后端 → Rust 六边形)**:架构守卫 + 1 上下文 1 crate + TDD;鉴权(本地 Argon2 +
  OIDC 飞书/企业微信)+ 会话落 PG + 令牌过期/登出;system-setting/project 全量 RBAC CRUD;
  **跨模块写端点按资源 RBAC**;复用测试管理域(case/bug/test-plan/api-test)。
- **Shepherd 主链路**:`requirement`(多版本)→ `design`(设计稿 + 人审批门)→ `task`(拆分 DAG)
  → `delivery`(AI 执行者 + 执行审计)→ `verification`(双向追溯 + 缺口检测);
  `orchestrator` 自动联动(交付→驱动任务→**验证门**→回灌验证)。
- **机群派发**:`delivery` 机群队列 + `agent-runtime`(纯 Rust 执行者:出站 register/heartbeat/长轮询认领
  → spawn CLI(claude/codex/opencode)→ 流式回传事件 → git 快照);单机进程内队列,多机 Redis Streams
  消费组(精确一次认领 + 终态 ack + 持有者掉线超时回收);`GET /agent/work/stats` 报积压/在飞。
- **AI 层**:`skill` 编排(定义/复用/compose + 注入执行);严格验证门(judge);自动拆分(Planner);
  **三触点(拆分/执行/验证)接真实 LLM**(OpenAI 兼容)。
- **接入层**:`mcp`(15 工具 + 按工具 RBAC + SSE/会话)、`shepherd-cli`(全链路 + `agent connect`)。
- **部署**:Docker/Compose 单机栈、Helm chart(dev/prod values)、多云 Terraform(AWS/GCP/Azure)、
  GHCR 镜像构建 + OIDC 自动部署流水线(见 `docs/DEPLOYMENT`)。
- **生产化(部分)**:版本化迁移、健康探针(`/healthz` `/readyz`)、结构化日志、优雅关闭、超时/体积限制。

---

## §A Shepherd 收尾(深化已有能力,低-中风险)

| # | 项 | 内容 | 估时 |
|---|---|---|---|
| A1 | **可观测性** | ✅ Prometheus metrics(`/metrics`:请求计数 + 时延直方图,按 method/status;零新依赖)+ 中间件;待补:OpenTelemetry 分布式追踪、业务指标(机群/LLM) | (HTTP 指标已交付) |
| A2 | **AI 执行深化审计** | ✅ agent-runtime 现额外捕获测试事件:测试命令 → `TEST_RESULT`,并从 tool_result 抽真实通过/失败汇总(cargo/pytest/jest…);经既有 seq 有序事件流回放(`GET /delivery/{id}/events`)。待补:逐 token 决策、文件 diff 内容 | (测试轨迹已交付) |
| A3 | **MCP 服务端推送** | ✅ 事件总线(broadcast)+ `GET /mcp` SSE 订阅:交付(running/delivered/failed)、验证门裁决、需求自动交付事件主动推送(`event: notification`);零新依赖 | (核心已交付) |
| A4 | **真实 LLM 产线化** | ✅ Anthropic 原生 Messages API 适配(与 OpenAI 兼容并存,`SHEPHERD_LLM_WIRE`)、每调用超时 + 对 429/5xx 退避重试(尊重 `Retry-After`)、prompt 版本化、延迟/令牌观测;待补:成本核算价表、流式 | (核心已交付) |
| A5 | **`shepherd init`** | ✅ 离线脚手架命令:生成需求模板 + 快速上手文档(`--dir`/`--force`);待补:从模板文件直接 `req add` 导入 | (核心已交付) |
| A6 | **需求/任务广度** | 需求评论/附件/排序、任务重指派/批量、依赖图可视化数据、project 成员管理 | ~3–5 周 |
| A7 | **MCP 安全** | ✅ 会话 id 随机不可猜(UUID,替代自增)+ 按属主隔离(跨用户复用/删除拒绝)+ MCP 调用审计日志(用户/方法/工具/成败);待补:按工具更细粒度 scope | (核心已交付) |
| | **小计** | | **~11–17 人周** |

---

## §B 测试管理域广度(对标成熟测试平台的端点覆盖,机械为主、可并行、低风险)

> Shepherd 不强依赖这些;但若要"替代成熟测试平台投入生产",需把已验证模式铺到剩余端点。
> 几乎无架构风险,是体力 + 评审。

| 域 | 端点(总/已做/剩) | 复杂度 | 估时(人周) | 备注 |
|---|---|---|---|---|
| project-management | 195 / 2 / 193 | 中(环境/文件/全局参数 + 模块树) | **~19** | 文件管理、环境较重 |
| bug-management | 56 / 2 / 54 | 低-中 | **~5** | 评论/附件/关联/同步 |
| case-management | 142 / 1 / 141 | 中-高(评审流、自定义字段、脑图) | **~24** | 评审核心已验 |
| test-plan | 156 / 2 / 154 | 中-高(编排、报告、定时) | **~26** | 统计核心已验 |
| system-setting | 356 / ~20 / ~336 | 中(海量配置 CRUD) | **~28** | 鉴权 + 用户/组织/角色已做 |
| api-test | 269 / 1 / 268 | 高(定义/场景/Mock/导入) | **~54** | 引擎见 §C;此处仅 CRUD/解析 |
| dashboard | 38 / 0 / 38 | 中(聚合查询) | **~5** | 依赖各域数据 |
| **小计** | **~1184 剩** | | **~160 人周** | ≈ 3 人 × 1 年(纯广度) |

---

## §C 难点 / 生产化(有实质风险 / 需设计)

| # | 项 | 为何难 / 现状 | 估时 | 风险 |
|---|---|---|---|---|
| C1 | **JMeter 执行引擎内部** | 跑 .jmx、分布式压测、结果回收。**建议不重写**,把外部 JMeter 执行器藏在 `TaskDispatcher` 端口后(下发已通;原生 `api-runner` 已是默认,可跑功能用例) | 包壳 ~3–6 周 / 重写 6+ 月 | 极高(若重写) |
| C2 | **执行/消息基础设施** | Kafka(rdkafka)、K8s 调度(kube-rs)、对象存储——各自 FFI/运维耦合 | 各 ~2–4 周 | 中-高 |
| C3 | **插件系统** | 动态加载第三方扩展;Rust 需重新设计(WASM?进程隔离?) | ~4–8 周 + 设计 | 高(架构未定) |
| C4 | **报告 / 通知 / 定时 / i18n** | 报告聚合分享、消息机器人、cron、多语言(web 中英双语已做) | 各 ~2–4 周 | 中 |
| C5 | **生产化收尾** | 限流 / CORS / 统一错误体(problem+json)、分环境配置;**容器 / Helm / 多云 Terraform / CI-CD 自动部署已交付(见 `docs/DEPLOYMENT`);metrics / 追踪见 A1** | ~2–4 周 | 中 |
| C6 | **鉴权收尾** | OIDC state CSRF 校验、令牌刷新、LDAP | ~1–3 周 | 高(安全) |

---

## §D 关键建议

1. **Shepherd 已可独立演进**:它不依赖 §B 的广度端点;若产品方向是 AI 研发监督,
   优先做 §A(可观测性 + 真实 LLM 产线化 + MCP 推送),§B/§C 按需。
2. **不要原生重写 JMeter 引擎**(C1):把外部 JMeter 执行器藏在 `TaskDispatcher` 后——砍掉数月与最大风险。
3. **真实 LLM 先做产线化护栏**(A4):重试/超时/限流/成本观测 + prompt 版本化,再放量。
4. **§B 广度可分域并行**:模式已固定,多人按域推进;每域先补读端点再补写端点。
5. **若要替代成熟测试平台投入生产**:§B(~160 人周)+ §C(不含 JMeter 重写 ~25–45 人周)+ §A 选做,
   合计 ≈ 4 名工程师 × ~1 年;坚持原生重写 JMeter 再 +6 个月以上。
