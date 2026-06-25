# Shepherd on GCP (GKE)

Provisions a full Shepherd stack on Google Cloud and installs the Helm chart via
the cloud-agnostic [`modules/shepherd-app`](../modules/shepherd-app) module.

## What this provisions

| Resource | Implementation |
|----------|----------------|
| Networking | `google_compute_network` / `google_compute_subnetwork` (+ pods/services secondary ranges) + Private Service Access for Cloud SQL |
| Kubernetes | `google_container_cluster` (GKE) + `google_container_node_pool` |
| Database | `google_sql_database_instance` — **PostgreSQL 16** (private IP) |
| Cache/queue | `google_redis_instance` — Memorystore **Redis 7** |
| Registry | `google_artifact_registry_repository` (Docker) |
| App | `module.app` → Helm release of `deploy/helm/shepherd`, ingress class `gce` |

`DATABASE_URL` is assembled as
`postgres://<db_username>:<db_password>@<private-ip>:5432/<db_name>` and
`REDIS_URL` as `redis://<host>:6379`, both passed to the app module as sensitive
values. Push per-component images under the `registry_url` output (one Artifact
Registry repo holding `shepherd-server`, `shepherd-agent-runtime`, `shepherd-web`).

## Prerequisites

- A GCP project with billing enabled and these APIs enabled: `container`,
  `sqladmin`, `redis`, `artifactregistry`, `servicenetworking`,
  `compute`.
- `gcloud` authenticated with permission to create the resources above.

## Usage

```bash
cp terraform.tfvars.example terraform.tfvars
# edit terraform.tfvars (set project_id, admin_password, etc.)

terraform init
terraform fmt
terraform validate
terraform plan
terraform apply
```

After apply:

```bash
$(terraform output -raw kubeconfig_command)
kubectl -n shepherd get pods
```

## Outputs

- `cluster_name`, `kubeconfig_command`
- `database_url` (sensitive), `redis_url` (sensitive)
- `registry_url` — Artifact Registry base; append `/<repo>:<tag>`
- `app_url`

## Note

This stack was **not validated locally** (no terraform binary or cloud
credentials in this repo). Always run `terraform init && terraform fmt &&
terraform validate && terraform plan` before `apply`.
