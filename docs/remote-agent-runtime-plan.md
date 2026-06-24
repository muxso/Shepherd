# 远程 AI 执行者 runtime — 实现计划

目标:页面注册/管理多个**远程 AI 编码执行者**(Claude Code / Codex / OpenCode),派活时按能力选一台执行,流式看进度,交付物=真实 commit。

## 实现状态(2026-06-24,未提交)

- ✅ **PoC 闭环**:`QueueAgentExecutor`(dispatch=入队+Accepted)+ `GET /agent/work/claim` 长轮询 + `scripts/agent/fleet-runtime.sh`(复用 `claude-agent-async.sh`)。`SHEPHERD_AGENT_FLEET=1` 启用。单机端到端已验证(dispatch→claim→complete→204)。
- ✅ **队列抽象**:`WorkQueue` 端口(`enqueue/claim/ack`)。`InMemoryWorkQueue`(单机,已验证)。
- ✅ **分布式队列**:`RedisStreamQueue`(feature `exec-queue-redis`,Redis Streams 消费组,`SHEPHERD_FLEET_REDIS=redis://…`)。**已对真 Redis 验证**:group 自动建 / XADD / XREADGROUP(PEL=1)/ 终态 `DeliveryService::with_queue`→XACK(PEL=0)。
- ✅ **回收 reaper(心跳判活)**:`WorkQueue::reclaim_dead(live, grace)` —— 仅回收「持有者(=runtime id,经 claim `?runtime=`)已不在在线集 + 空闲过 grace」的 PEL 消息(XPENDING 取 consumer→比对注册表在线 id→XRANGE 取 payload→重 XADD+XACK)。server 后台循环喂 `registry.list()` 在线集(`SHEPHERD_FLEET_REAP_INTERVAL_S` / `SHEPHERD_FLEET_RECLAIM_MS=grace`)。真 Redis 验证:**死 runtime 的任务被重投,在线 runtime(持续心跳)的长任务不误回收**。流加 MAXLEN~10000 防膨胀。
- ✅ **register/heartbeat + 机群视图**:`FleetRegistry`(InMemory + RedisFleetRegistry),端点 `POST /agent/runtime`、`POST /agent/runtime/{id}/heartbeat`、`GET /agent/runtime`(在线=心跳<30s)。`fleet-runtime.sh` 出站注册+心跳。Web `Agents.tsx` 加「AI 执行者机群」表(5s 轮询在线)。真 Redis 验证通过。
- ✅ **多 CLI(claude/codex/opencode)**:`ExecutorKind` 加 `OpenCode`,三者各一条 stream + 消费组,能力隔离已验证(dispatch OPENCODE → 仅 `caps=OPENCODE` 认领到,`caps=CLAUDE_CODE`→204)。`fleet-runtime.sh` 按 `executor` 路由到对应 bridge:claude→`claude-agent-async.sh`(流式),codex→`codex-agent-async.sh`、opencode→`opencode-agent-async.sh`(均转通用 `cli-agent-async.sh`:非流式,跑 `$CLI_CMD "<prompt>"`→快照 commit→回调)。**codex/opencode 的 CLI 命令是 best-effort 默认(`codex exec` / `opencode run`),未对真 CLI 验证,可经 `CODEX_CMD`/`OPENCODE_CMD` 覆盖。**
- ⬜ **TODO**:codex/opencode 真 CLI 对接验证 + 流式事件解析(现非流式粗粒度)、PG 持久化注册表、注册令牌(现复用 admin 登录)、每任务 git worktree 隔离、离线 runtime 清理(现仅判活不删)。

## 0. 硬约束(决定整套架构)

- **agent 跑在内网,无公网入站**;**只有服务端有公网**。
- ⇒ 派发必须 **pull**:runtime 出站拨号(register / heartbeat / 长轮询认领 / 事件回流 / 收尾),server **永不**主动连 runtime。
- ⇒ 判活靠 **心跳**,不靠反向探活(对比 runner-agent 注册时拉 `/protocols`,这里做不到)。
- ⇒ 选路 = server 端 **按能力分桶的任务队列** + runtime **长轮询认领**;server 无需知道 runtime 地址。
- 回调那一半(`/delivery/{id}/events|complete|fail`,均出站)**原样复用**。

---

## 1. 架构总览

```
runtime(内网, 只出站)                       server(公网)
─────────────────────                       ──────────────────────────────
1 register ─POST /agent/runtime ──────────►  落库: caps=[claude,codex], labels, maxConc
              ◄── runtimeId + claimToken ───
2 heartbeat ─POST /agent/runtime/{id}/ping ► 周期(如10s); server 据此判活
3 claim     ─GET  /agent/work/claim?caps=.. ► 长轮询(hold≤25s): 队列有匹配→出队返回 WorkSpec(含 attemptId)
              ◄── WorkSpec | 204 ───────────  (无活则 204, runtime 立即重连)
4 run       本地: worktree + spawn claude/codex/opencode
5 stream    ─POST /delivery/{id}/events ───►  FILE_CHANGE/TOOL_CALL/DECISION (现成)
6 finish    ─POST /delivery/{id}/complete ─►  交付物=commit URL (现成) / /fail
```

server 端 delivery 的 executor 不再是"同步/异步直接跑",而是 `QueueAgentExecutor`:**dispatch = 入队 + 立即返回 Running**,真正执行由某台 runtime 认领后异步回调收尾。

---

## 2. CLI 后端抽象(runtime 内)

三工具差异只有三点,抽成 trait:

```rust
// crates/agent-runtime/src/backend.rs
#[async_trait]
trait CliAgentBackend: Send + Sync {
    fn kind(&self) -> &str;                              // "CLAUDE_CODE" / "CODEX" / "OPENCODE"
    fn spawn(&self, workspace: &Path, prompt: &str) -> std::io::Result<Child>;
    fn parse_line(&self, line: &str) -> Option<ExecEvent>;   // tool_use → FILE_CHANGE/TOOL_CALL/DECISION
    fn final_result(&self, ev: &serde_json::Value) -> Verdict; // {ok, summary}
}
```

| 后端 | spawn | parse_line |
|---|---|---|
| Claude Code | `claude -p --output-format stream-json --verbose --permission-mode acceptEdits` | 搬 `scripts/agent/stream_events.py` 的映射(Edit/Write→FILE_CHANGE, Bash→TOOL_CALL, text/todo→DECISION) |
| Codex | `codex exec --json …` | Codex 事件格式 |
| OpenCode | `opencode run …`(待确认 CLI) | OpenCode 格式 |

工具无关、全共享的部分:worktree 隔离、commit 快照、事件回流、回调、并发控制。

---

## 3. Crate 划分

| crate | 新/改 | 内容 |
|---|---|---|
| `crates/agent-runtime`(新二进制) | 新 | runtime 守护进程:register/heartbeat/claim 循环 + `CliAgentBackend` 三实现 + worktree + commit 快照 + 回调。结构参照 `crates/runner-agent`。 |
| `crates/agent-fleet`(新, 六边形) | 新 | server 端"执行者机群"域:runtime 注册表 + 任务队列 + 认领。domain(Runtime/WorkItem/Capability)、ports(FleetRepository/Queue)、application(register/heartbeat/enqueue/claim/list)、adapters(http/pg)。 |
| `crates/delivery` | 改 | 新增 `QueueAgentExecutor`(`dispatch` = 入队到 agent-fleet + 返回 `Accepted{run_id}`)。`ExecutorKind` 加 `OpenCode`。 |
| `crates/server` | 改 | executor 路由加分支:`SHEPHERD_AGENT_FLEET=1` → `QueueAgentExecutor`;挂 agent-fleet 路由。 |
| `crates/migrate` | 改 | 新表(见 §5);记得 `touch crates/migrate/src/lib.rs` 重编。 |
| `web/src/pages/Agents.tsx` | 改 | 从 runner-agent 视图改/加"AI 执行者机群":在线状态(心跳)、能力(claude/codex/opencode)、当前并发、最近任务。注册改为"生成接入令牌"给 runtime 用(不是填 baseUrl,因为 server 连不上它)。 |

> 注意 §概念区分:现有 `crates/runner` 的 runner-agent 是**协议/API 探测执行机**(http/grpc/sql),与本计划的 **AI 编码执行者** 是两套。新建 agent-fleet,不要塞进 runner。

---

## 4. 队列与认领协议(server 端,全部入站 from runtime)

| 端点 | 方向 | 说明 |
|---|---|---|
| `POST /agent/runtime` | runtime→server | 注册:body=`{name,caps:["CLAUDE_CODE",..],labels,maxConcurrency}`;返回 `{runtimeId, claimToken}`(claimToken 权限 `DELIVERY:READ+UPDATE` + `AGENT:CLAIM`) |
| `POST /agent/runtime/{id}/ping` | runtime→server | 心跳:body=`{running:[attemptId..], free:N}`;server 刷 `last_seen` |
| `GET /agent/work/claim?caps=CLAUDE_CODE,CODEX&max=1` | runtime→server | **长轮询**(hold≤25s):队列有匹配能力的 `Queued` 任务 → 原子置 `Claimed(runtimeId)` 并返回 WorkSpec;无 → 204 |
| `POST /delivery/{id}/running\|events\|complete\|fail` | runtime→server | **复用现成**;鉴权用 claimToken |

**认领原子性**:`UPDATE agent_work_item SET status='CLAIMED', runtime_id=$1, claimed_at=now() WHERE id=(SELECT id FROM agent_work_item WHERE status='QUEUED' AND capability=ANY($caps) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING …`。`SKIP LOCKED` 保证多 runtime 并发认领不重。

**超时回收**:扫 `Claimed` 且 `last_seen`(对应 runtime)超 N×心跳 或 `Running` 超 maxRunSecs → 置回 `Queued`(重派)或 `Failed`。需幂等:回调带 attemptId,迟到的回调按当前状态决定接受/丢弃。

---

## 5. 数据模型(migrations)

```sql
-- 0065_agent_runtime.sql
CREATE TABLE agent_runtime (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  caps          TEXT[] NOT NULL,          -- ['CLAUDE_CODE','CODEX']
  labels        JSONB NOT NULL DEFAULT '{}',
  max_concurrency INT NOT NULL DEFAULT 1,
  token_hash    TEXT NOT NULL,            -- claimToken 仅存哈希
  last_seen     TIMESTAMPTZ,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 0065_agent_work_item.sql  (任务队列)
CREATE TABLE agent_work_item (
  id            TEXT PRIMARY KEY,
  attempt_id    TEXT NOT NULL,            -- 关联 delivery attempt
  capability    TEXT NOT NULL,            -- 要求的执行者种类
  spec          JSONB NOT NULL,           -- WorkSpec
  status        TEXT NOT NULL,            -- QUEUED/CLAIMED/RUNNING/DONE/FAILED
  runtime_id    TEXT REFERENCES agent_runtime(id),
  claimed_at    TIMESTAMPTZ,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ON agent_work_item (status, capability, created_at);
```

---

## 6. runtime 自理项(server 够不着那台机)

- 各 CLI 二进制 + 登录态(每台一份 seat);启动自检 `which claude/codex/opencode` 决定 `caps`。
- **每任务 git worktree 隔离**(`git worktree add` 到临时目录),避免并发互踩(现脚本共享单工作区的坑)。
- commit 快照:跑前记 HEAD,跑后 `git write-tree`/`commit-tree` 推 `shepherd/deliver/<attemptId>`,reference=commit URL(搬现成逻辑)。
- 本地并发上限(信号量)、认领节流、claimToken 持有 + 401 自动重注册。
- 断线重连:claim 204/超时即重发;心跳失败退避重连。

---

## 7. 分阶段落地

1. **PoC(打通闭环)**:agent-fleet 最小版(内存队列)+ `QueueAgentExecutor` 入队 + agent-runtime 只挂 Claude 后端 + 长轮询认领 + 复用回调。单 runtime 跑通 dispatch→流式→commit。**不改 web,用 curl 注册。**
2. **持久化 + 多后端**:落 PG(§5)、`SKIP LOCKED` 认领、Codex/OpenCode 后端、超时回收。
3. **选路策略**:按 label/repo/最小负载选(不只能力匹配 + 轮询)。
4. **Web**:Agents 页改成机群视图(在线/能力/并发/任务),注册=发令牌。
5. **韧性**:重试/自纠正(对接现有 `SHEPHERD_MAX_REVISIONS` + orchestration observer)、并发上限背压、审计完善。

---

## 8. 风险 / 待定

- **OpenCode CLI** 的流式输出格式与登录方式待确认(可能没有 `stream-json` 等价物 → 退化为粗粒度事件)。
- **认领公平性 / 饥饿**:长轮询 + `SKIP LOCKED` 基本够;高并发再加 NOTIFY/LISTEN 或 WS 推。
- **凭证边界**:claimToken 泄露=可冒领任务;短 TTL + 可吊销 + 仅 `AGENT:CLAIM`+`DELIVERY:UPDATE`。
- **迟到回调幂等**:超时回收后原 runtime 仍可能回调,按 attempt 当前状态裁决。
- 与现有单 env executor(`SHEPHERD_AGENT_CMD/URL`)**并存**,用 `SHEPHERD_AGENT_FLEET=1` 切换,不破坏现有本地/mock 流程。
