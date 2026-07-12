# 安全策略

> English: [SECURITY.md](SECURITY.md).

## 报告漏洞

请不要在公开 Issue 中披露漏洞。请使用 GitHub 的私有漏洞报告功能(Security → Advisories → New),或发送邮件到仓库所有者资料页上列出的地址。我们会在 72 小时内确认收到,并在修复发布前与你协调披露时间。

## 受支持的版本

项目处于 1.0 之前阶段,只有最新的 `main` 会收到安全修复。

## 威胁模型说明

部署前请注意:

- **服务器一旦被攻陷,等于每个 agent-runtime 机器都能被远程执行代码。** 运行时从服务器拉取任务提示词,再喂给本地的 AI CLI。请把服务器视为整个机群的信任根,并尽量缩小其暴露面。
- **修改默认管理员密码。** 当 `SHEPHERD_ADMIN_PASSWORD` 是弱默认值时,服务器会在启动时报错提醒。该密码仅用于管理员账号初始化与 Web 登录。
- **运行时凭据:只用 API Key,每个运行时一把。** 通过 `POST /system/apikey` 为每个运行时签发独立密钥,授予最小权限集(`DELIVERY:UPDATE` + `REQUIREMENT:UPDATE`),并设为 `SHEPHERD_AGENT_KEY`(缺少该值的运行时拒绝启动);某个机器被攻陷时,吊销这一把密钥即可隔离影响。
- **TLS 终止放在反向代理上。** 服务器监听明文 HTTP,切勿直接暴露在公网上。
- **CORS**:只在 `SHEPHERD_CORS_ORIGINS` 中填入受信任的来源。
- **限流**:默认开启(每客户端 200 rps);可通过 `SHEPHERD_RATE_LIMIT_RPS` 调整,设为 0 关闭。
