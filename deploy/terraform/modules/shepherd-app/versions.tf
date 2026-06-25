terraform {
  required_version = ">= 1.5"

  required_providers {
    helm = {
      source  = "hashicorp/helm"
      version = ">= 2.12, < 3.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = ">= 2.25, < 3.0"
    }
  }
}

# NOTE: This module declares NO provider blocks. The caller (each cloud stack)
# is responsible for configuring and passing the `helm` and `kubernetes`
# providers, typically wired from the freshly-created cluster's outputs.
