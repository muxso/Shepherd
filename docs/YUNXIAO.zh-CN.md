# 云效研发统计（第一阶段）

入口：`/devops`，首页二级菜单「云效研发统计」。项目、需求、缺陷、迭代和实际提交来自云效，仅作单向同步；云效保留业务编辑入口。外部事实存储于 `ms_devops_*`；同一次成功同步会将来源拥有的需求和功能用例投影到原生项目列表，不生成任务、交付、AI验收事件，不影响已有接口测试结果。

## 数据与统计

- 云效分页期间可能重复返回同一工作项，采集端按类型+外部ID去重并保留源更新时间较新的记录，完成数量按去重结果核验。
- 用 `cloud_org_id + cloud_project_id` 唯一映射一个 Shepherd 项目。所有记录用 `source_id + kind + external_id` 去重；提交 ID 为仓库ID+SHA。
- `requirement / bug / sprint` 首次全量建立快照，后续完整拉取、按差异更新；从完整成功快照消失的记录标记 inactive，保留历史。提交默认拉取最近7天、保留更早已采集事实，每次成功校验后刷新该窗口。
- 延期口径：优先级紧急/高/中，未发布、完成或取消，且迭代结束日或计划/产品完成日早于上海时区今天；保留全部优先级原始记录。
- 交付口径：云效状态为「已发布」，且 updateStatusAt 在所选最近N天内；不当作 Shepherd 人工验收结果。
- 代码时间使用作者 authoredDate；按实际作者归组，排除多父提交和合并标题，committer不作为作者补充。来源分支祖先减去目标分支祖先用于需求关联，避免把合并者或目标已有提交算作需求代码。
- 组织可见仓库代码与需求关联代码分别展示；未关联提交保留，并不据此认定属于某个项目。未知作者别名列入「待归属」，在共享采集器 report_groups.py 维护。
- 状态历史从首次成功同步的基线之后开始记录；保留云效状态时间及同步观察时间，不推算导入前历史。轮询间发生并结束的多次变化可能不可见。
- 代码覆盖区间显示实际已采集窗口；选择更长日期不会伪造未采集的历史代码。后续连续窗口逐步累积；重启超过窗口的历史缺口需要补采。
- 已接入配置范围内的 Testhub 功能用例，保留用例库/目录树、原编号、创建人、负责人、优先级、标签、前置条件、步骤和预期结果。测试计划和执行结果尚未迁移，不伪造用例执行通过状态。
- 原生条目通过 external_source_id + external_id 唯一识别，手工创建的其他条目不被同步覆盖或删除。需求正文变化追加不可变版本；云效状态保存在自定义字段并映射原生生命周期，不创建AI验收。
- 需求和测试用例的列表响应会省略正文，采集器补取详情，并按源更新时间缓存于私有 requirement-details.json / testcase-details.json。列表缺失更新时间时重新获取详情，避免沿用过期正文。空步骤只在云效原始详情确实为空时保留。

## 同步协议

HTTP 端点已注册 OpenAPI：

- `GET/POST /api/devops/sources`
- `POST /api/devops/sources/{id}/runs`
- `PUT /api/devops/runs/{id}/records`，每批最多200条，上传方默认100条且JSON小于1.4MB
- `POST /api/devops/runs/{id}/complete`，必须提供四种核心类型及可选testcase类型的精确去重数量及代码窗口
- `POST /api/devops/runs/{id}/fail`
- `GET /api/devops/sources/{id}/dashboard?days=7`
- `GET /api/devops/sources/{id}/records?kind=requirement&page=1&pageSize=30`

暂存数据不参与统计；complete 在一个事务中核验数量、写入状态历史、更新事实、完成运行。数量不符或拉取失败时保留上次成功数据。同一来源只允许一个运行，一小时超时后允许新运行。写入需要 `DEVOPS:SYNC` 或 `PROJECT:UPDATE`；来源映射需要 `PROJECT:UPDATE`；看板需要项目、需求、缺陷的 READ 权限。给机器单独签发 `DEVOPS:SYNC` + `PROJECT:READ` API Key，不使用管理员密码。

## 当前部署

- 复用 `/home/liber/requirement-code-metrics/service.py` 的只读云效采集器，配置取自 `~/.config/devops-weekly/env`。不复制云效令牌进仓库。
- 共享采集器需包含 `scripts/yunxiao-collector.patch` 的兼容扩展：为 commitsDetail 增加 requirementIds，不改变原n8n统计和发送口径。
- `scripts/yunxiao_sync.py` 仅依赖Python标准库和共享采集器现有Pillow依赖；读取私有env文件。
- env内容：SHEPHERD_URL、SHEPHERD_SYNC_KEY、SHEPHERD_SOURCE_ID、SHEPHERD_COLLECTOR_PATH、SHEPHERD_SYNC_DAYS、SHEPHERD_TEST_REPO_IDS（显式指定用例库ID范围）。存放 `~/.config/shepherd-yunxiao/env`，权限600。
- `systemctl --user status shepherd-yunxiao-sync.timer` 检查每小时任务，`journalctl --user -u shepherd-yunxiao-sync.service` 查看结果。定时器不发送企业微信消息。
- 上次完整采集快照保存在 `~/.cache/shepherd-yunxiao/snapshot.json`，权限600，可重放同一快照验证幂等。

## 镜像构建

本次保留原v0.0.6运行镜像，仅替换应用二进制和静态资源。服务器构建复用Cargo缓存：

```
docker build --network host --build-context cargo-cache=/home/liber/.cargo/registry -f deploy/docker/Dockerfile.yunxiao -t shepherd-server:yunxiao-native-20261028 .
# 先在web目录npm ci && npm run build
docker build -f deploy/docker/Dockerfile.yunxiao-web -t shepherd-web:yunxiao-native-20261028 .
```

将docker-compose.yunxiao.yml作为现有compose文件的最后一层override，仅重建server/web，数据库和Redis保留。

## 验证

```
cargo test -p devops --all-features --locked
cargo clippy -p devops --all-features --all-targets --locked -- -D warnings -A clippy::unwrap-used
# 必须使用隔离测试数据库：
SHEPHERD_TEST_DATABASE_URL=... cargo test -p devops --all-features --test pg_sync --locked -- --ignored
python3 -m unittest discover -s scripts -p test_yunxiao_sync.py -v
cd web && npm ci && npm run build
```

数据库迁移0112新增事实表，0113为原生需求和功能用例新增外部来源标识；上线前备份数据库和原server/web镜像标签。回滚应用可恢复原镜像；新增外部事实表可保留，不影响原业务表。

## 原生页面与滚动修复（2026-10-28）

- 统计页使用限定视口高度的独立纵向滚动容器，适配外层overflow:hidden布局，保证底部需求/缺陷/迭代/提交/历史/记录标签可访问。
- 原生需求入口：`/p/31fa76e3-82ec-4ef3-8360-7d9f856df78c/requirement`。前端加载所有后端分页，避免200条截断；无验收标准的导入需求不逐条请求覆盖率。
- 原生用例入口：`/p/31fa76e3-82ec-4ef3-8360-7d9f856df78c/functional-case`，目录在「云效测试库」下按库/目录层级组织。
- 同步完成时在同一事务中发布外部事实和原生投影；数据量大时complete端点允许600秒，其他读取/批量暂存端点保留原超时。
- 用例详细内容缓存每500条写入检查点；全批次采集失败不会发布半份原生数据。
- 首次回填目标为1313需求、7693功能用例；其中源详情有4221条非空步骤、2928条前置条件，其他用例保持源数据真实空值。
- UI回归测试：设置私有SHEPHERD_E2E_AUTH_FILE与E2E_BASE_URL后执行 `npx playwright test yunxiao.spec.ts`，只读取已有项目，不创建/编辑测试资产。

- 用例目录统计使用单次索引和子树计数，避免7693条用例与750个模块反复嵌套遍历。
- Docker构建在编译前触碰migrate入口，保证新增SQL迁移重新嵌入二进制；线上确认迁移113已应用。
- 线上Chromium回归4项全部通过，覆盖720px视口滚动、超过首页的需求、原生用例详情和8000条数据目录索引。
