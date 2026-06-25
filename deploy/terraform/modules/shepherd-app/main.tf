locals {
  # The fleet (multi-host queue) is enabled only when a redis url is provided.
  # Whether redis is configured is not itself secret, so unwrap for use in a
  # non-sensitive `set` block.
  fleet_enabled = nonsensitive(var.redis_url != "")

  # Non-sensitive helm values mapped from module variables.
  base_set = [
    {
      name  = "global.image.registry"
      value = var.image_registry
    },
    {
      name  = "global.image.tag"
      value = var.image_tag
    },
    {
      name  = "server.replicas"
      value = tostring(var.server_replicas)
    },
    {
      name  = "server.ingress.enabled"
      value = tostring(var.ingress_enabled)
    },
    {
      name  = "server.ingress.className"
      value = var.ingress_class_name
    },
    {
      name  = "server.ingress.host"
      value = var.server_host
    },
    {
      name  = "agentRuntime.replicas"
      value = tostring(var.agent_replicas)
    },
    {
      name  = "agentRuntime.mock"
      value = tostring(var.agent_mock)
    },
    {
      name  = "web.ingress.enabled"
      value = tostring(var.ingress_enabled)
    },
    {
      name  = "web.ingress.className"
      value = var.ingress_class_name
    },
    {
      name  = "web.ingress.host"
      value = var.web_host
    },
    {
      name  = "config.sessionTtlSecs"
      value = tostring(var.session_ttl_secs)
    },
    {
      name  = "config.fleet.enabled"
      value = tostring(local.fleet_enabled)
    },
  ]

  # Optional OIDC values, flattened only when provided.
  feishu_set = try(var.oidc.feishu, null) == null ? [] : [
    {
      name  = "config.oidc.feishu.appId"
      value = var.oidc.feishu.app_id
    },
    {
      name  = "config.oidc.feishu.redirect"
      value = var.oidc.feishu.redirect
    },
  ]

  wecom_set = try(var.oidc.wecom, null) == null ? [] : [
    {
      name  = "config.oidc.wecom.corpId"
      value = var.oidc.wecom.corp_id
    },
    {
      name  = "config.oidc.wecom.redirect"
      value = var.oidc.wecom.redirect
    },
  ]
}

resource "kubernetes_namespace" "this" {
  count = var.create_namespace ? 1 : 0

  metadata {
    name = var.namespace
    labels = {
      "app.kubernetes.io/managed-by" = "terraform"
      "app.kubernetes.io/part-of"    = "shepherd"
    }
  }
}

resource "helm_release" "shepherd" {
  name      = var.release_name
  namespace = var.namespace
  chart     = var.chart_path

  create_namespace = false
  timeout          = var.helm_timeout
  wait             = true
  atomic           = true
  dependency_update = true

  # Non-sensitive values.
  dynamic "set" {
    for_each = concat(local.base_set, local.feishu_set, local.wecom_set)
    content {
      name  = set.value.name
      value = set.value.value
    }
  }

  # Sensitive values — never rendered to plan output.
  set_sensitive {
    name  = "config.adminPassword"
    value = var.admin_password
  }

  set_sensitive {
    name  = "database.url"
    value = var.database_url
  }

  set_sensitive {
    name  = "config.fleet.redisUrl"
    value = var.redis_url
  }

  dynamic "set_sensitive" {
    for_each = try(var.oidc.feishu, null) == null ? [] : [1]
    content {
      name  = "config.oidc.feishu.appSecret"
      value = var.oidc.feishu.app_secret
    }
  }

  dynamic "set_sensitive" {
    for_each = try(var.oidc.wecom, null) == null ? [] : [1]
    content {
      name  = "config.oidc.wecom.secret"
      value = var.oidc.wecom.secret
    }
  }

  depends_on = [kubernetes_namespace.this]
}
