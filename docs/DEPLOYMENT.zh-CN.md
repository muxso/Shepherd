# Shepherd — 部署与运维

本指南涵盖镜像构建、本地运行、通过 Helm 部署到 Kubernetes、使用 Terraform 预置云基础设施、
CI/CD 流水线以及 Day-2 运维。终端用户与 API 使用方式见 [USAGE.zh-CN.md](./USAGE.zh-CN.md)。

Shepherd 是一个 Rust workspace（`crates/`）加一个 Vite/React SPA（`web/`），交付为三个镜像：
**shepherd-server**、**shepherd-agent-runtime** 和 **shepherd-web**。

- [拓扑](#topology)
- [构建镜像](#building-images)
- [本地栈（docker-compose）](#local-stack-docker-compose)
- [通过 Helm 部署到 Kubernetes](#kubernetes-via-helm)
- [多云 Terraform](#multi-cloud-terraform)
- [CI/CD 自动部署](#cicd-auto-deploy)
- [Day-2 运维](#day-2-operations)

---

## 拓扑 {#topology}

```
                          公网 / 用户
                                 │
                                 ▼
                  ┌──────────────────────────────┐
                  │  shepherd-web (nginx:1.27)    │   :80
                  │  静态 SPA + 反向代理           │
                  │  Accept: text/html → SPA      │
                  │  其它 → proxy_pass server      │
                  └───────────────┬──────────────┘
                                  │ /api /auth /project /organization …
                                  ▼
                  ┌──────────────────────────────┐
                  │  shepherd-server  （公网）     │   :8088
                  │  GET /healthz  → 200 ok       │   （存活）
                  │  GET /readyz   → 200/503      │   （就绪，检测 PG）
                  │  启动时自动执行数据库迁移        │
                  └───┬───────────────────────┬──┘
                      │                        │
        长轮询  ───────┘                        ├──────────────┐
       （仅出站，无入站端口）                      ▼              ▼
                  ▲                    ┌──────────────┐  ┌──────────────┐
                  │                    │ PostgreSQL 16│  │  Redis 7     │
      ┌───────────┴──────────────┐    │  （必需）     │  │（仅多机群需要）│
      │ shepherd-agent-runtime   │    └──────────────┘  └──────────────┘
      │ （1..N 副本，无端口）       │
      │ 通过长轮询拉取任务           │
      └──────────────────────────┘
```

关键特性：

- **shepherd-web** 是浏览器唯一对接的 HTTP 入口。nginx 提供静态 SPA，并按 `Accept` 头将后端
  前缀反向代理到 server：`Accept: text/html` 的请求返回 SPA（`/index.html`），其余全部代理到
  `shepherd-server`。
- **shepherd-server** 面向公网，暴露健康检查端点，并在**启动时自动执行数据库迁移**——无需独立的
  迁移任务。
- **shepherd-agent-runtime** **没有任何入站端口**，仅向 server 发起出站长轮询以拉取任务。可水平
  扩展，可安全置于仅出站网络的私有子网中。
- **PostgreSQL 16** 始终必需。**Redis 7** 仅在多机群（multi-host fleet）时必需；单 server 副本
  使用进程内队列，可省略 Redis。

---

## 构建镜像 {#building-images}

镜像源位于 [`deploy/docker/`](../deploy/docker/)，包含三个 Dockerfile：

| 镜像 | 构建基础镜像 | 运行基础镜像 | 端口 |
|------|------------|------------|------|
| `shepherd-server` | `rust:1.86-bookworm` | `debian:bookworm-slim` + `ca-certificates libssl3` | 8088 |
| `shepherd-agent-runtime` | `rust:1.86-bookworm` | `debian:bookworm-slim` + `ca-certificates libssl3 git` | 无 |
| `shepherd-web` | `node:18-alpine` | `nginx:1.27-alpine` | 80 |

Rust 镜像以 `cargo build --release -p server` / `-p agent-runtime` 构建 workspace，并使用
cargo 构建缓存分层。web 镜像执行 `npm ci && npm run build`，通过 nginx 提供 `web/dist`
（`nginx.conf` 实现上述基于 `Accept` 的代理）。

使用 Buildx 在本地构建三个镜像：

```bash
export OWNER=muxso          # GHCR owner / 组织
export TAG=$(git rev-parse --short HEAD)

docker build -f deploy/docker/Dockerfile.server        -t ghcr.io/$OWNER/shepherd-server:$TAG        .
docker build -f deploy/docker/Dockerfile.agent-runtime -t ghcr.io/$OWNER/shepherd-agent-runtime:$TAG .
docker build -f deploy/docker/Dockerfile.web           -t ghcr.io/$OWNER/shepherd-web:$TAG           .
```

推送到 GHCR（或任意 registry）：

```bash
echo "$GITHUB_TOKEN" | docker login ghcr.io -u "$OWNER" --password-stdin
docker push ghcr.io/$OWNER/shepherd-server:$TAG
docker push ghcr.io/$OWNER/shepherd-agent-runtime:$TAG
docker push ghcr.io/$OWNER/shepherd-web:$TAG
```

> CI 中此过程完全自动化——见 [CI/CD 自动部署](#cicd-auto-deploy)。

---

## 本地栈（docker-compose） {#local-stack-docker-compose}

在单机上运行整套系统最快的方式。
[`deploy/docker/docker-compose.yml`](../deploy/docker/docker-compose.yml) 会启动
PostgreSQL、Redis、server、一个 agent-runtime 和 web 前端。

```bash
cd deploy/docker
docker compose up -d --build
```

随后打开 <http://localhost:8080>（web → nginx），以 `admin` 登录，密码取自
`SHEPHERD_ADMIN_PASSWORD`。

compose 栈所连接的内容：

- `postgres`（PostgreSQL 16），带命名卷；`DATABASE_URL` 指向它。
- `redis`（Redis 7），用于机群队列。
- `server`——暴露 `8088`，设置 `DATABASE_URL`、`SHEPHERD_FLEET_REDIS`、
  `SHEPHERD_ADMIN_PASSWORD`。首次启动自动执行迁移；PG 可达后就绪检查转为 200。
- `agent-runtime`——无端口；`SHEPHERD_BASE=http://server:8088`，并设 `AGENT_MOCK=1`，
  无需真实 agent CLI 即可运行。设置 `AGENT_MOCK=0` 并挂载 `git`/`claude`/`codex` 二进制可驱动
  真实后端。
- `web`——nginx 监听 `8080:80`，反向代理到 `server`。

销毁（加 `-v` 同时删除数据库卷）：

```bash
docker compose down        # 保留数据
docker compose down -v     # 清空数据
```

这是推荐的**开发路径**。配置细节（环境变量表、机群配置、注册真实 agent 后端）见
[USAGE.zh-CN.md](./USAGE.zh-CN.md)。

---

## 通过 Helm 部署到 Kubernetes {#kubernetes-via-helm}

Chart 位于 [`deploy/helm/shepherd/`](../deploy/helm/shepherd/)（`apiVersion: v2`，
`appVersion: 0.0.1`）。它渲染 server/agent-runtime/web 的 Deployment + Service、server 与
web 的 Ingress、可选 HPA 与 server 的 PodDisruptionBudget、一个共享 Secret
（`DATABASE_URL`、`SHEPHERD_ADMIN_PASSWORD`、`SHEPHERD_FLEET_REDIS`、OIDC）以及一个非敏感
ConfigMap（含 nginx 配置）。探针接到 `/healthz`（存活）与 `/readyz`（就绪）。

### 安装 / 升级

```bash
helm lint deploy/helm/shepherd

# 开发 / 演示：集群内 PostgreSQL + Redis，mock agent，无外部 DB
helm upgrade --install shepherd deploy/helm/shepherd \
  -n shepherd --create-namespace \
  -f deploy/helm/shepherd/values-dev.yaml

# 生产：外部托管 DB/Redis，真实镜像，ingress + TLS
helm upgrade --install shepherd deploy/helm/shepherd \
  -n shepherd --create-namespace \
  -f deploy/helm/shepherd/values-prod.yaml \
  --set global.image.registry=ghcr.io/muxso \
  --set global.image.tag=v0.0.1 \
  --set config.adminPassword="$SHEPHERD_ADMIN_PASSWORD" \
  --set database.url="$DATABASE_URL" \
  --set config.fleet.redisUrl="$REDIS_URL"
```

检查发布状态：

```bash
kubectl -n shepherd rollout status deploy/shepherd-server
kubectl -n shepherd get pods,svc,ingress
```

### 关键 values

完整键列表见 [`values.yaml`](../deploy/helm/shepherd/values.yaml)。要点：

```yaml
global:
  image: { registry: ghcr.io/muxso, tag: latest, pullPolicy: IfNotPresent }
server:
  replicas: 2
  service: { type: ClusterIP, port: 8088 }
  ingress: { enabled: false, className: "", host: shepherd.example.com, tls: false }
  autoscaling: { enabled: false, minReplicas: 2, maxReplicas: 6, targetCPUUtilizationPercentage: 70 }
agentRuntime:
  replicas: 3
  mock: false                                   # AGENT_MOCK
  autoscaling: { enabled: true, minReplicas: 1, maxReplicas: 10, targetCPUUtilizationPercentage: 70 }
web:
  replicas: 2
  service: { type: ClusterIP, port: 80 }
  ingress: { enabled: true, className: "", host: shepherd.example.com, tls: false }
config:
  adminPassword: ""                             # → Secret SHEPHERD_ADMIN_PASSWORD（必需）
  sessionTtlSecs: 28800
  fleet: { enabled: true, redisUrl: "" }        # SHEPHERD_FLEET_REDIS
  oidc: { feishu: {...}, wecom: {...} }
database:
  url: ""                                       # DATABASE_URL → Secret（除非 postgresql.enabled 否则必需）
postgresql: { enabled: false }                  # 集群内 PG（仅 dev/demo）
redis: { enabled: false }                       # 集群内 Redis（仅 dev/demo）
```

### 开发 vs 生产 values

- **`values-dev.yaml`**——`postgresql.enabled: true`、`redis.enabled: true`、
  `agentRuntime.mock: true`、无 server ingress。自包含，无需任何外部依赖。集群内
  PostgreSQL/Redis 来自带条件的 Bitnami 子 chart 依赖，**仅用于 dev/demo**——切勿用作生产数据存储。
- **`values-prod.yaml`**——将 `database.url` 与 `config.fleet.redisUrl` 指向托管服务，使用真实
  镜像，开启 ingress 与 TLS，关闭 mock。作为起始模板使用。

### Ingress 与 TLS

在 web（浏览器入口）以及可选的 server 上启用 ingress：

```yaml
web:
  ingress:
    enabled: true
    className: nginx            # 或按需 alb / gce
    host: shepherd.example.com
    tls: true
    annotations:
      cert-manager.io/cluster-issuer: letsencrypt-prod
```

当 `tls: true` 时，chart 会添加引用 `<host>-tls` Secret 的 TLS 块。配合 cert-manager（上述
注解）或自行提供证书 Secret。在云 LB ingress class（`alb`、`gce`）上请遵循对应控制器的注解约定。

### 扩缩容与 HPA

- **静态**：调整 `server.replicas` / `agentRuntime.replicas` / `web.replicas`。
- **自动扩缩**：设置 `autoscaling.enabled: true`。agent-runtime 的 HPA 默认开启
  （`1..10`，70% CPU）——机群是弹性层，随负载伸缩。server 的 HPA 默认关闭，可在 API 流量突发时开启。
- `PodDisruptionBudget` 在节点排空期间保持 server 可用。

### 密钥处理

密钥绝不写入镜像或提交到仓库。在安装时通过 `--set`（来自 shell/CI 环境）提供，或使用预创建 Secret +
`existingSecret` 模式。`config.adminPassword` 与 `database.url` 在生产中**必需**，会写入 chart
Secret 的 `SHEPHERD_ADMIN_PASSWORD` 与 `DATABASE_URL`。生产部署中应优先使用密钥管理工具（External
Secrets Operator、SealedSecrets、云端密钥库），而非明文 `--set`。

---

## 多云 Terraform {#multi-cloud-terraform}

[`deploy/terraform/`](../deploy/terraform/) 下的 Terraform 预置云基础设施，随后在新建集群上安装
Helm chart。每个云目录预置一个托管 Kubernetes 集群、一个 PostgreSQL 16 服务、一个 Redis 7 缓存、
一个容器 registry 和一个 ingress class，然后调用云无关的 `modules/shepherd-app` 模块来
`helm install` chart。

| 目录 | Kubernetes | PostgreSQL | Redis | Registry | Ingress class |
|-----|-----------|-----------|-------|----------|---------------|
| [`aws/`](../deploy/terraform/aws/) | EKS + 托管节点组 | RDS PostgreSQL 16 | ElastiCache Redis 7 | ECR（×3） | `alb`（AWS LB Controller） |
| [`gcp/`](../deploy/terraform/gcp/) | GKE | Cloud SQL PostgreSQL 16 | Memorystore Redis | Artifact Registry | GCE |
| [`azure/`](../deploy/terraform/azure/) | AKS | Azure Database for PostgreSQL Flexible Server | Azure Cache for Redis | ACR | （集群 ingress） |

`modules/shepherd-app` 是云无关的：给定已配置的 `helm` + `kubernetes` provider 及 DB/Redis
端点，它创建 `helm_release`（`chart = "../../helm/shepherd"`），传入 registry、tag、密钥、host 与
副本数。该模块**不含任何 provider 块**——由云目录传入已配置好的 provider。

### init / fmt / validate / plan / apply

在所选云目录中运行，例如 `deploy/terraform/aws`：

```bash
cd deploy/terraform/aws
cp terraform.tfvars.example terraform.tfvars   # 然后编辑
# 填入：region、集群规格、db_password、admin_password、image_tag、hosts …

terraform init
terraform fmt -check
terraform validate
terraform plan -out tfplan
terraform apply tfplan
```

> **未在仓库内验证。** 本仓库没有 Terraform 二进制、云凭证或 state。**务必**先运行
> `init` / `fmt` / `validate` / `plan` 并审阅 plan，再执行 `apply`。敏感变量（`database_url`、
> `redis_url`、`admin_password`、数据库密码）标记为 `sensitive = true`；通过 tfvars、`TF_VAR_*`
> 或密钥后端提供——切勿提交。

### 输出

每个云目录导出相同的 output：

| Output | 用途 |
|--------|------|
| `cluster_name` | 已预置集群名 |
| `kubeconfig_command` | 获取 kubeconfig 的单行命令（如 `aws eks update-kubeconfig …`） |
| `database_url`（敏感） | chart 使用的连接串 |
| `redis_url`（敏感） | 机群 Redis URL |
| `registry_url` | 推送三个镜像的位置 |
| `app_url` | web 前端的公网 URL |

apply 之后：

```bash
$(terraform output -raw kubeconfig_command)
kubectl -n shepherd get pods
echo "App: $(terraform output -raw app_url)"
```

每个云目录都有自己的 `README.md`，说明 provider 版本（`versions.tf`、Terraform `>= 1.5`、已固定
版本的 provider）与前置条件。

---

## CI/CD 自动部署 {#cicd-auto-deploy}

[`.github/workflows/`](../.github/workflows/) 下两个 GitHub Actions 工作流。

### `images.yml`——构建与推送镜像

- **触发**：推送 `v*` tag，或 `workflow_dispatch`。
- 以矩阵构建 `[server, agent-runtime, web]`，使用 `docker/build-push-action` + Buildx，以
  `${{ github.repository_owner }}` 用内置 `GITHUB_TOKEN` 登录 GHCR，并将各镜像以 git tag 与提交
  `sha` 打标后推送。

无需额外密钥——`GITHUB_TOKEN` 即可覆盖 GHCR。

### `deploy.yml`——部署到云

- **触发**：`workflow_dispatch`（输入 `cloud` = `aws|gcp|azure`、`environment`、`image_tag`）
  与 `release: published`。**由 release 驱动，绝不由 push 驱动**——合并到分支不会触发部署。
- 运行于 GitHub **Environment**，借此将作业置于**人工审批**门后（以及你配置的任何环境保护规则 /
  必需审阅者）。
- 通过 **OIDC** 认证到云（无长期云密钥）：`aws-actions/configure-aws-credentials`、
  `google-github-actions/auth` 或 `azure/login`。
- 获取目标集群的 kubeconfig，然后运行：

  ```bash
  helm upgrade --install shepherd deploy/helm/shepherd \
    -f deploy/helm/shepherd/values-<env>.yaml \
    --set global.image.tag=<image_tag> \
    --set config.adminPassword=$SHEPHERD_ADMIN_PASSWORD \
    --set database.url=$DATABASE_URL \
    --set config.fleet.redisUrl=$REDIS_URL
  ```

### 所需密钥 / 变量

将以下项设置为 GitHub 仓库或 **Environment** 密钥（云相关项作用域限定到对应环境）：

**应用（所有云）：**

| Secret | 用途 |
|--------|------|
| `SHEPHERD_ADMIN_PASSWORD` | 管理员登录 + agent-runtime 认证 |
| `DATABASE_URL` | `postgres://…` 连接串 |
| `REDIS_URL` | 机群 Redis URL |

**AWS（OIDC）：**

| Secret | 用途 |
|--------|------|
| `AWS_ROLE_ARN` | 通过 OIDC 承担的 IAM 角色 |
| `AWS_REGION` | 区域 |
| `EKS_CLUSTER` | 用于 `update-kubeconfig` 的集群名 |

**GCP（Workload Identity Federation）：**

| Secret | 用途 |
|--------|------|
| `GCP_WORKLOAD_IDENTITY_PROVIDER` | WIF provider 资源 |
| `GCP_SA` | 待模拟的服务账号 |
| `GKE_CLUSTER` | 集群名 |
| `GKE_ZONE` | 集群 zone/region |
| `GCP_PROJECT` | 项目 ID |

**Azure（OIDC）：**

| Secret | 用途 |
|--------|------|
| `AZURE_CLIENT_ID` | 应用注册 / 联合身份 |
| `AZURE_TENANT_ID` | 租户 |
| `AZURE_SUBSCRIPTION_ID` | 订阅 |
| `AKS_CLUSTER` | 集群名 |
| `AKS_RG` | 资源组 |

所需密钥同样在 `deploy.yml` 顶部的注释块中记录。

---

## Day-2 运维 {#day-2-operations}

### 数据库迁移

迁移在 **server 启动时自动执行**——没有独立的迁移步骤。因此滚动更新会在新 server pod 起来时应用待处理
迁移；请保持迁移向后兼容，使新旧 pod 在滚动期间可共存。就绪检查（`/readyz`）在 PostgreSQL 不可达时返回
503，因此只有 pod 的 DB 连接（与迁移）成功后才会接收流量。

### 备份

PostgreSQL 是唯一可信数据源——务必备份。

- **托管（RDS / Cloud SQL / Azure DB）**：在 Terraform/控制台中启用自动备份 + 时间点恢复并设置保留窗口。
- **自托管**：定时执行 `pg_dump`/`pg_basebackup`，例如：

  ```bash
  pg_dump "$DATABASE_URL" | gzip > shepherd-$(date +%F).sql.gz
  ```

Redis 是临时的机群队列（仅多机群时存在），无需备份；丢失它会重新入队在飞任务，而不会丢失已提交数据。

### 可观测性

- **存活**：`GET /healthz` → `200 ok`。
- **就绪**：`GET /readyz` → PostgreSQL 可达时 `200`，否则 `503`。接到 Kubernetes 就绪探针。
- **机群统计**：`GET /agent/work/stats` 报告 agent 机群各能力的积压 / 在飞计数——用它观察队列深度并合理
  调整副本数。

```bash
curl -fsS https://shepherd.example.com/healthz
curl -fsS https://shepherd.example.com/readyz
curl -fsS -u admin:$SHEPHERD_ADMIN_PASSWORD https://shepherd.example.com/agent/work/stats
```

### 扩缩机群

agent-runtime 是弹性层。按副本或 HPA 扩缩：

```bash
kubectl -n shepherd scale deploy/shepherd-agent-runtime --replicas=8
# 或依赖 HPA（默认 1..10，70% CPU）
```

观察 `GET /agent/work/stats`：持续积压 → 增加副本（或调高 `agentRuntime.autoscaling.maxReplicas`）；
长期空闲 → 缩容。由于 runtime 仅出站，增加副本无需改动 ingress/网络。

### 单机 vs 多机 Redis

- **单 server 副本**：省略 Redis——server 使用进程内队列（`config.fleet.enabled: false` / 无
  `redisUrl`）。
- **多 server 副本（HA）**：**必需**共享的 **Redis 7**，使所有副本看到同一个机群队列
  （`config.fleet.enabled: true`、`config.fleet.redisUrl`）。失速任务的回收/重领由
  `SHEPHERD_FLEET_REAP_INTERVAL_S` 与 `SHEPHERD_FLEET_RECLAIM_MS` 控制。

### 轮换管理员密码与密钥

```bash
helm upgrade shepherd deploy/helm/shepherd -n shepherd --reuse-values \
  --set config.adminPassword="$NEW_PASSWORD"
kubectl -n shepherd rollout restart deploy/shepherd-server deploy/shepherd-agent-runtime
```

请同时轮换 agent-runtime 与 server，使两端共享新凭证（runtime 使用
`SHEPHERD_ADMIN_USER` / `SHEPHERD_ADMIN_PASSWORD` 认证）。`DATABASE_URL` /
`SHEPHERD_FLEET_REDIS` 同理轮换。若使用密钥管理工具，更新后端密钥并重启相应 deployment。

### 零停机滚动更新

部署默认即为滚动更新。发布新镜像：

```bash
helm upgrade shepherd deploy/helm/shepherd -n shepherd --reuse-values \
  --set global.image.tag=v0.0.2
kubectl -n shepherd rollout status deploy/shepherd-server
```

当 `server.replicas >= 2` 时，就绪探针 + PodDisruptionBudget 保证全程至少有一个 server 在服务。
新 pod 仅在 `/readyz` 通过（DB 可达、迁移已应用）后才接收流量。用 `helm rollback shepherd` 回滚。

---

应用配置、web UI 导览、机群/agent 配置及 HTTP/MCP API 使用见 [USAGE.zh-CN.md](./USAGE.zh-CN.md)。
