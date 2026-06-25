###############################################################################
# Providers
###############################################################################

provider "aws" {
  region = var.region
}

data "aws_availability_zones" "available" {
  state = "available"
}

locals {
  azs = slice(data.aws_availability_zones.available.names, 0, 3)

  cluster_name = "${var.name_prefix}-eks"

  db_password   = var.db_password != "" ? var.db_password : random_password.db.result
  db_host       = module.rds.db_instance_address
  database_url  = "postgres://${var.db_username}:${local.db_password}@${local.db_host}:5432/${var.db_name}"
  redis_url     = "redis://${aws_elasticache_replication_group.redis.primary_endpoint_address}:6379"
  ecr_registry  = "${data.aws_caller_identity.current.account_id}.dkr.ecr.${var.region}.amazonaws.com"
  ecr_repos     = ["shepherd-server", "shepherd-agent-runtime", "shepherd-web"]
}

data "aws_caller_identity" "current" {}

resource "random_password" "db" {
  length  = 24
  special = false
}

###############################################################################
# VPC
###############################################################################

module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 5.8"

  name = "${var.name_prefix}-vpc"
  cidr = var.vpc_cidr

  azs             = local.azs
  private_subnets = [for k, v in local.azs : cidrsubnet(var.vpc_cidr, 4, k)]
  public_subnets  = [for k, v in local.azs : cidrsubnet(var.vpc_cidr, 4, k + 8)]

  enable_nat_gateway   = true
  single_nat_gateway   = true
  enable_dns_hostnames = true
  enable_dns_support   = true

  # Tags required by the AWS Load Balancer Controller for subnet auto-discovery.
  public_subnet_tags = {
    "kubernetes.io/role/elb"                      = "1"
    "kubernetes.io/cluster/${local.cluster_name}" = "shared"
  }
  private_subnet_tags = {
    "kubernetes.io/role/internal-elb"             = "1"
    "kubernetes.io/cluster/${local.cluster_name}" = "shared"
  }

  tags = var.tags
}

###############################################################################
# EKS
###############################################################################

module "eks" {
  source  = "terraform-aws-modules/eks/aws"
  version = "~> 20.8"

  cluster_name    = local.cluster_name
  cluster_version = var.kubernetes_version

  cluster_endpoint_public_access = true

  vpc_id     = module.vpc.vpc_id
  subnet_ids = module.vpc.private_subnets

  # Grant the identity running terraform admin on the cluster.
  enable_cluster_creator_admin_permissions = true

  eks_managed_node_groups = {
    default = {
      instance_types = var.node_instance_types
      desired_size   = var.node_desired_size
      min_size       = var.node_min_size
      max_size       = var.node_max_size
    }
  }

  tags = var.tags
}

###############################################################################
# RDS PostgreSQL 16
###############################################################################

resource "aws_security_group" "rds" {
  name_prefix = "${var.name_prefix}-rds-"
  description = "Allow PostgreSQL from the EKS nodes"
  vpc_id      = module.vpc.vpc_id

  ingress {
    description     = "PostgreSQL from EKS nodes"
    from_port       = 5432
    to_port         = 5432
    protocol        = "tcp"
    security_groups = [module.eks.node_security_group_id]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = var.tags
}

module "rds" {
  source  = "terraform-aws-modules/rds/aws"
  version = "~> 6.7"

  identifier = "${var.name_prefix}-pg"

  engine               = "postgres"
  engine_version       = "16"
  family               = "postgres16"
  major_engine_version = "16"
  instance_class       = var.db_instance_class

  allocated_storage     = var.db_allocated_storage
  max_allocated_storage = var.db_allocated_storage * 2

  db_name  = var.db_name
  username = var.db_username
  password = local.db_password
  port     = 5432

  manage_master_user_password = false

  multi_az               = false
  create_db_subnet_group = true
  subnet_ids             = module.vpc.private_subnets
  vpc_security_group_ids = [aws_security_group.rds.id]

  skip_final_snapshot     = true
  backup_retention_period = 7
  deletion_protection     = false

  tags = var.tags
}

###############################################################################
# ElastiCache Redis 7
###############################################################################

resource "aws_security_group" "redis" {
  name_prefix = "${var.name_prefix}-redis-"
  description = "Allow Redis from the EKS nodes"
  vpc_id      = module.vpc.vpc_id

  ingress {
    description     = "Redis from EKS nodes"
    from_port       = 6379
    to_port         = 6379
    protocol        = "tcp"
    security_groups = [module.eks.node_security_group_id]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = var.tags
}

resource "aws_elasticache_subnet_group" "redis" {
  name       = "${var.name_prefix}-redis"
  subnet_ids = module.vpc.private_subnets
  tags       = var.tags
}

resource "aws_elasticache_replication_group" "redis" {
  replication_group_id = "${var.name_prefix}-redis"
  description          = "Shepherd fleet redis"

  engine               = "redis"
  engine_version       = "7.1"
  node_type            = var.redis_node_type
  num_cache_clusters   = 1
  port                 = 6379
  parameter_group_name = "default.redis7"

  subnet_group_name  = aws_elasticache_subnet_group.redis.name
  security_group_ids = [aws_security_group.redis.id]

  automatic_failover_enabled = false
  at_rest_encryption_enabled = true

  tags = var.tags
}

###############################################################################
# ECR repositories (3)
###############################################################################

resource "aws_ecr_repository" "repos" {
  for_each = toset(local.ecr_repos)

  name                 = each.value
  image_tag_mutability = "MUTABLE"

  image_scanning_configuration {
    scan_on_push = true
  }

  tags = var.tags
}

###############################################################################
# Kubernetes / Helm providers (wired from EKS outputs)
###############################################################################

data "aws_eks_cluster_auth" "this" {
  name = module.eks.cluster_name
}

provider "kubernetes" {
  host                   = module.eks.cluster_endpoint
  cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)
  token                  = data.aws_eks_cluster_auth.this.token
}

provider "helm" {
  kubernetes {
    host                   = module.eks.cluster_endpoint
    cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)
    token                  = data.aws_eks_cluster_auth.this.token
  }
}

###############################################################################
# Shepherd application (cloud-agnostic module)
###############################################################################

module "app" {
  source = "../modules/shepherd-app"

  providers = {
    kubernetes = kubernetes
    helm       = helm
  }

  image_registry     = local.ecr_registry
  image_tag          = var.image_tag
  database_url       = local.database_url
  redis_url          = local.redis_url
  admin_password     = var.admin_password
  server_host        = var.server_host
  web_host           = var.web_host
  ingress_class_name = "alb"
  server_replicas    = var.server_replicas
  agent_replicas     = var.agent_replicas

  depends_on = [
    module.eks,
    module.rds,
    aws_elasticache_replication_group.redis,
  ]
}
