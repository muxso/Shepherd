output "cluster_name" {
  description = "EKS cluster name."
  value       = module.eks.cluster_name
}

output "kubeconfig_command" {
  description = "Command to update your local kubeconfig for this cluster."
  value       = "aws eks update-kubeconfig --region ${var.region} --name ${module.eks.cluster_name}"
}

output "database_url" {
  description = "PostgreSQL connection string."
  value       = local.database_url
  sensitive   = true
}

output "redis_url" {
  description = "Redis connection string."
  value       = local.redis_url
  sensitive   = true
}

output "registry_url" {
  description = "ECR registry base URL (append /<repo>:<tag>)."
  value       = local.ecr_registry
}

output "app_url" {
  description = "Public URL of the Shepherd web UI."
  value       = "https://${var.web_host}"
}
