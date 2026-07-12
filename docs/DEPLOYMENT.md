# Shepherd — Deployment & Operations

This guide covers building images, running locally, deploying to Kubernetes via Helm,
provisioning cloud infrastructure with Terraform, the CI/CD pipeline, and Day-2
operations. For end-user and API usage see [USAGE.md](./USAGE.md).

Shepherd is a Rust workspace (`crates/`) plus a Vite/React SPA (`web/`). It ships as
three images: **shepherd-server**, **shepherd-agent-runtime**, and **shepherd-web**.

- [Topology](#topology)
- [Building images](#building-images)
- [Local stack (docker-compose)](#local-stack-docker-compose)
- [Kubernetes via Helm](#kubernetes-via-helm)
- [Multi-cloud Terraform](#multi-cloud-terraform)
- [CI/CD auto-deploy](#cicd-auto-deploy)
- [Day-2 operations](#day-2-operations)

---

## Topology

```
                          Internet / Users
                                 │
                                 ▼
                  ┌──────────────────────────────┐
                  │  shepherd-web (nginx:1.27)    │   :80
                  │  static SPA + reverse proxy   │
                  │  Accept: text/html → SPA      │
                  │  else → proxy_pass server     │
                  └───────────────┬──────────────┘
                                  │ /api /auth /project /organization …
                                  ▼
                  ┌──────────────────────────────┐
                  │  shepherd-server  (PUBLIC)    │   :8088
                  │  GET /healthz  → 200 ok       │   (liveness)
                  │  GET /readyz   → 200/503      │   (readiness, checks PG)
                  │  auto-runs DB migrations on   │
                  │  startup                      │
                  └───┬───────────────────────┬──┘
                      │                        │
        long-poll ────┘                        ├──────────────┐
        (outbound only,                        ▼              ▼
         NO inbound ports)            ┌──────────────┐  ┌──────────────┐
                  ▲                    │ PostgreSQL 16│  │  Redis 7     │
                  │                    │  (required)  │  │ (fleet only) │
      ┌───────────┴──────────────┐    └──────────────┘  └──────────────┘
      │ shepherd-agent-runtime   │
      │ (1..N replicas, no ports)│
      │ pulls work via long-poll │
      └──────────────────────────┘
```

Key properties:

- **shepherd-web** is the only HTTP entry point a browser talks to. nginx serves the
  static SPA and reverse-proxies backend prefixes to the server, disambiguating by the
  `Accept` header: `Accept: text/html` requests get the SPA (`/index.html`), everything
  else is proxied to `shepherd-server`.
- **shepherd-server** is public-facing, exposes health endpoints, and **runs database
  migrations on startup** — no separate migration job is needed.
- **shepherd-agent-runtime** has **no inbound ports**. It only makes outbound
  long-poll requests to the server to pull work. It is horizontally scalable and safe
  to place in a private subnet behind egress-only networking.
- **PostgreSQL 16** is always required. **Redis 7** is required only for a multi-host
  fleet; a single server replica uses an in-process queue and can omit Redis.

---

## Building images

Image sources live in [`deploy/docker/`](../deploy/docker/). Three Dockerfiles:

| Image | Build base | Runtime base | Port |
|-------|-----------|--------------|------|
| `shepherd-server` | `rust:1.86-bookworm` | `debian:bookworm-slim` + `ca-certificates libssl3` | 8088 |
| `shepherd-agent-runtime` | `rust:1.86-bookworm` | `debian:bookworm-slim` + `ca-certificates libssl3 git` | none |
| `shepherd-web` | `node:18-alpine` | `nginx:1.27-alpine` | 80 |

The Rust images build the workspace with `cargo build --release -p server` /
`-p agent-runtime` and use cargo build-cache layering. The web image runs
`npm ci && npm run build` and serves `web/dist` through nginx (`nginx.conf` implements
the `Accept`-based proxy described above).

Build all three locally with Buildx:

```bash
export OWNER=muxso          # GHCR owner / org
export TAG=$(git rev-parse --short HEAD)

docker build -f deploy/docker/Dockerfile.server        -t ghcr.io/$OWNER/shepherd-server:$TAG        .
docker build -f deploy/docker/Dockerfile.agent-runtime -t ghcr.io/$OWNER/shepherd-agent-runtime:$TAG .
docker build -f deploy/docker/Dockerfile.web           -t ghcr.io/$OWNER/shepherd-web:$TAG           .
```

Push to GHCR (or any registry):

```bash
echo "$GITHUB_TOKEN" | docker login ghcr.io -u "$OWNER" --password-stdin
docker push ghcr.io/$OWNER/shepherd-server:$TAG
docker push ghcr.io/$OWNER/shepherd-agent-runtime:$TAG
docker push ghcr.io/$OWNER/shepherd-web:$TAG
```

> In CI this is fully automated — see [CI/CD auto-deploy](#cicd-auto-deploy).

---

## Local stack (docker-compose)

The fastest way to run the whole system on one host. The compose file in
[`deploy/docker/docker-compose.yml`](../deploy/docker/docker-compose.yml) starts
PostgreSQL, Redis, the server, an agent-runtime, and the web frontend.

```bash
cd deploy/docker
docker compose up -d --build
```

Then open <http://localhost:8080> (web → nginx) and log in as `admin` with the
password from `SHEPHERD_ADMIN_PASSWORD`.

What the compose stack wires up:

- `postgres` (PostgreSQL 16) with a named volume; `DATABASE_URL` points the server at it.
- `redis` (Redis 7) for the fleet queue.
- `server` — exposes `8088`, sets `DATABASE_URL`, `SHEPHERD_FLEET_REDIS`,
  `SHEPHERD_ADMIN_PASSWORD`. Migrations run automatically on first boot; readiness flips
  to 200 once PG is reachable.
- `agent-runtime` — no ports; `SHEPHERD_BASE=http://server:8088`, with `AGENT_MOCK=1`
  so it runs without real agent CLIs. Set `AGENT_MOCK=0` and mount `git`/`claude`/`codex`
  binaries to drive real backends. Auth is API-key only: issue a key after the first
  server boot (web 个人中心 → API KEY, or `POST /system/apikey`), set `SHEPHERD_AGENT_KEY`
  in `deploy/docker/.env`, then start/restart the `agent-runtime` service.
- `web` — nginx on `8080:80`, reverse-proxying to `server`.

Tear down (add `-v` to drop the database volume):

```bash
docker compose down        # keep data
docker compose down -v     # wipe data
```

This is the recommended **dev path**. For configuration details (env table, fleet
setup, registering real agent backends) see [USAGE.md](./USAGE.md).

---

## Kubernetes via Helm

The chart lives in [`deploy/helm/shepherd/`](../deploy/helm/shepherd/) (`apiVersion: v2`,
`appVersion: 0.0.1`). It renders Deployments + Services for server/agent-runtime/web,
Ingress for server and web, optional HPAs and a server PodDisruptionBudget, a shared
Secret (`DATABASE_URL`, `SHEPHERD_ADMIN_PASSWORD`, `SHEPHERD_AGENT_KEY`,
`SHEPHERD_FLEET_REDIS`, OIDC) and a
non-secret ConfigMap (including the nginx config). Probes are wired to `/healthz`
(liveness) and `/readyz` (readiness).

### Install / upgrade

```bash
helm lint deploy/helm/shepherd

# Dev / demo: in-cluster PostgreSQL + Redis, mock agents, no external DB
helm upgrade --install shepherd deploy/helm/shepherd \
  -n shepherd --create-namespace \
  -f deploy/helm/shepherd/values-dev.yaml

# Production: external managed DB/Redis, real images, ingress + TLS
helm upgrade --install shepherd deploy/helm/shepherd \
  -n shepherd --create-namespace \
  -f deploy/helm/shepherd/values-prod.yaml \
  --set global.image.registry=ghcr.io/muxso \
  --set global.image.tag=v0.0.1 \
  --set config.adminPassword="$SHEPHERD_ADMIN_PASSWORD" \
  --set config.agentKey="$SHEPHERD_AGENT_KEY" \
  --set database.url="$DATABASE_URL" \
  --set config.fleet.redisUrl="$REDIS_URL"
```

`config.agentKey` is the static API key (`sak_…`) the agent-runtime pods authenticate
with — the runtime has no password path and exits at startup without it. Issue it via
`POST /system/apikey` (permissions `DELIVERY:UPDATE` + `REQUIREMENT:UPDATE`); on a fresh
cluster, install first, issue the key, then `helm upgrade --set config.agentKey=…`.

Check rollout:

```bash
kubectl -n shepherd rollout status deploy/shepherd-server
kubectl -n shepherd get pods,svc,ingress
```

### Key values

The full key list is in [`values.yaml`](../deploy/helm/shepherd/values.yaml). Highlights:

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
  adminPassword: ""                             # → Secret SHEPHERD_ADMIN_PASSWORD (required)
  agentKey: ""                                  # → Secret SHEPHERD_AGENT_KEY (required for agent-runtime)
  sessionTtlSecs: 28800
  fleet: { enabled: true, redisUrl: "" }        # SHEPHERD_FLEET_REDIS
  oidc: { feishu: {...}, wecom: {...} }
database:
  url: ""                                       # DATABASE_URL → Secret (required unless postgresql.enabled)
postgresql: { enabled: false }                  # in-cluster PG (dev/demo only)
redis: { enabled: false }                       # in-cluster Redis (dev/demo only)
```

### Dev vs prod values

- **`values-dev.yaml`** — `postgresql.enabled: true`, `redis.enabled: true`,
  `agentRuntime.mock: true`, no server ingress. Self-contained; nothing external required.
  In-cluster PostgreSQL/Redis come from conditioned Bitnami subchart dependencies and are
  **for dev/demo only** — never use them as a production datastore.
- **`values-prod.yaml`** — points `database.url` and `config.fleet.redisUrl` at managed
  services, real images, ingress enabled with TLS, mock off. Treat as a starting template.

### Ingress & TLS

Enable ingress on the web (browser entry) and optionally the server:

```yaml
web:
  ingress:
    enabled: true
    className: nginx            # or alb / gce as appropriate
    host: shepherd.example.com
    tls: true
    annotations:
      cert-manager.io/cluster-issuer: letsencrypt-prod
```

With `tls: true` the chart adds a TLS block referencing a `<host>-tls` Secret. Pair it
with cert-manager (annotation above) or provide the certificate Secret yourself. On a
cloud LB ingress class (`alb`, `gce`) follow that controller's annotation conventions.

### Scaling & HPA

- **Static**: bump `server.replicas` / `agentRuntime.replicas` / `web.replicas`.
- **Autoscaling**: set `autoscaling.enabled: true`. The agent-runtime HPA is on by
  default (`1..10` on 70% CPU) — the fleet is the elastic tier and scales with workload.
  The server HPA is off by default; enable it for bursty API traffic.
- A `PodDisruptionBudget` keeps the server available during node drains.

### Secrets handling

Secrets are never baked into images or committed. Provide them at install time via
`--set` (from your shell/CI environment) or with a pre-created Secret + `existingSecret`
pattern. `config.adminPassword`, `config.agentKey` and `database.url` are **required**
in production and land in the chart's Secret as `SHEPHERD_ADMIN_PASSWORD`,
`SHEPHERD_AGENT_KEY` and `DATABASE_URL`. Prefer a
secrets manager (External Secrets Operator, SealedSecrets, cloud secret stores) over
plaintext `--set` in real deployments.

---

## Multi-cloud Terraform

Terraform under [`deploy/terraform/`](../deploy/terraform/) provisions cloud infra and
then installs the Helm chart on the new cluster. Each cloud directory provisions a
managed Kubernetes cluster, a PostgreSQL 16 server, a Redis 7 cache, a container
registry, and an ingress class, then calls the cloud-agnostic
`modules/shepherd-app` module to `helm install` the chart.

| Dir | Kubernetes | PostgreSQL | Redis | Registry | Ingress class |
|-----|-----------|-----------|-------|----------|---------------|
| [`aws/`](../deploy/terraform/aws/) | EKS + managed node group | RDS PostgreSQL 16 | ElastiCache Redis 7 | ECR (×3) | `alb` (AWS LB Controller) |
| [`gcp/`](../deploy/terraform/gcp/) | GKE | Cloud SQL PostgreSQL 16 | Memorystore Redis | Artifact Registry | GCE |
| [`azure/`](../deploy/terraform/azure/) | AKS | Azure Database for PostgreSQL Flexible Server | Azure Cache for Redis | ACR | (cluster ingress) |

`modules/shepherd-app` is cloud-agnostic: given configured `helm` + `kubernetes`
providers and the DB/Redis endpoints, it creates the `helm_release`
(`chart = "../../helm/shepherd"`) with the registry, tag, secrets, hosts, and replica
counts. It declares **no provider blocks** — the cloud directory passes configured
providers in.

### init / fmt / validate / plan / apply

Run from the chosen cloud directory, e.g. `deploy/terraform/aws`:

```bash
cd deploy/terraform/aws
cp terraform.tfvars.example terraform.tfvars   # then edit
# fill in: region, cluster sizing, db_password, admin_password, image_tag, hosts …

terraform init
terraform fmt -check
terraform validate
terraform plan -out tfplan
terraform apply tfplan
```

> **Not validated in-repo.** This repository has no Terraform binary, cloud
> credentials, or state. **Always** run `init` / `fmt` / `validate` / `plan` and review
> the plan before `apply`. Secret variables (`database_url`, `redis_url`,
> `admin_password`, db passwords) are marked `sensitive = true`; supply them via tfvars,
> `TF_VAR_*`, or your secrets backend — never commit them.

### Outputs

Each cloud directory exports the same outputs:

| Output | Use |
|--------|-----|
| `cluster_name` | Provisioned cluster name |
| `kubeconfig_command` | One-liner to fetch kubeconfig (e.g. `aws eks update-kubeconfig …`) |
| `database_url` (sensitive) | Connection string the chart consumes |
| `redis_url` (sensitive) | Fleet Redis URL |
| `registry_url` | Where to push the three images |
| `app_url` | Public URL of the web frontend |

After apply:

```bash
$(terraform output -raw kubeconfig_command)
kubectl -n shepherd get pods
echo "App: $(terraform output -raw app_url)"
```

Each cloud directory has its own `README.md` with provider versions
(`versions.tf`, Terraform `>= 1.5`, pinned providers) and prerequisites.

---

## CI/CD auto-deploy

Two GitHub Actions workflows under [`.github/workflows/`](../.github/workflows/).

### `images.yml` — build & push images

- **Triggers**: push of a `v*` tag, or `workflow_dispatch`.
- Matrix builds `[server, agent-runtime, web]` with `docker/build-push-action` + Buildx,
  logs in to GHCR as `${{ github.repository_owner }}` using the built-in `GITHUB_TOKEN`,
  and pushes each image tagged with the git tag and the commit `sha`.

No extra secrets are required — `GITHUB_TOKEN` covers GHCR.

### `deploy.yml` — deploy to a cloud

- **Triggers**: `workflow_dispatch` (inputs `cloud` = `aws|gcp|azure`, `environment`,
  `image_tag`) and `release: published`. **Release-driven, never push-driven** — merging
  to a branch does not deploy.
- Runs against a GitHub **Environment**, which gates the job behind **manual approval**
  (and any environment protection rules / required reviewers you configure).
- Authenticates to the cloud via **OIDC** (no long-lived cloud keys):
  `aws-actions/configure-aws-credentials`, `google-github-actions/auth`, or `azure/login`.
- Fetches kubeconfig for the target cluster, then runs:

  ```bash
  helm upgrade --install shepherd deploy/helm/shepherd \
    -f deploy/helm/shepherd/values-<env>.yaml \
    --set global.image.tag=<image_tag> \
    --set config.adminPassword=$SHEPHERD_ADMIN_PASSWORD \
    --set database.url=$DATABASE_URL \
    --set config.fleet.redisUrl=$REDIS_URL
  ```

### Required secrets / variables

Set these as GitHub repository or **Environment** secrets (scope cloud-specific ones to
the matching environment):

**Application (all clouds):**

| Secret | Purpose |
|--------|---------|
| `SHEPHERD_ADMIN_PASSWORD` | Admin account bootstrap + web login |
| `SHEPHERD_AGENT_KEY` | Static API key (`sak_…`) — the only agent-runtime credential |
| `DATABASE_URL` | `postgres://…` connection string |
| `REDIS_URL` | Fleet Redis URL |

**AWS (OIDC):**

| Secret | Purpose |
|--------|---------|
| `AWS_ROLE_ARN` | IAM role assumed via OIDC |
| `AWS_REGION` | Region |
| `EKS_CLUSTER` | Cluster name for `update-kubeconfig` |

**GCP (Workload Identity Federation):**

| Secret | Purpose |
|--------|---------|
| `GCP_WORKLOAD_IDENTITY_PROVIDER` | WIF provider resource |
| `GCP_SA` | Service account to impersonate |
| `GKE_CLUSTER` | Cluster name |
| `GKE_ZONE` | Cluster zone/region |
| `GCP_PROJECT` | Project ID |

**Azure (OIDC):**

| Secret | Purpose |
|--------|---------|
| `AZURE_CLIENT_ID` | App registration / federated identity |
| `AZURE_TENANT_ID` | Tenant |
| `AZURE_SUBSCRIPTION_ID` | Subscription |
| `AKS_CLUSTER` | Cluster name |
| `AKS_RG` | Resource group |

The required secrets are also documented in a comment block at the top of `deploy.yml`.

---

## Day-2 operations

### Database migrations

Migrations run **automatically on server startup** — there is no separate migration
step. A rolling update therefore applies pending migrations as the new server pods come
up; keep migrations backward-compatible so old and new pods can coexist during the roll.
Readiness (`/readyz`) returns 503 until PostgreSQL is reachable, so traffic is only sent
to a pod once its DB connection (and migrations) succeed.

### Backups

PostgreSQL is the single source of truth — back it up.

- **Managed (RDS / Cloud SQL / Azure DB)**: enable automated backups + point-in-time
  recovery and set a retention window in Terraform/console.
- **Self-managed**: schedule `pg_dump`/`pg_basebackup`, e.g.

  ```bash
  pg_dump "$DATABASE_URL" | gzip > shepherd-$(date +%F).sql.gz
  ```

Redis is a transient fleet queue (only with multi-host fleet) and does not require
backup; losing it re-queues in-flight work rather than losing committed data.

### Observability

- **Liveness**: `GET /healthz` → `200 ok`.
- **Readiness**: `GET /readyz` → `200` when PostgreSQL is reachable, else `503`. Wired to
  the Kubernetes readiness probe.
- **Fleet stats**: `GET /agent/work/stats` reports per-capability backlog / in-flight
  counts for the agent fleet — use it to watch queue depth and right-size replicas.

```bash
curl -fsS https://shepherd.example.com/healthz
curl -fsS https://shepherd.example.com/readyz
curl -fsS -H "Authorization: Bearer $SHEPHERD_AGENT_KEY" https://shepherd.example.com/agent/work/stats
```

### Scaling the fleet

The agent-runtime is the elastic tier. Scale it by replicas or HPA:

```bash
kubectl -n shepherd scale deploy/shepherd-agent-runtime --replicas=8
# or rely on the HPA (default 1..10 on 70% CPU)
```

Watch `GET /agent/work/stats`: sustained backlog → add replicas (or raise
`agentRuntime.autoscaling.maxReplicas`); persistent idle → scale down. Because the
runtime is outbound-only, adding replicas needs no ingress/networking changes.

### Single-host vs multi-host Redis

- **Single server replica**: omit Redis — the server uses an in-process queue
  (`config.fleet.enabled: false` / no `redisUrl`).
- **Multiple server replicas (HA)**: a shared **Redis 7** is **required** so all
  replicas see one fleet queue (`config.fleet.enabled: true`, `config.fleet.redisUrl`).
  Reaping/reclaim of stalled work is governed by `SHEPHERD_FLEET_REAP_INTERVAL_S` and
  `SHEPHERD_FLEET_RECLAIM_MS`.

### Rotating the admin password & secrets

```bash
helm upgrade shepherd deploy/helm/shepherd -n shepherd --reuse-values \
  --set config.adminPassword="$NEW_PASSWORD"
kubectl -n shepherd rollout restart deploy/shepherd-server
```

The admin password only affects the server (admin account + web login). To rotate the
runtime credential, issue a new API key via `POST /system/apikey`, then
`helm upgrade --reuse-values --set config.agentKey="$NEW_KEY"` and restart
`deploy/shepherd-agent-runtime`; revoke the old key once the rollout completes.
Rotate `DATABASE_URL` / `SHEPHERD_FLEET_REDIS` the same way. With a secrets manager,
update the backing secret and restart the deployments.

### Zero-downtime rolling updates

Deploys are rolling by default. To ship a new image:

```bash
helm upgrade shepherd deploy/helm/shepherd -n shepherd --reuse-values \
  --set global.image.tag=v0.0.2
kubectl -n shepherd rollout status deploy/shepherd-server
```

With `server.replicas >= 2`, the readiness probe + PodDisruptionBudget keep at least one
server serving throughout. New pods only receive traffic after `/readyz` passes (DB
reachable, migrations applied). Roll back with `helm rollback shepherd`.

---

See [USAGE.md](./USAGE.md) for application configuration, the web UI tour, fleet/agent
setup, and HTTP/MCP API usage.
