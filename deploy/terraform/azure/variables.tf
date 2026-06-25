variable "location" {
  description = "Azure region."
  type        = string
  default     = "eastus"
}

variable "name_prefix" {
  description = "Prefix applied to all resource names (lowercase, alphanumeric for ACR)."
  type        = string
  default     = "shepherd"
}

variable "resource_group_name" {
  description = "Resource group to create."
  type        = string
  default     = "shepherd-rg"
}

variable "kubernetes_version" {
  description = "AKS Kubernetes version. Empty = AKS default."
  type        = string
  default     = ""
}

variable "node_vm_size" {
  description = "VM size for the AKS default node pool."
  type        = string
  default     = "Standard_D4s_v5"
}

variable "node_count" {
  description = "Node count for the AKS default node pool."
  type        = number
  default     = 3
}

variable "node_min_count" {
  description = "Minimum nodes (autoscaling)."
  type        = number
  default     = 2
}

variable "node_max_count" {
  description = "Maximum nodes (autoscaling)."
  type        = number
  default     = 6
}

variable "db_name" {
  description = "PostgreSQL database name."
  type        = string
  default     = "shepherd"
}

variable "db_username" {
  description = "PostgreSQL administrator login."
  type        = string
  default     = "shepherd"
}

variable "db_password" {
  description = "PostgreSQL administrator password. If empty, a random password is generated."
  type        = string
  sensitive   = true
  default     = ""
}

variable "db_sku_name" {
  description = "Azure Database for PostgreSQL Flexible Server SKU."
  type        = string
  default     = "GP_Standard_D2s_v3"
}

variable "db_storage_mb" {
  description = "PostgreSQL storage size (MiB)."
  type        = number
  default     = 32768
}

variable "redis_capacity" {
  description = "Azure Cache for Redis capacity (0-6 for Basic/Standard)."
  type        = number
  default     = 1
}

variable "redis_sku_name" {
  description = "Azure Cache for Redis SKU (Basic, Standard, Premium)."
  type        = string
  default     = "Standard"
}

variable "redis_family" {
  description = "Azure Cache for Redis family (C for Basic/Standard, P for Premium)."
  type        = string
  default     = "C"
}

variable "admin_password" {
  description = "Shepherd admin password (SHEPHERD_ADMIN_PASSWORD). MUST be set."
  type        = string
  sensitive   = true
}

variable "image_tag" {
  description = "Container image tag to deploy."
  type        = string
  default     = "latest"
}

variable "server_host" {
  description = "Public hostname for the shepherd-server ingress."
  type        = string
  default     = "shepherd.example.com"
}

variable "web_host" {
  description = "Public hostname for the shepherd-web ingress."
  type        = string
  default     = "shepherd.example.com"
}

variable "server_replicas" {
  description = "Number of shepherd-server replicas."
  type        = number
  default     = 2
}

variable "agent_replicas" {
  description = "Number of shepherd-agent-runtime replicas."
  type        = number
  default     = 3
}
