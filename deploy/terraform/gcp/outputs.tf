output "cluster_name" {
  description = "GKE cluster name."
  value       = google_container_cluster.primary.name
}

output "kubeconfig_command" {
  description = "Command to fetch cluster credentials into your kubeconfig."
  value       = "gcloud container clusters get-credentials ${google_container_cluster.primary.name} --region ${var.region} --project ${var.project_id}"
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
  description = "Artifact Registry base URL (append /<repo>:<tag>)."
  value       = local.registry_url
}

output "app_url" {
  description = "Public URL of the Shepherd web UI."
  value       = "https://${var.web_host}"
}
