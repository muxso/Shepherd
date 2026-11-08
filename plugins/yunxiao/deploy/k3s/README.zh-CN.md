# 云效同步 CronJob

`shepherd/shepherd-yunxiao-sync` 每小时上海时间 15 分运行，禁止重叠，超时两小时，不自动重试。失败时保留最后一次成功快照。手动 Job 也必须先确认没有正在运行的采集；数据库来源锁和缓存文件锁仍然生效。

## 构建

构建上下文包含本目录的 Dockerfile、仓库内 `plugins/yunxiao/scripts/*.py`（`scripts/`），以及当前已验证的外部只读采集器（`collector/`）：`service.py`、`report_groups.py`、`report_image.py`、`report_weekly.py`、`report_selection.py`。外部采集器不在本仓库，发布时应记录每个文件的 SHA256，并先验证导入和配置。不得包含 env、凭据、缓存或日志。用固定基础镜像 digest 构建，将独立同步镜像推至私仓，替换清单中的 `SYNC_IMAGE_DIGEST`。

## 配置

Secret `shepherd-yunxiao-sync` 承载原同步环境及云效凭据。通过受保护文件在进程内读取并提交 Secret，不在终端打印。内部服务地址由清单覆盖，采集器路径固定在镜像中；缓存路径统一映射到 `/data/.cache/shepherd-yunxiao`。保留原 `SHEPHERD_SYNC_DAYS=90` 及测试库配置。

PVC 保存私有缓存和回放快照。仓库语言识别只读挂载已有代码缓存，因此清单需要固定到存有缓存的节点。应用前替换 `SYNC_NODE_NAME`，并将两处 `/path/to/code-cache` 改为该节点的实际缓存根目录。迁移节点时需要一起迁移仓库缓存、PVC 数据并修改节点选择；不挂载代码 RAG 的凭据目录。

## 首次切换

1. 备份原 systemd 配置和缓存，确认旧服务已结束，临时停止旧 timer。
2. 创建 Secret、PVC 和暂停的 CronJob。将旧私有缓存复制到 PVC 中，保持目录 700、文件 600，所有者 1000:1000。
3. 从 CronJob 创建手动验证 Job，检查完整采集、原子发布和数据库成功记录。
4. 验证成功后禁用旧 timer，将 CronJob 的 `suspend` 改为 `false`。发布清单默认暂停，避免首次应用就启动重复调度。
5. 失败则保持 CronJob 暂停，恢复旧 timer。回退前确认没有活动 Job；保留旧 unit 和环境文件。

## 运维

```sh
kubectl -n shepherd get cronjob shepherd-yunxiao-sync
kubectl -n shepherd get jobs --sort-by=.metadata.creationTimestamp
kubectl -n shepherd logs job/JOB_NAME
kubectl -n shepherd patch cronjob shepherd-yunxiao-sync --type=merge -p '{"spec":{"suspend":true}}'
```

暂停仅影响未来调度，不会停止已启动的 Job。修改周期、时区、资源或超时后重新应用清单，再按当前需要恢复调度。日志仅限内部运维访问，避免公开上游响应或业务快照。
