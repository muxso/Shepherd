# 多语言文档链接检查报告 / Documentation Link Check Report

- 生成日期 / Generated: **2026-08-09**
- 检查范围 / Scope: 12 个文档（README + `docs/*.md`，英文 + 简体中文）
- 链接总数 / Total links checked: **130**
- 有效 / Valid: **130**　无效 / Broken: **0**

## 结论 / Result

> ✅ 所有内部文档链接、跨语言切换、章节锚点与资源引用均有效。

## 跨语言配对完整性 / Cross-language pairing

| 文档组 / Group | 文件 | 中英齐全 / EN+ZH |
|---|---|---|
| `README` | `README.md`, `README.zh-CN.md` | ✅ |
| `docs/COMMENT_CONVENTIONS` | `COMMENT_CONVENTIONS.md`, `COMMENT_CONVENTIONS.zh-CN.md` | ✅ |
| `docs/DEPLOYMENT` | `DEPLOYMENT.md`, `DEPLOYMENT.zh-CN.md` | ✅ |
| `docs/EXECUTORS` | `EXECUTORS.md`, `EXECUTORS.zh-CN.md` | ✅ |
| `docs/INSTALL` | `INSTALL.md`, `INSTALL.zh-CN.md` | ✅ |
| `docs/USAGE` | `USAGE.md`, `USAGE.zh-CN.md` | ✅ |

## 各文档链接清单 / Link inventory

### README.md  (17 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 11 | doc | `README.zh-CN.md` | OK | 🔁 |
| 112 | doc | `docs/USAGE.md#5-configuration-environment-variables` | OK |  |
| 118 | doc | `docs/USAGE.md` | OK |  |
| 118 | doc | `docs/USAGE.zh-CN.md` | OK | 🔁 |
| 118 | doc | `docs/DEPLOYMENT.md` | OK |  |
| 118 | doc | `docs/DEPLOYMENT.zh-CN.md` | OK | 🔁 |
| 122 | doc | `docs/USAGE.md` | OK |  |
| 122 | doc | `docs/USAGE.zh-CN.md` | OK | 🔁 |
| 123 | doc | `docs/DEPLOYMENT.md` | OK |  |
| 123 | doc | `docs/DEPLOYMENT.zh-CN.md` | OK | 🔁 |
| 124 | doc | `docs/INSTALL.md` | OK |  |
| 124 | doc | `docs/INSTALL.zh-CN.md` | OK | 🔁 |
| 125 | doc | `docs/EXECUTORS.md` | OK |  |
| 125 | doc | `docs/EXECUTORS.zh-CN.md` | OK | 🔁 |
| 126 | doc | `docs/COMMENT_CONVENTIONS.md` | OK |  |
| 126 | doc | `docs/COMMENT_CONVENTIONS.zh-CN.md` | OK | 🔁 |
| 159 | file | `LICENSE` | OK |  |

### README.zh-CN.md  (17 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 11 | doc | `README.md` | OK | 🔁 |
| 115 | doc | `docs/USAGE.zh-CN.md#5-配置环境变量` | OK |  |
| 121 | doc | `docs/USAGE.zh-CN.md` | OK |  |
| 121 | doc | `docs/USAGE.md` | OK | 🔁 |
| 121 | doc | `docs/DEPLOYMENT.zh-CN.md` | OK |  |
| 121 | doc | `docs/DEPLOYMENT.md` | OK | 🔁 |
| 125 | doc | `docs/USAGE.zh-CN.md` | OK |  |
| 125 | doc | `docs/USAGE.md` | OK | 🔁 |
| 126 | doc | `docs/DEPLOYMENT.zh-CN.md` | OK |  |
| 126 | doc | `docs/DEPLOYMENT.md` | OK | 🔁 |
| 127 | doc | `docs/INSTALL.zh-CN.md` | OK |  |
| 127 | doc | `docs/INSTALL.md` | OK | 🔁 |
| 128 | doc | `docs/EXECUTORS.zh-CN.md` | OK |  |
| 128 | doc | `docs/EXECUTORS.md` | OK | 🔁 |
| 129 | doc | `docs/COMMENT_CONVENTIONS.zh-CN.md` | OK |  |
| 129 | doc | `docs/COMMENT_CONVENTIONS.md` | OK | 🔁 |
| 162 | file | `LICENSE` | OK |  |

### docs/COMMENT_CONVENTIONS.md  (1 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `COMMENT_CONVENTIONS.zh-CN.md` | OK | 🔁 |

### docs/COMMENT_CONVENTIONS.zh-CN.md  (1 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `COMMENT_CONVENTIONS.md` | OK | 🔁 |

### docs/DEPLOYMENT.md  (21 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `DEPLOYMENT.zh-CN.md` | OK | 🔁 |
| 7 | doc | `./USAGE.md` | OK |  |
| 12 | anchor(same-file) | `#topology` | OK |  |
| 13 | anchor(same-file) | `#building-images` | OK |  |
| 14 | anchor(same-file) | `#local-stack-docker-compose` | OK |  |
| 15 | anchor(same-file) | `#kubernetes-via-helm` | OK |  |
| 16 | anchor(same-file) | `#multi-cloud-terraform` | OK |  |
| 17 | anchor(same-file) | `#cicd-auto-deploy` | OK |  |
| 18 | anchor(same-file) | `#day-2-operations` | OK |  |
| 74 | file | `../deploy/docker/` | OK |  |
| 107 | anchor(same-file) | `#cicd-auto-deploy` | OK |  |
| 114 | file | `../deploy/docker/docker-compose.yml` | OK |  |
| 147 | doc | `./USAGE.md` | OK |  |
| 153 | file | `../deploy/helm/shepherd/` | OK |  |
| 197 | file | `../deploy/helm/shepherd/values.yaml` | OK |  |
| 277 | file | `../deploy/terraform/` | OK |  |
| 285 | file | `../deploy/terraform/aws/` | OK |  |
| 286 | file | `../deploy/terraform/gcp/` | OK |  |
| 287 | file | `../deploy/terraform/azure/` | OK |  |
| 345 | file | `../.github/workflows/` | OK |  |
| 514 | doc | `./USAGE.md` | OK |  |

### docs/DEPLOYMENT.zh-CN.md  (21 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `DEPLOYMENT.md` | OK | 🔁 |
| 6 | doc | `./USAGE.zh-CN.md` | OK |  |
| 11 | anchor(same-file) | `#topology` | OK |  |
| 12 | anchor(same-file) | `#building-images` | OK |  |
| 13 | anchor(same-file) | `#local-stack-docker-compose` | OK |  |
| 14 | anchor(same-file) | `#kubernetes-via-helm` | OK |  |
| 15 | anchor(same-file) | `#multi-cloud-terraform` | OK |  |
| 16 | anchor(same-file) | `#cicd-auto-deploy` | OK |  |
| 17 | anchor(same-file) | `#day-2-operations` | OK |  |
| 69 | file | `../deploy/docker/` | OK |  |
| 101 | anchor(same-file) | `#cicd-auto-deploy` | OK |  |
| 108 | file | `../deploy/docker/docker-compose.yml` | OK |  |
| 140 | doc | `./USAGE.zh-CN.md` | OK |  |
| 146 | file | `../deploy/helm/shepherd/` | OK |  |
| 188 | file | `../deploy/helm/shepherd/values.yaml` | OK |  |
| 262 | file | `../deploy/terraform/` | OK |  |
| 269 | file | `../deploy/terraform/aws/` | OK |  |
| 270 | file | `../deploy/terraform/gcp/` | OK |  |
| 271 | file | `../deploy/terraform/azure/` | OK |  |
| 326 | file | `../.github/workflows/` | OK |  |
| 483 | doc | `./USAGE.zh-CN.md` | OK |  |

### docs/EXECUTORS.md  (17 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `EXECUTORS.zh-CN.md` | OK | 🔁 |
| 7 | doc | `./USAGE.md` | OK |  |
| 7 | doc | `./DEPLOYMENT.md` | OK |  |
| 9 | anchor(same-file) | `#how-dispatch-reaches-a-cli` | OK |  |
| 10 | anchor(same-file) | `#common-runtime-configuration` | OK |  |
| 11 | anchor(same-file) | `#claude-code` | OK |  |
| 12 | anchor(same-file) | `#codex` | OK |  |
| 13 | anchor(same-file) | `#opencode` | OK |  |
| 14 | anchor(same-file) | `#codebuddy` | OK |  |
| 15 | anchor(same-file) | `#one-runtime-multiple-executors` | OK |  |
| 16 | anchor(same-file) | `#mock-executor-no-cli` | OK |  |
| 17 | anchor(same-file) | `#windows` | OK |  |
| 18 | anchor(same-file) | `#docker-and-host-clis` | OK |  |
| 19 | anchor(same-file) | `#shared-checkout-different-branches` | OK |  |
| 20 | anchor(same-file) | `#dispatching-to-a-specific-executor` | OK |  |
| 21 | anchor(same-file) | `#pool-runners-scenario-execution` | OK |  |
| 22 | anchor(same-file) | `#troubleshooting` | OK |  |

### docs/EXECUTORS.zh-CN.md  (17 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `EXECUTORS.md` | OK | 🔁 |
| 6 | doc | `./USAGE.zh-CN.md` | OK |  |
| 7 | doc | `./DEPLOYMENT.zh-CN.md` | OK |  |
| 9 | anchor(same-file) | `#派发如何到达-cli` | OK |  |
| 10 | anchor(same-file) | `#runtime-通用配置` | OK |  |
| 11 | anchor(same-file) | `#claude-code` | OK |  |
| 12 | anchor(same-file) | `#codex` | OK |  |
| 13 | anchor(same-file) | `#opencode` | OK |  |
| 14 | anchor(same-file) | `#codebuddy` | OK |  |
| 15 | anchor(same-file) | `#一个-runtime-承接多种执行者` | OK |  |
| 16 | anchor(same-file) | `#mock-执行者无需-cli` | OK |  |
| 17 | anchor(same-file) | `#windows` | OK |  |
| 18 | anchor(same-file) | `#docker-与宿主机-cli` | OK |  |
| 19 | anchor(same-file) | `#共用检出不同分支` | OK |  |
| 20 | anchor(same-file) | `#指定执行者派发` | OK |  |
| 21 | anchor(same-file) | `#资源池执行机场景执行` | OK |  |
| 22 | anchor(same-file) | `#排障` | OK |  |

### docs/INSTALL.md  (4 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `INSTALL.zh-CN.md` | OK | 🔁 |
| 5 | doc | `DEPLOYMENT.md` | OK |  |
| 5 | file | `../deploy/docker/docker-compose.ghcr.yml` | OK |  |
| 124 | doc | `EXECUTORS.md` | OK |  |

### docs/INSTALL.zh-CN.md  (4 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `INSTALL.md` | OK | 🔁 |
| 5 | doc | `DEPLOYMENT.zh-CN.md` | OK |  |
| 5 | file | `../deploy/docker/docker-compose.ghcr.yml` | OK |  |
| 124 | doc | `EXECUTORS.zh-CN.md` | OK |  |

### docs/USAGE.md  (5 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `USAGE.zh-CN.md` | OK | 🔁 |
| 5 | doc | `DEPLOYMENT.md` | OK |  |
| 89 | doc | `DEPLOYMENT.md` | OK |  |
| 282 | doc | `./EXECUTORS.md` | OK |  |
| 356 | doc | `DEPLOYMENT.md` | OK |  |

### docs/USAGE.zh-CN.md  (5 链接)

| 行 | 类型 | 目标 | 状态 | 跨语言 |
|---|---|---|---|---|
| 3 | doc | `USAGE.md` | OK | 🔁 |
| 5 | doc | `DEPLOYMENT.zh-CN.md` | OK |  |
| 89 | doc | `DEPLOYMENT.zh-CN.md` | OK |  |
| 278 | doc | `./EXECUTORS.zh-CN.md` | OK |  |
| 352 | doc | `DEPLOYMENT.zh-CN.md` | OK |  |

## 说明 / Notes

- 检查方式 / Method: 移除 HTML 注释后提取 Markdown 链接，对文件目标做存在性校验，对 `.md` 目标按 GitHub 风格 slug（小写、保留字母/数字/连字符、空格转 `-`、丢弃 `。、()#` 等标点）校验章节锚点。
- 外部链接（http/https/mailto）与注释中的占位链接（如 `docs/assets/demo.gif`）未计入有效/无效判定。
- 所有文档顶部均已统一语言切换导航 `[简体中文](x.zh-CN.md) · English` / `简体中文 · [English](x.md)`。
