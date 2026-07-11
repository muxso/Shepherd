# Changelog

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/);版本号遵循 SemVer。
未发布的改动记在 Unreleased 下,发版时移入对应版本段落。

## [Unreleased]

### Added
- 缺陷 ↔ 需求/场景用例/功能用例 关联(追溯链),含关联抽屉 UI
- 派发可定向到具体注册 runtime(按 name;Redis 下走专属流,目标离线任务留队)
- 项目成员/用户组管理接通后端(添加成员、建组、移除)
- per-runtime API key(`sak_` 前缀,argon2 存储,60s 验证缓存,可吊销),
  含 Web 管理页与 agent-runtime 的 `SHEPHERD_AGENT_KEY` 静态凭证模式
- 登录失败锁定:同用户名连续 5 次失败锁 15 分钟(HTTP 429)
- SECURITY.md / CONTRIBUTING.md / LICENSE(GPL-2.0)/ CHANGELOG

### Changed
- 全部读端点要求认证 + `READ` 权限(此前约 50 个 GET 匿名可访问)
- 限流默认开启(每客户端 200 rps,`SHEPHERD_RATE_LIMIT_RPS=0` 关闭)
- server 与 agent-runtime 检测到弱默认管理员口令时启动告警
- Web 配色改为简约浅蓝(Arco 风格),默认暗色为中性深灰
- workspace 全员 `publish = false`(通用 crate 名不发 crates.io)

### Fixed
- 互斥锁中毒改为恢复而非连环 panic(请求路径上的限流/metrics/会话)
- 英文语言下左侧导航溢出;测试超时路径不回收子进程等 clippy 修复
