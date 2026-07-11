# Security Policy / 安全策略

## Reporting a vulnerability / 报告漏洞

请勿在公开 issue 中披露安全漏洞。请通过 GitHub 的
[Private vulnerability reporting](../../security/advisories/new) 提交,或发邮件到仓库
Owner 主页公示的邮箱。我们会在 72 小时内确认,并在修复发布前与你协调披露时间。

Please do not disclose vulnerabilities in public issues. Use GitHub's
private vulnerability reporting (Security → Advisories → New), or email the
address listed on the repository owner's profile. We acknowledge reports
within 72 hours and coordinate disclosure with you before a fix ships.

## Supported versions / 支持版本

项目处于 0.x 阶段,只有 `main` 分支最新版本接收安全修复。
Only the latest `main` receives security fixes while the project is pre-1.0.

## Threat model notes / 威胁模型须知

部署前必读:

- **server 被攻破 = 所有 agent-runtime 机器可被远程执行代码。** 执行机通过出站长轮询
  从 server 领取任务,任务 prompt 会直接喂给本机的 AI CLI(claude/codex/opencode/codebuddy)
  执行。server 是全机群的信任根,务必最小化其暴露面。
- **必须修改默认管理员口令。**`SHEPHERD_ADMIN_PASSWORD` 使用弱默认值时 server 与
  agent-runtime 启动都会打警告;生产环境应设置强随机口令。
- **执行机凭据:推荐每台 runtime 一把 API key,而非共享管理员口令。** 管理员通过
  `POST /system/apikey` 按最小权限(`DELIVERY:UPDATE` + `REQUIREMENT:UPDATE`)
  为每台执行机签发独立 key,设为 `SHEPHERD_AGENT_KEY`;单台失陷只需吊销那一把
  key,不用全机群换口令。
- **建议在反向代理层终结 TLS。** server 自身监听明文 HTTP;公网部署必须置于
  HTTPS 反代之后,agent-runtime 的 `SHEPHERD_BASE` 也应指向 https 地址。
- **CORS**:仅将可信来源写入 `SHEPHERD_CORS_ORIGINS`,不要使用通配符。
- **限流**:默认按客户端 200 rps;`SHEPHERD_RATE_LIMIT_RPS` 可调,设 0 关闭。

Before deploying:

- **A compromised server means RCE on every agent-runtime box.** Runtimes pull
  task prompts from the server and feed them to local AI CLIs. Treat the server
  as the fleet's root of trust and minimize its exposure.
- **Change the default admin password.** Both server and agent-runtime warn at
  startup when `SHEPHERD_ADMIN_PASSWORD` is a weak default.
- **Runtime credentials: prefer one API key per runtime over a shared admin
  password.** Issue each runtime its own key via `POST /system/apikey` with the
  minimal permission set (`DELIVERY:UPDATE` + `REQUIREMENT:UPDATE`) and set it
  as `SHEPHERD_AGENT_KEY`; a compromised box is contained by revoking that one
  key instead of rotating a fleet-wide password.
- **Terminate TLS at a reverse proxy.** The server listens on plain HTTP;
  never expose it directly on the public internet.
- **CORS**: put only trusted origins in `SHEPHERD_CORS_ORIGINS`.
- **Rate limiting**: on by default (200 rps per client); tune via
  `SHEPHERD_RATE_LIMIT_RPS`, set 0 to disable.
