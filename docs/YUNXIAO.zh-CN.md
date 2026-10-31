# Shepherd 云效同步插件

云效数据只存放在插件独立数据库中，通过 Shepherd 的插件入口查看。插件复用登录与权限，不向 Shepherd 原生项目、需求、用例、目录、任务、交付或统计表写入数据；主系统也不联查插件表。

## 安装与启用

1. 使用 Shepherd 标准 server/web 镜像构建和 Compose 配置。云效插件默认编译，可以通过 `cargo build -p server --no-default-features` 排除。
2. 为插件创建独立 PostgreSQL 数据库和专用账号。账号仅有插件库权限，不授予 Shepherd 主库访问权限。
3. 设置 `SHEPHERD_PLUGINS=yunxiao` 和 `SHEPHERD_YUNXIAO_DATABASE_URL=postgres://.../shepherd_yunxiao`，然后重启服务。未配置插件库或连接指向主库时，启动失败，不会回退到主库。Docker Compose 支持这两个变量。
4. 插件只在启用时连接自己的数据库，运行 `plugins/yunxiao/backend/migrations/` 下的迁移。表名保留 `ms_devops_*`，全部位于插件库，无原生业务表外键。`projectId` 仅保留为兼容旧同步协议的外部关联文本，不创建或查询原生项目。
5. 在「系统 → 插件」查看状态，进入云效统计页面。需求、缺陷、迭代、用例、提交和同步记录都在插件中查询，不出现在原生业务列表或人机协同统计中。

## 采集与同步

采集器位于 `plugins/yunxiao/scripts/`，仅单向读取云效，通过 Shepherd 插件 API 提交完整快照，不向云效写入，不发送消息。

- 复用共享只读采集器，`SHEPHERD_COLLECTOR_PATH` 默认为 `~/requirement-code-metrics`，凭据沿用其私有配置。
- 共享采集器需要 `plugins/yunxiao/scripts/yunxiao-collector.patch` 中的 requirementIds 扩展。
- 插件私有 env 包含 `SHEPHERD_URL`、`SHEPHERD_SYNC_KEY`、`SHEPHERD_SOURCE_ID`、`SHEPHERD_COLLECTOR_PATH`、`SHEPHERD_SYNC_DAYS` 和 `SHEPHERD_TEST_REPO_IDS`；存放于 `~/.config/shepherd-yunxiao/env`，权限 600。
- 运行 `python3 plugins/yunxiao/scripts/yunxiao_sync.py ~/.config/shepherd-yunxiao/env`。
- 定时任务位于 `plugins/yunxiao/deploy/`，默认仓库位置 `~/Shepherd`。迁移旧单元时更新路径并执行 `systemctl --user daemon-reload`。
- 私有缓存、详情缓存和可重放快照保留在 `~/.cache/shepherd-yunxiao/`。
- 完整快照校验通过后才在插件库内事务发布。采集失败保留上次成功数据；按来源和外部 ID 去重，源更新时间较新的记录优先。
- 云效「已发布」状态是外部业务事实，不计作 Shepherd 人工或 AI 验收。

## 停用与旧版本兼容

清空 `SHEPHERD_PLUGINS` 并重启，同时停止采集 timer。停用后不连接或迁移插件库，接口不挂载，菜单隐藏；原生业务与插件历史数据均不自动删除。

旧接入曾在主库运行迁移 0112/0113，并投影原生需求、用例和目录。本版本只校验已经执行的旧迁移，保留原 SQL 校验和；新装主库不会执行这两项迁移。旧表、字段和已经导入的原生记录不自动清理，也不自动复制到独立插件库。已有部署需要在备份、核对历史原生记录是否被人工编辑后，另行迁移或清理；本次代码调整未执行线上数据变更。

历史验证文档 `YUNXIAO-*-20261028.md` 记录的是旧接入，不能作为当前数据隔离保证或部署步骤。

## 验证

```sh
cargo test -p shepherd-yunxiao --all-features --locked
cargo test -p server --bin server plugins::tests --locked
cargo check -p server --no-default-features --locked
python3 -m unittest discover -s plugins/yunxiao/scripts -p 'test_*.py' -v
cd web && npm run build
```

数据库集成测试仅使用独立的 `SHEPHERD_YUNXIAO_TEST_DATABASE_URL`，执行 `cargo test -p shepherd-yunxiao --all-features --test pg_sync -- --ignored`。测试不依赖原生项目表，并验证同步不创建原生业务表。
