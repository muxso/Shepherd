# 变更日志

格式遵循 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);版本号遵循 SemVer。
未发布的改动放在 Unreleased 下,发布时移入对应的版本小节。

> English: [CHANGELOG.md](CHANGELOG.md).

## [Unreleased]

## [0.0.2] - 2026-07-14

### 新增
- 缺陷 ↔ 需求 / 场景用例 / 功能用例 的关联(可追溯链路),包含关联抽屉 UI
- 派发可指定某个已注册的运行时(按名称;在 Redis 下使用独立 stream,离线的目标任务保持排队)
- 项目成员 / 用户组管理接入后端(添加成员、创建组、移除)
- 每个运行时独立的 API Key(`sak_` 前缀,argon2 存储,60 秒校验缓存,可吊销),含 Web 管理页面与 agent-runtime 的 `SHEPHERD_AGENT_KEY` 静态凭据模式
- 登录失败锁定:同一用户名连续失败 5 次锁定 15 分钟(HTTP 429)
- SECURITY.md / CONTRIBUTING.md / LICENSE(GPL-2.0)/ CHANGELOG

### 变更
- 所有读取端点现在都需要认证 + `READ` 权限(此前约 50 个 GET 端点可匿名访问)
- 限流默认开启(每客户端 200 rps;`SHEPHERD_RATE_LIMIT_RPS=0` 可关闭)
- 检测到弱默认管理员密码时,server 与 agent-runtime 会在启动时告警
- Web 配色改为清爽的浅蓝(Arco 风格);默认深色主题为中性深灰
- 所有 workspace 成员 `publish = false`(不把共享 crate 名发布到 crates.io)

### 修复
- Mutex 中毒现在会恢复,而不再级联 panic(请求路径上的限流器 / 指标 / 会话)
- 英文语言下的左侧导航溢出;若干 clippy 修复,包括测试超时路径未回收子进程的问题
