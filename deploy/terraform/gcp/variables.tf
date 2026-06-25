variable "project_id" {
  description = "GCP project ID."
  type        = string
}

variable "region" {
  description = "GCP region."
  type        = string
  default     = "us-central1"
}

variable "name_prefix" {
  description = "Prefix applied to all resource names."
  type        = string
  default     = "shepherd"
}

variable "kubernetes_version" {
  description = "GKE release channel min master version (prefix). Leave empty to use channel default."
  type        = string
  default     = ""
}

variable "node_machine_type" {
  description = "Machine type for GKE nodes."
  type        = string
  default     = "e2-standard-4"
}

variable "node_count" {
  description = "Number of nodes per zone in the default node pool."
  type        = number
  default     = 1
}

variable "node_min_count" {
  description = "Minimum nodes per zone (autoscaling)."
  type        = number
  default     = 1
}

variable "node_max_count" {
  description = "Maximum nodes per zone (autoscaling)."
  type        = number
  default     = 3
}

variable "db_name" {
  description = "PostgreSQL database name."
  type        = string
  default     = "shepherd"
}

variable "db_username" {
  description = "PostgreSQL user."
  type        = string
  default     = "shepherd"
}

variable "db_password" {
  description = "PostgreSQL password. If empty, a random password is generated."
  type        = string
  sensitive   = true
  default     = ""
}

variable "db_tier" {
  description = "Cloud SQL machine tier."
  type        = string
  default     = "db-custom-2-7680"
}

variable "redis_memory_size_gb" {
  description = "Memorystore Redis capacity (GiB)."
  type        = number
  default     = 1
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
