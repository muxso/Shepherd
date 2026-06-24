# 执行者 runtime —— 已迁移到纯 Rust

原先这里的 bridge 脚本(`fleet-runtime.sh` / `claude-agent-async.sh` / `cli-agent-async.sh`
/ `codex-/opencode-agent-async.sh` / `design-draft.sh` / `stream_events.py` / `mock-agent.sh`)
已被 **`crates/agent-runtime`**(纯 Rust 可部署二进制)取代并删除。

## 用法

在内网机器(装好 `git` + 对应 CLI:`claude` / `codex` / `opencode`)运行:

```bash
SHEPHERD_BASE=https://<有公网的 server>   \
SHEPHERD_CAPS=CLAUDE_CODE                 \
RUNTIME_NAME=$(hostname)                  \
AGENT_WORKDIR=/path/to/被改动的仓库        \
MS_ADMIN_PASSWORD=s3cret                  \
  ./target/release/agent-runtime
```

runtime 出站(无需公网入站):登录 → 注册 → 心跳 → 长轮询认领 →
按 `executor` 选后端(claude 流式 / codex / opencode)→ 按模式回调:

- **implement**:工作区改动 `git` 快照成 commit → `POST /delivery/{id}/complete`(交付物 = commit)。
- **design**(OpenSpec/BMAD,由 `ProposalService` 经 `context=design` 标记):产 markdown 设计稿
  → `POST /proposal/{id}/design` → 提案进入待审。

环境变量:`SHEPHERD_BASE` / `SHEPHERD_CAPS` / `RUNTIME_NAME` / `AGENT_WORKDIR` /
`MS_ADMIN_USER`+`MS_ADMIN_PASSWORD`;`AGENT_MOCK=1` 用 mock 后端(自测,不调真 CLI、不耗用量);
`CODEX_CMD` / `OPENCODE_CMD` 覆盖 CLI 命令;`CLAUDE_BIN` 覆盖 claude 路径。

server 侧用 `SHEPHERD_AGENT_FLEET=1`(+ 可选 `SHEPHERD_FLEET_REDIS` 做分布式队列)启用机群。

## 遗留(仍在 delivery 代码里,无 bundled 脚本)

`SHEPHERD_AGENT_CMD`(本地 spawn 子进程)/ `SHEPHERD_AGENT_URL`(远端 Agent API)两种执行者
路由仍存在于 `crates/delivery`,但不再附带示例脚本——优先用上面的 `agent-runtime`。
