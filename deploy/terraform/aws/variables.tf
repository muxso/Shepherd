variable "region" {
  description = "AWS region to deploy into."
  type        = string
  default     = "us-east-1"
}

variable "name_prefix" {
  description = "Prefix applied to all resource names."
  type        = string
  default     = "shepherd"
}

variable "vpc_cidr" {
  description = "CIDR block for the VPC."
  type        = string
  default     = "10.0.0.0/16"
}

variable "kubernetes_version" {
  description = "EKS Kubernetes version."
  type        = string
  default     = "1.30"
}

variable "node_instance_types" {
  description = "Instance types for the EKS managed node group."
  type        = list(string)
  default     = ["t3.large"]
}

variable "node_desired_size" {
  description = "Desired number of nodes in the managed node group."
  type        = number
  default     = 3
}

variable "node_min_size" {
  description = "Minimum number of nodes."
  type        = number
  default     = 2
}

variable "node_max_size" {
  description = "Maximum number of nodes."
  type        = number
  default     = 6
}

variable "db_name" {
  description = "PostgreSQL database name."
  type        = string
  default     = "shepherd"
}

variable "db_username" {
  description = "PostgreSQL master username."
  type        = string
  default     = "shepherd"
}

variable "db_password" {
  description = "PostgreSQL master password. If empty, a random password is generated."
  type        = string
  sensitive   = true
  default     = ""
}

variable "db_instance_class" {
  description = "RDS instance class."
  type        = string
  default     = "db.t3.medium"
}

variable "db_allocated_storage" {
  description = "RDS allocated storage (GiB)."
  type        = number
  default     = 20
}

variable "redis_node_type" {
  description = "ElastiCache Redis node type."
  type        = string
  default     = "cache.t3.micro"
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

variable "tags" {
  description = "Common tags applied to all resources."
  type        = map(string)
  default = {
    Project   = "shepherd"
    ManagedBy = "terraform"
  }
}
