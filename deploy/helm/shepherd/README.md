# Shepherd Helm chart

Deploys the three Shepherd components to Kubernetes:

| Component | Kind | Ports | Notes |
|-----------|------|-------|-------|
| `shepherd-server` | Deployment + Service + optional Ingress/HPA + PodDisruptionBudget | 8088 (ClusterIP) | Public API. Runs DB migrations on startup. Liveness `/healthz`, readiness `/readyz`. |
| `shepherd-agent-runtime` | Deployment + optional HPA | none | Outbound-only worker fleet. Long-polls the server at `SHEPHERD_BASE`. Horizontally scalable. |
| `shepherd-web` | Deployment + Service + optional Ingress + ConfigMap | 80 (ClusterIP) | nginx serving the SPA and reverse-proxying API prefixes to the server, disambiguated by the `Accept` header. |

Shared resources: one `Secret` (sensitive env — only keys that are set), one `ConfigMap` (non-secret server env), one `ServiceAccount`. For dev/demo, optional in-cluster PostgreSQL and Redis `StatefulSet`s.

## Prerequisites

- Kubernetes 1.23+ (uses `autoscaling/v2`, `networking.k8s.io/v1`, `policy/v1`).
- Helm 3.8+.
- Container images published as `<registry>/shepherd-server`, `<registry>/shepherd-agent-runtime`, `<registry>/shepherd-web` (default registry `ghcr.io/muxso`).

## Install

### Dev / demo (in-cluster Postgres + Redis, mocked agents, no ingress)

```sh
helm install shepherd deploy/helm/shepherd \
  -f deploy/helm/shepherd/values-dev.yaml \
  --set config.adminPassword="$(openssl rand -hex 16)"
```

This brings up single replicas of each component plus a throwaway PostgreSQL 16 and
Redis 7. `DATABASE_URL` and `SHEPHERD_FLEET_REDIS` are derived automatically from the
in-cluster services. `agentRuntime.mock=true` sets `AGENT_MOCK=1` so no real agent CLIs
are required.

Reach the UI:

```sh
kubectl port-forward svc/shepherd-web 8080:80
# open http://localhost:8080  (login: admin / <adminPassword>)
```

### Production (external managed DB + Redis, ingress + TLS, real replicas)

Secrets must never be committed — pass them at install time (or via a secret manager
/ sealed secret):

```sh
helm upgrade --install shepherd deploy/helm/shepherd \
  -f deploy/helm/shepherd/values-prod.yaml \
  --set global.image.tag=v1.2.3 \
  --set config.adminPassword="$ADMIN_PASSWORD" \
  --set database.url="$DATABASE_URL" \
  --set config.fleet.redisUrl="$REDIS_URL"
```

`values-prod.yaml` enables ingress + TLS for both `web` and `server`, real replica
counts, and HPAs, and keeps the in-cluster datastores disabled.

## Values overview

| Key | Default | Purpose |
|-----|---------|---------|
| `global.image.registry` | `ghcr.io/muxso` | Image registry/owner. |
| `global.image.tag` | `latest` | Image tag for all three components. |
| `global.image.pullPolicy` | `IfNotPresent` | Image pull policy. |
| `global.imagePullSecrets` | `[]` | Private-registry pull secrets. |
| `server.replicas` | `2` | Server replicas (ignored when `server.autoscaling.enabled`). |
| `server.resources` | requests 100m/128Mi, limits 1/512Mi | Server resources. |
| `server.service.{type,port}` | `ClusterIP` / `8088` | Server Service. |
| `server.ingress.{enabled,className,host,tls,annotations}` | disabled | Server ingress. |
| `server.autoscaling.*` | disabled, 2–6 @ 70% CPU | Server HPA. |
| `server.env` | `{}` | Extra server env. |
| `agentRuntime.replicas` | `3` | Agent-runtime replicas (ignored when autoscaling). |
| `agentRuntime.mock` | `false` | Sets `AGENT_MOCK=1` for demo without real CLIs. |
| `agentRuntime.resources` | requests 100m/128Mi, limits 2/1Gi | Agent-runtime resources. |
| `agentRuntime.autoscaling.*` | enabled, 1–10 @ 70% CPU | Agent-runtime HPA. |
| `agentRuntime.env` | `{}` | Extra agent-runtime env (e.g. `CLAUDE_BIN`). |
| `web.replicas` | `2` | Web replicas. |
| `web.resources` | requests 50m/64Mi, limits 500m/256Mi | Web resources. |
| `web.service.{type,port}` | `ClusterIP` / `80` | Web Service. |
| `web.ingress.{enabled,className,host,tls,annotations}` | enabled | Web ingress. |
| `config.adminPassword` | `""` (**required**) | `SHEPHERD_ADMIN_PASSWORD` (server admin bootstrap + web login). |
| `config.agentKey` | `""` (**required** for agent-runtime) | `SHEPHERD_AGENT_KEY` — static API key (`sak_…`) the runtimes authenticate with; issue via `POST /system/apikey`. |
| `config.sessionTtlSecs` | `28800` | `SHEPHERD_SESSION_TTL_SECS`. |
| `config.fleet.enabled` | `true` | Multi-host fleet mode (`SHEPHERD_AGENT_FLEET`). |
| `config.fleet.redisUrl` | `""` | `SHEPHERD_FLEET_REDIS` (required for multi-replica fleet). |
| `config.fleet.reapIntervalSecs` | `15` | `SHEPHERD_FLEET_REAP_INTERVAL_S`. |
| `config.fleet.reclaimMs` | `30000` | `SHEPHERD_FLEET_RECLAIM_MS`. |
| `config.oidc.feishu.*` / `config.oidc.wecom.*` | `""` | Optional OIDC; only set keys land in the Secret. |
| `database.url` | `""` | `DATABASE_URL` (required unless `postgresql.enabled`). |
| `postgresql.enabled` | `false` | In-cluster PostgreSQL (dev/demo). |
| `redis.enabled` | `false` | In-cluster Redis (dev/demo). |
| `serviceAccount.{create,name}` | `true` / `""` | ServiceAccount. |

## Secrets & required values

- `config.adminPassword` is **required** for any non-throwaway deployment. When empty,
  the server falls back to its built-in default — fine for a local demo, unsafe otherwise.
- `config.agentKey` is **required** whenever agent-runtime pods run: the runtime only
  authenticates with a static API key (`SHEPHERD_AGENT_KEY`) and exits at startup without
  one. Bootstrap order on a fresh cluster: install with the runtime scaled to 0 (or let it
  crash-loop), issue a key (`POST /system/apikey`, permissions `DELIVERY:UPDATE` +
  `REQUIREMENT:UPDATE`), then `helm upgrade --set config.agentKey=sak_…`.
- `database.url` is **required** unless `postgresql.enabled=true` (then it is derived
  from the in-cluster service).
- For a multi-replica server fleet, set `config.fleet.redisUrl` (or `redis.enabled=true`
  for dev). A single server replica can run on the in-process queue with fleet disabled.
- Only secret keys that are actually set are written to the `Secret` — no empty/placeholder
  secrets are emitted, and no secret values are hardcoded in the chart.

## In-cluster datastores

`postgresql.enabled` / `redis.enabled` ship minimal single-replica `StatefulSet`s
(`postgres:16-alpine`, `redis:7-alpine`) intended for dev/demo only — no HA, no backups.
To use the Bitnami subcharts instead, uncomment the `dependencies` block in `Chart.yaml`,
run `helm dependency build deploy/helm/shepherd`, and delete `templates/dev-*.yaml`.

## nginx Accept-header proxy

`shepherd-web` serves the SPA and reverse-proxies these backend prefixes to the server:

```
/api /auth /project /organization /system /role /user-role /requirement
/decomposition /delivery /verification /skill /bug /functional-case /test-plan
/perf /runner /runner-agent /case-review /mcp
```

Each prefixed request is disambiguated by the `Accept` header: `Accept: text/html`
(browser deep-link/navigation) serves the SPA `/index.html`; everything else
(`proxy_pass`) goes to `shepherd-server`. The catch-all `location /` uses
`try_files $uri /index.html` for SPA routing.

## Validate / render locally

```sh
helm lint deploy/helm/shepherd
helm template deploy/helm/shepherd                                      # default
helm template deploy/helm/shepherd -f deploy/helm/shepherd/values-dev.yaml
helm template deploy/helm/shepherd -f deploy/helm/shepherd/values-prod.yaml \
  --set config.adminPassword=x --set database.url=postgres://… --set config.fleet.redisUrl=redis://…
```
