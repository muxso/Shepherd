# 任务派发 → 真实执行者(AgentExecutor)

「派发」按钮(前端 `createDelivery` → `POST /delivery`)会把任务交给一个
**可插拔的执行者** `dyn AgentExecutor`。具体用哪个,由 server 启动时的环境变量决定
(见 `crates/server/src/main.rs` 的 executor 路由):

| 环境变量 | 执行者 | 行为 |
|---|---|---|
| `SHEPHERD_AGENT_URL` | `HttpAgentExecutor` | POST 到远端 Agent API(异步,返回 `run_id`,回调 `/delivery/{id}/complete`) |
| `SHEPHERD_AGENT_CMD` | `LocalCommandAgentExecutor` | 本地 spawn 子进程(同步,跑完即交付) |
| `SHEPHERD_LLM_URL` | `LlmExecutor` | 直连 LLM |
| (都没设) | `EchoAgentExecutor` | **桩**:回显任务标题为假 `stub://` 交付物,不干活 |

> 默认是 Echo 桩 —— 看起来「派发即验证通过」其实没做任何事。要真干活必须设上面之一。

## 子进程协议(`SHEPHERD_AGENT_CMD`)

server 把 WorkSpec(任务标题/描述/验收标准)作为 prompt 写到子进程 **stdin**,
然后按行读 **stdout**(见 `crates/delivery/src/adapters/local.rs`):

- `{"event":"<KIND>","message":"...","detail":"..."}` → 审计事件,实时回流
  (合法 KIND:`DECISION` / `FILE_CHANGE` / `TEST_RESULT` / `TOOL_CALL` / `VERDICT` / `LOG`)
- `{"reference":"...","summary":"..."}` → 交付物(任务转 Delivered)
- 其它行 → 当作 `LOG` 事件
- 退出码非 0 → 任务转 Failed(stderr 进错误信息)

## 同步 vs 异步(重要)

server 有一道全局 **30s 请求超时**(`TimeoutLayer`)。`claude -p` 跑一个真实任务
通常要几十秒~几分钟,**同步执行者会把派发请求卡到 408**。所以真实 Claude 必须走**异步**:

- **同步模式**(默认,`SHEPHERD_AGENT_CMD` 不配 `SHEPHERD_AGENT_ASYNC`):
  server spawn 子进程并**阻塞等它跑完**,按 stdout 协议收尾。只适合**秒级**执行者
  (如 `mock-agent.sh`、测试桩)。
- **异步模式**(`SHEPHERD_AGENT_ASYNC=1`):派发**立即返回 Running**,子进程在后台跑,
  跑完经 HTTP 回调 `/delivery/{id}/complete|fail|events` 自行收尾。server 启动时给子进程
  铸一枚回调令牌(`DELIVERY:READ+UPDATE`),并经环境变量注入
  `SHEPHERD_ATTEMPT_ID` / `SHEPHERD_CALLBACK_URL` / `SHEPHERD_CALLBACK_TOKEN`。
  **真实 Claude 用这个。**

## 三个现成脚本

- **`mock-agent.sh`** —— 同步、确定性演示/验证执行者。不调 AI、不改文件,按协议产出
  审计事件 + `mock://` 交付物。
- **`claude-agent.sh`** —— 同步桥接 `claude -p`(会被 30s 超时卡死,仅留作参考/极快任务)。
- **`claude-agent-async.sh`** —— **异步桥接(推荐)**。把 prompt 交给 `claude -p --output-format json`,
  跑完经 HTTP 回调把 `claude://` 交付物 + 审计事件回灌。**会真实改文件、消耗用量**,需 `claude` 已登录。

## 启用(真实 Claude,异步)

在期望被改动的仓库根启动 server(Claude 会在 server 的 cwd 改文件):

```bash
SHEPHERD_AGENT_CMD="$PWD/scripts/agent/claude-agent-async.sh" \
SHEPHERD_AGENT_ASYNC=1 \
DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret \
./target/debug/server
```

然后在需求详情「拆分/交付/验证」里点任务的「派发」即可 —— 派发秒回,UI 轮询/刷新看进度。

## 端到端自测(用 mock,不消耗用量)

```bash
SHEPHERD_AGENT_CMD="$PWD/scripts/agent/mock-agent.sh" MS_BIND=127.0.0.1:9185 \
DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest MS_ADMIN_PASSWORD=s3cret \
./target/debug/server &
TOKEN=$(curl -s -XPOST :9185/auth/login -d '{"username":"admin","password":"s3cret"}' \
  -H 'content-type: application/json' | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
curl -s -XPOST :9185/delivery -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"decompositionId":"<id>","taskId":"<id>","title":"t","executor":"CLAUDE_CODE"}'
# 期望:status=DELIVERED,deliverable.reference 以 mock:// 开头(非 stub://)
```
