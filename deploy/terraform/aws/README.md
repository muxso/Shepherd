# Shepherd on AWS (EKS)

Provisions a full Shepherd stack on AWS and installs the Helm chart via the
cloud-agnostic [`modules/shepherd-app`](../modules/shepherd-app) module.

## What this provisions

| Resource | Implementation |
|----------|----------------|
| Networking | `terraform-aws-modules/vpc` — VPC, 3 public + 3 private subnets, single NAT GW |
| Kubernetes | `terraform-aws-modules/eks` — EKS cluster + a managed node group |
| Database | `terraform-aws-modules/rds` — RDS **PostgreSQL 16** |
| Cache/queue | `aws_elasticache_replication_group` — ElastiCache **Redis 7** |
| Registry | `aws_ecr_repository` ×3 — `shepherd-server`, `shepherd-agent-runtime`, `shepherd-web` |
| App | `module.app` → Helm release of `deploy/helm/shepherd`, ingress class `alb` |

The `DATABASE_URL` is assembled as
`postgres://<db_username>:<db_password>@<rds-address>:5432/<db_name>` and the
`REDIS_URL` as `redis://<primary-endpoint>:6379`, both passed to the app module
as sensitive values.

## Prerequisites

- AWS credentials with permissions to create the resources above.
- An ingress controller able to satisfy `ingressClassName: alb` — install the
  [AWS Load Balancer Controller](https://kubernetes-sigs.github.io/aws-load-balancer-controller/)
  after the cluster exists (subnets are already tagged for auto-discovery).
- The Shepherd images pushed to the created ECR repos (see `registry_url` output)
  or to whatever registry you point `image_registry` at.

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
- `registry_url` — ECR base; append `/<repo>:<tag>`
- `app_url`

## Note

This stack was **not validated locally** (no terraform binary or cloud
credentials in this repo). Always run `terraform init && terraform fmt &&
terraform validate && terraform plan` before `apply`.
