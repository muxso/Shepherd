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

## 两个现成脚本

- **`mock-agent.sh`** —— 确定性演示/验证执行者。不调用 AI、不改文件,只按协议产出
  审计事件 + `mock://` 交付物。用于证明「派发 → executor」链路是真的、给演示一个稳定结果。
- **`claude-agent.sh`** —— 真实桥接。把 prompt 交给本机 `claude -p --output-format json`,
  把 Claude 的最终结果转成 `claude://` 交付物。**会真实改文件、消耗用量**,需 `claude` 已登录。

## 启用(真实 Claude)

在期望被改动的仓库根启动 server(Claude 会在 server 的 cwd 改文件):

```bash
SHEPHERD_AGENT_CMD="$PWD/scripts/agent/claude-agent.sh" \
DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret \
./target/debug/server
```

然后在需求详情「拆分/交付/验证」里点任务的「派发」即可。

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
