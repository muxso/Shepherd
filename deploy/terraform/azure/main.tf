###############################################################################
# Providers
###############################################################################

provider "azurerm" {
  features {}
}

locals {
  acr_name = replace("${var.name_prefix}acr", "-", "")

  db_password  = var.db_password != "" ? var.db_password : random_password.db.result
  db_fqdn      = azurerm_postgresql_flexible_server.pg.fqdn
  database_url = "postgres://${var.db_username}:${local.db_password}@${local.db_fqdn}:5432/${var.db_name}"
  redis_url    = "redis://:${azurerm_redis_cache.cache.primary_access_key}@${azurerm_redis_cache.cache.hostname}:6379"
  registry_url = azurerm_container_registry.acr.login_server
}

resource "random_password" "db" {
  length  = 24
  special = false
}

resource "azurerm_resource_group" "rg" {
  name     = var.resource_group_name
  location = var.location
}

###############################################################################
# Networking
###############################################################################

resource "azurerm_virtual_network" "vnet" {
  name                = "${var.name_prefix}-vnet"
  resource_group_name = azurerm_resource_group.rg.name
  location            = azurerm_resource_group.rg.location
  address_space       = ["10.0.0.0/16"]
}

resource "azurerm_subnet" "aks" {
  name                 = "${var.name_prefix}-aks-subnet"
  resource_group_name  = azurerm_resource_group.rg.name
  virtual_network_name = azurerm_virtual_network.vnet.name
  address_prefixes     = ["10.0.0.0/20"]
}

# Delegated subnet for the PostgreSQL Flexible Server (VNet integration).
resource "azurerm_subnet" "db" {
  name                 = "${var.name_prefix}-db-subnet"
  resource_group_name  = azurerm_resource_group.rg.name
  virtual_network_name = azurerm_virtual_network.vnet.name
  address_prefixes     = ["10.0.16.0/24"]

  service_endpoints = ["Microsoft.Storage"]

  delegation {
    name = "fs"
    service_delegation {
      name = "Microsoft.DBforPostgreSQL/flexibleServers"
      actions = [
        "Microsoft.Network/virtualNetworks/subnets/join/action",
      ]
    }
  }
}

resource "azurerm_private_dns_zone" "pg" {
  name                = "${var.name_prefix}.postgres.database.azure.com"
  resource_group_name = azurerm_resource_group.rg.name
}

resource "azurerm_private_dns_zone_virtual_network_link" "pg" {
  name                  = "${var.name_prefix}-pg-link"
  resource_group_name   = azurerm_resource_group.rg.name
  private_dns_zone_name = azurerm_private_dns_zone.pg.name
  virtual_network_id    = azurerm_virtual_network.vnet.id
}

###############################################################################
# AKS
###############################################################################

resource "azurerm_kubernetes_cluster" "aks" {
  name                = "${var.name_prefix}-aks"
  resource_group_name = azurerm_resource_group.rg.name
  location            = azurerm_resource_group.rg.location
  dns_prefix          = "${var.name_prefix}-aks"
  kubernetes_version  = var.kubernetes_version != "" ? var.kubernetes_version : null

  default_node_pool {
    name                 = "default"
    vm_size              = var.node_vm_size
    vnet_subnet_id       = azurerm_subnet.aks.id
    auto_scaling_enabled = true
    node_count           = var.node_count
    min_count            = var.node_min_count
    max_count            = var.node_max_count
  }

  identity {
    type = "SystemAssigned"
  }

  network_profile {
    network_plugin = "azure"
  }

  # Managed NGINX ingress (application routing add-on) provides the
  # "webapprouting.kubernetes.azure.com" ingress class.
  web_app_routing {
    dns_zone_ids = []
  }
}

# Allow AKS kubelet identity to pull from ACR.
resource "azurerm_role_assignment" "aks_acr_pull" {
  scope                = azurerm_container_registry.acr.id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_kubernetes_cluster.aks.kubelet_identity[0].object_id
}

###############################################################################
# Azure Database for PostgreSQL Flexible Server (PostgreSQL 16)
###############################################################################

resource "azurerm_postgresql_flexible_server" "pg" {
  name                = "${var.name_prefix}-pg"
  resource_group_name = azurerm_resource_group.rg.name
  location            = azurerm_resource_group.rg.location

  version  = "16"
  sku_name = var.db_sku_name
  storage_mb = var.db_storage_mb

  administrator_login    = var.db_username
  administrator_password = local.db_password

  delegated_subnet_id = azurerm_subnet.db.id
  private_dns_zone_id  = azurerm_private_dns_zone.pg.id

  zone = "1"

  depends_on = [azurerm_private_dns_zone_virtual_network_link.pg]
}

resource "azurerm_postgresql_flexible_server_database" "db" {
  name      = var.db_name
  server_id = azurerm_postgresql_flexible_server.pg.id
  charset   = "UTF8"
  collation = "en_US.utf8"
}

###############################################################################
# Azure Cache for Redis (Redis 7)
###############################################################################

resource "azurerm_redis_cache" "cache" {
  name                = "${var.name_prefix}-redis"
  resource_group_name = azurerm_resource_group.rg.name
  location            = azurerm_resource_group.rg.location

  capacity      = var.redis_capacity
  family        = var.redis_family
  sku_name      = var.redis_sku_name
  redis_version = 6

  # Enable the non-TLS port so the in-cluster redis:// URL on 6379 works.
  non_ssl_port_enabled = true
  minimum_tls_version  = "1.2"
}

###############################################################################
# Azure Container Registry
###############################################################################

resource "azurerm_container_registry" "acr" {
  name                = local.acr_name
  resource_group_name = azurerm_resource_group.rg.name
  location            = azurerm_resource_group.rg.location
  sku                 = "Standard"
  admin_enabled       = false
}

###############################################################################
# Kubernetes / Helm providers (wired from AKS outputs)
###############################################################################

provider "kubernetes" {
  host                   = azurerm_kubernetes_cluster.aks.kube_config[0].host
  client_certificate     = base64decode(azurerm_kubernetes_cluster.aks.kube_config[0].client_certificate)
  client_key             = base64decode(azurerm_kubernetes_cluster.aks.kube_config[0].client_key)
  cluster_ca_certificate = base64decode(azurerm_kubernetes_cluster.aks.kube_config[0].cluster_ca_certificate)
}

provider "helm" {
  kubernetes {
    host                   = azurerm_kubernetes_cluster.aks.kube_config[0].host
    client_certificate     = base64decode(azurerm_kubernetes_cluster.aks.kube_config[0].client_certificate)
    client_key             = base64decode(azurerm_kubernetes_cluster.aks.kube_config[0].client_key)
    cluster_ca_certificate = base64decode(azurerm_kubernetes_cluster.aks.kube_config[0].cluster_ca_certificate)
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
  database_url        = local.database_url
  redis_url           = local.redis_url
  admin_password      = var.admin_password
  agent_key           = var.agent_key
  server_host         = var.server_host
  web_host            = var.web_host
  ingress_class_name  = "webapprouting.kubernetes.azure.com"
  server_replicas     = var.server_replicas
  agent_replicas      = var.agent_replicas

  depends_on = [
    azurerm_kubernetes_cluster.aks,
    azurerm_postgresql_flexible_server_database.db,
    azurerm_redis_cache.cache,
  ]
}
