terraform {
  required_version = ">= 1.5"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.40, < 6.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = ">= 2.25, < 3.0"
    }
    helm = {
      source  = "hashicorp/helm"
      version = ">= 2.12, < 3.0"
    }
    random = {
      source  = "hashicorp/random"
      version = ">= 3.5"
    }
  }
}
