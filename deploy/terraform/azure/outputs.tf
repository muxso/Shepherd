output "cluster_name" {
  description = "AKS cluster name."
  value       = azurerm_kubernetes_cluster.aks.name
}

output "kubeconfig_command" {
  description = "Command to fetch cluster credentials into your kubeconfig."
  value       = "az aks get-credentials --resource-group ${azurerm_resource_group.rg.name} --name ${azurerm_kubernetes_cluster.aks.name}"
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
  description = "ACR login server (append /<repo>:<tag>)."
  value       = local.registry_url
}

output "app_url" {
  description = "Public URL of the Shepherd web UI."
  value       = "https://${var.web_host}"
}
