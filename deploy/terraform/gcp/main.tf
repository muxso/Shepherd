###############################################################################
# Providers
###############################################################################

provider "google" {
  project = var.project_id
  region  = var.region
}

data "google_client_config" "current" {}

locals {
  cluster_name = "${var.name_prefix}-gke"
  repo_id      = "${var.name_prefix}-images"

  db_password  = var.db_password != "" ? var.db_password : random_password.db.result
  db_host      = google_sql_database_instance.pg.private_ip_address
  database_url = "postgres://${var.db_username}:${local.db_password}@${local.db_host}:5432/${var.db_name}"
  redis_url    = "redis://${google_redis_instance.cache.host}:6379"
  registry_url = "${var.region}-docker.pkg.dev/${var.project_id}/${local.repo_id}"
}

resource "random_password" "db" {
  length  = 24
  special = false
}

###############################################################################
# Networking
###############################################################################

resource "google_compute_network" "vpc" {
  name                    = "${var.name_prefix}-vpc"
  auto_create_subnetworks = false
}

resource "google_compute_subnetwork" "subnet" {
  name          = "${var.name_prefix}-subnet"
  region        = var.region
  network       = google_compute_network.vpc.id
  ip_cidr_range = "10.0.0.0/20"

  secondary_ip_range {
    range_name    = "pods"
    ip_cidr_range = "10.4.0.0/14"
  }
  secondary_ip_range {
    range_name    = "services"
    ip_cidr_range = "10.8.0.0/20"
  }
}

# Private Service Access range for Cloud SQL private IP.
resource "google_compute_global_address" "private_ip" {
  name          = "${var.name_prefix}-priv-ip"
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 16
  network       = google_compute_network.vpc.id
}

resource "google_service_networking_connection" "private_vpc" {
  network                 = google_compute_network.vpc.id
  service                 = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.private_ip.name]
}

###############################################################################
# GKE
###############################################################################

resource "google_container_cluster" "primary" {
  name     = local.cluster_name
  location = var.region

  network    = google_compute_network.vpc.id
  subnetwork = google_compute_subnetwork.subnet.id

  # Use a separately managed node pool.
  remove_default_node_pool = true
  initial_node_count       = 1

  min_master_version = var.kubernetes_version != "" ? var.kubernetes_version : null

  ip_allocation_policy {
    cluster_secondary_range_name  = "pods"
    services_secondary_range_name = "services"
  }

  release_channel {
    channel = "REGULAR"
  }

  deletion_protection = false
}

resource "google_container_node_pool" "primary" {
  name     = "${var.name_prefix}-pool"
  location = var.region
  cluster  = google_container_cluster.primary.name

  node_count = var.node_count

  autoscaling {
    min_node_count = var.node_min_count
    max_node_count = var.node_max_count
  }

  node_config {
    machine_type = var.node_machine_type
    oauth_scopes = [
      "https://www.googleapis.com/auth/cloud-platform",
    ]
  }

  management {
    auto_repair  = true
    auto_upgrade = true
  }
}

###############################################################################
# Cloud SQL — PostgreSQL 16
###############################################################################

resource "google_sql_database_instance" "pg" {
  name             = "${var.name_prefix}-pg"
  region           = var.region
  database_version = "POSTGRES_16"

  depends_on = [google_service_networking_connection.private_vpc]

  settings {
    tier              = var.db_tier
    availability_type = "ZONAL"
    disk_autoresize   = true

    ip_configuration {
      ipv4_enabled    = false
      private_network = google_compute_network.vpc.id
    }

    backup_configuration {
      enabled = true
    }
  }

  deletion_protection = false
}

resource "google_sql_database" "db" {
  name     = var.db_name
  instance = google_sql_database_instance.pg.name
}

resource "google_sql_user" "user" {
  name     = var.db_username
  instance = google_sql_database_instance.pg.name
  password = local.db_password
}

###############################################################################
# Memorystore — Redis 7
###############################################################################

resource "google_redis_instance" "cache" {
  name               = "${var.name_prefix}-redis"
  tier               = "BASIC"
  memory_size_gb     = var.redis_memory_size_gb
  region             = var.region
  redis_version      = "REDIS_7_0"
  authorized_network = google_compute_network.vpc.id

  depends_on = [google_service_networking_connection.private_vpc]
}

###############################################################################
# Artifact Registry
###############################################################################

resource "google_artifact_registry_repository" "images" {
  location      = var.region
  repository_id = local.repo_id
  format        = "DOCKER"
  description   = "Shepherd container images"
}

###############################################################################
# Kubernetes / Helm providers (wired from GKE outputs)
###############################################################################

provider "kubernetes" {
  host                   = "https://${google_container_cluster.primary.endpoint}"
  cluster_ca_certificate = base64decode(google_container_cluster.primary.master_auth[0].cluster_ca_certificate)
  token                  = data.google_client_config.current.access_token
}

provider "helm" {
  kubernetes {
    host                   = "https://${google_container_cluster.primary.endpoint}"
    cluster_ca_certificate = base64decode(google_container_cluster.primary.master_auth[0].cluster_ca_certificate)
    token                  = data.google_client_config.current.access_token
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

  image_registry     = local.registry_url
  image_tag          = var.image_tag
  database_url       = local.database_url
  redis_url          = local.redis_url
  admin_password     = var.admin_password
  agent_key          = var.agent_key
  server_host        = var.server_host
  web_host           = var.web_host
  ingress_class_name = "gce"
  server_replicas    = var.server_replicas
  agent_replicas     = var.agent_replicas

  depends_on = [
    google_container_node_pool.primary,
    google_sql_user.user,
    google_redis_instance.cache,
  ]
}
