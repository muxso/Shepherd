# Shepherd 云效同步插件

插件依附于 Shepherd 主应用，复用其服务入口、会话与权限，数据和迁移使用独立数据库。

- `plugin.json`：Shepherd 插件目录使用的元信息。
- `backend/`：可选 Rust crate `shepherd-yunxiao`，由 server 的 `yunxiao` feature 编译。
- `scripts/`：只读采集器和单元测试，通过 Shepherd 同步 API 提交快照。
- `deploy/`：用户级定时采集单元。
- 前端扩展位于 `web/src/plugins/yunxiao/`，由 Shepherd 插件状态控制入口。

在 Shepherd 服务中设置 `SHEPHERD_PLUGINS=yunxiao` ，配置独立的 `SHEPHERD_YUNXIAO_DATABASE_URL` 并重启即可启用。
完整配置、构建和验证步骤见 [运维文档](../../docs/YUNXIAO.zh-CN.md)。
