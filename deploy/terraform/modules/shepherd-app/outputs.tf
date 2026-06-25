output "release_name" {
  description = "Name of the deployed Helm release."
  value       = helm_release.shepherd.name
}

output "namespace" {
  description = "Namespace Shepherd is installed into."
  value       = var.namespace
}
