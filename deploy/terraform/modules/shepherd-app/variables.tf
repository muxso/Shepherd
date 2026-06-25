variable "namespace" {
  description = "Kubernetes namespace to install Shepherd into. Created by this module."
  type        = string
  default     = "shepherd"
}

variable "release_name" {
  description = "Helm release name."
  type        = string
  default     = "shepherd"
}

variable "image_registry" {
  description = "Container image registry/owner prefix, e.g. ghcr.io/muxso or <account>.dkr.ecr.<region>.amazonaws.com."
  type        = string
  default     = "ghcr.io/muxso"
}

variable "image_tag" {
  description = "Container image tag to deploy."
  type        = string
  default     = "latest"
}

variable "database_url" {
  description = "Full PostgreSQL connection string, e.g. postgres://user:pass@host:5432/db."
  type        = string
  sensitive   = true
}

variable "redis_url" {
  description = "Full Redis connection string for the multi-host fleet, e.g. redis://host:6379. Empty disables the fleet redis."
  type        = string
  sensitive   = true
  default     = ""
}

variable "admin_password" {
  description = "Initial admin password (SHEPHERD_ADMIN_PASSWORD). MUST be set in production."
  type        = string
  sensitive   = true
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

variable "ingress_class_name" {
  description = "Ingress class name (e.g. alb, gce, nginx, webapprouting.kubernetes.azure.com)."
  type        = string
  default     = ""
}

variable "ingress_enabled" {
  description = "Enable the web ingress."
  type        = bool
  default     = true
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

variable "agent_mock" {
  description = "Run the agent runtime in mock mode (AGENT_MOCK=1) — demo only."
  type        = bool
  default     = false
}

variable "session_ttl_secs" {
  description = "Session TTL in seconds (SHEPHERD_SESSION_TTL_SECS)."
  type        = number
  default     = 28800
}

variable "oidc" {
  description = "Optional OIDC provider configuration (Feishu / WeCom)."
  type = object({
    feishu = optional(object({
      app_id     = string
      app_secret = string
      redirect   = string
    }))
    wecom = optional(object({
      corp_id  = string
      secret   = string
      redirect = string
    }))
  })
  # Not marked sensitive as a whole: app_id / corp_id / redirect are public.
  # The app_secret / secret fields are pushed via set_sensitive in main.tf so
  # they never appear in plan output.
  default = null
}

variable "chart_path" {
  description = "Path to the Shepherd Helm chart relative to this module."
  type        = string
  default     = "../../helm/shepherd"
}

variable "helm_timeout" {
  description = "Timeout (seconds) for the helm release operations."
  type        = number
  default     = 600
}

variable "create_namespace" {
  description = "Whether this module should create the namespace."
  type        = bool
  default     = true
}
