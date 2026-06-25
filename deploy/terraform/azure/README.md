# Shepherd on Azure (AKS)

Provisions a full Shepherd stack on Azure and installs the Helm chart via the
cloud-agnostic [`modules/shepherd-app`](../modules/shepherd-app) module.

## What this provisions

| Resource | Implementation |
|----------|----------------|
| Networking | `azurerm_virtual_network` + AKS subnet + delegated DB subnet + Private DNS zone |
| Kubernetes | `azurerm_kubernetes_cluster` (AKS) with autoscaling default node pool + Web Application Routing add-on |
| Database | `azurerm_postgresql_flexible_server` — **PostgreSQL 16** (VNet-integrated, private) |
| Cache/queue | `azurerm_redis_cache` — Azure Cache for **Redis** |
| Registry | `azurerm_container_registry` (ACR) + `AcrPull` role for the kubelet identity |
| App | `module.app` → Helm release of `deploy/helm/shepherd`, ingress class `webapprouting.kubernetes.azure.com` |

`DATABASE_URL` is assembled as
`postgres://<db_username>:<db_password>@<server-fqdn>:5432/<db_name>` and
`REDIS_URL` as `redis://:<access-key>@<host>:6379`, both passed to the app module
as sensitive values.

> Azure Cache for Redis disables the non-TLS port by default. This stack sets
> `non_ssl_port_enabled = true` so the in-cluster `redis://...:6379` URL works.
> Tighten to TLS (`rediss://...:6380`) if your deployment requires it.

## Prerequisites

- An Azure subscription and `az` authenticated with Contributor access.
- The `Microsoft.ContainerService`, `Microsoft.DBforPostgreSQL`, `Microsoft.Cache`,
  and `Microsoft.ContainerRegistry` resource providers registered.

## Usage

```bash
cp terraform.tfvars.example terraform.tfvars
# edit terraform.tfvars (set admin_password etc.)

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
- `registry_url` — ACR login server; append `/<repo>:<tag>`
- `app_url`

## Note

This stack was **not validated locally** (no terraform binary or cloud
credentials in this repo). Always run `terraform init && terraform fmt &&
terraform validate && terraform plan` before `apply`.
