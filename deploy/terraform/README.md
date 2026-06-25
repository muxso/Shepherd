# Shepherd — Terraform

Infrastructure-as-code to provision Shepherd on a managed Kubernetes cluster in
AWS, GCP, or Azure, then install the [Helm chart](../helm/shepherd) on top.

## Layout

```
deploy/terraform/
├── modules/
│   └── shepherd-app/     # cloud-agnostic Helm install (helm_release + namespace)
├── aws/                  # VPC + EKS + RDS PG 16 + ElastiCache Redis 7 + ECR + app
├── gcp/                  # GKE + Cloud SQL PG 16 + Memorystore Redis 7 + Artifact Registry + app
├── azure/                # AKS + PostgreSQL Flexible Server 16 + Azure Cache for Redis + ACR + app
└── README.md            # you are here
```

### `modules/shepherd-app` (shared)

A **provider-agnostic** module. It does **not** declare provider blocks; each
cloud stack configures `kubernetes` + `helm` providers from its freshly-created
cluster and passes them in via a `providers = { ... }` block. The module:

- creates the target `kubernetes_namespace`, and
- installs the chart at `../../helm/shepherd` as a `helm_release`, mapping
  inputs onto chart values via `set` / `set_sensitive`.

Inputs include `image_registry`, `image_tag`, `database_url` (sensitive),
`redis_url` (sensitive), `admin_password` (sensitive), `server_host`,
`web_host`, `ingress_class_name`, `server_replicas`, `agent_replicas`,
`namespace`, and an optional `oidc` object. Outputs: `release_name`, `namespace`.

### Cloud stacks (`aws/`, `gcp/`, `azure/`)

Each stack provisions networking, a managed Kubernetes cluster, a managed
PostgreSQL 16 instance, a managed Redis instance, and a container registry, then
assembles the connection strings and calls `module "app"`:

- `DATABASE_URL` = `postgres://USER:PASS@HOST:5432/DB`
- `REDIS_URL`    = `redis://HOST:6379` (Azure includes the access key)

Each stack exposes the same outputs: `cluster_name`, `kubeconfig_command`,
`database_url` (sensitive), `redis_url` (sensitive), `registry_url`, `app_url`.

| Cloud | Cluster | Postgres | Redis | Registry | Ingress class |
|-------|---------|----------|-------|----------|---------------|
| AWS | EKS | RDS | ElastiCache | ECR (3 repos) | `alb` |
| GCP | GKE | Cloud SQL | Memorystore | Artifact Registry | `gce` |
| Azure | AKS | PG Flexible Server | Azure Cache for Redis | ACR | `webapprouting.kubernetes.azure.com` |

## Provider versions

Pinned in each `versions.tf` (Terraform `>= 1.5`):

| Provider | Constraint |
|----------|-----------|
| `hashicorp/aws` | `>= 5.40, < 6.0` |
| `hashicorp/google` | `>= 5.20, < 6.0` |
| `hashicorp/azurerm` | `>= 3.100, < 4.0` |
| `hashicorp/kubernetes` | `>= 2.25, < 3.0` |
| `hashicorp/helm` | `>= 2.12, < 3.0` |
| `hashicorp/random` | `>= 3.5` |

## Workflow

```bash
cd deploy/terraform/<aws|gcp|azure>

cp terraform.tfvars.example terraform.tfvars
# edit terraform.tfvars: set admin_password (and project_id on GCP), tune sizing

terraform init       # download providers + registry modules
terraform fmt        # canonical formatting
terraform validate   # static validation
terraform plan       # review the change set
terraform apply      # create everything

# wire kubectl to the new cluster
$(terraform output -raw kubeconfig_command)
kubectl -n shepherd get pods
```

Push the three images (`shepherd-server`, `shepherd-agent-runtime`,
`shepherd-web`) to the registry reported by `terraform output registry_url`
before (or in parallel with) the Helm install, or point `image_registry` at an
existing registry such as `ghcr.io/<owner>`.

## Note

These stacks were **not validated locally** — this repo has no `terraform`
binary and no cloud credentials. Always run
`terraform init && terraform fmt && terraform validate && terraform plan`
before `apply`. See `docs/DEPLOYMENT.md` for the full deployment guide.
