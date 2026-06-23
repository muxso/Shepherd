//! 资源池领域模型(零 IO)。
//!
//! 资源池是批量/场景执行的执行节点归属。管理面支持创建/更新/删除,字段对齐参考 UI:
//! 名称、描述、最大并发数、类型(Node/Kubernetes)、应用组织(全部/指定)、工作节点 URL、
//! 类型相关节点配置(config,JSONB)。领域只做与 IO 无关的校验。

use serde_json::Value;
use thiserror::Error;

/// 资源池视图(读模型 / 创建·更新返回)。
///
/// `config` 为类型相关配置(Node 的节点表 / Kubernetes 的连接信息),作为不透明 JSON 透传;
/// `created_at` / `updated_at` 为已格式化的时间字符串(`YYYY-MM-DD HH:MM:SS`)。
#[derive(Debug, Clone, PartialEq)]
pub struct ResourcePool {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub max_concurrency: i32,
    pub pool_type: String,
    pub all_org: bool,
    pub org_ids: Vec<String>,
    pub server_url: String,
    pub config: Value,
    pub created_at: String,
    pub updated_at: String,
}

/// 资源池创建/更新校验错误。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResourcePoolError {
    #[error("resource pool name must not be empty")]
    EmptyName,
    #[error("resource pool type must be Node or Kubernetes")]
    BadType,
    #[error("max concurrency must not be negative")]
    BadConcurrency,
    #[error("specified-org scope requires at least one organization")]
    EmptyOrgScope,
}

/// 创建/更新资源池的原始入参(未校验)。
#[derive(Debug, Clone, Default)]
pub struct ResourcePoolDraft {
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub max_concurrency: i32,
    pub pool_type: String,
    pub all_org: bool,
    pub org_ids: Vec<String>,
    pub server_url: String,
    pub config: Value,
}

/// 已校验的待落库资源池。
#[derive(Debug, Clone, PartialEq)]
pub struct NewResourcePool {
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub max_concurrency: i32,
    pub pool_type: String,
    pub all_org: bool,
    pub org_ids: Vec<String>,
    pub server_url: String,
    pub config: Value,
}

impl NewResourcePool {
    /// 校验入参:名称去空白且非空;类型限 Node/Kubernetes(空兜底 Node);并发非负;
    /// 指定组织时至少一个组织;org_ids 去空白去重空项;描述/URL 去首尾空白。
    pub fn new(draft: ResourcePoolDraft) -> Result<Self, ResourcePoolError> {
        let name = draft.name.trim().to_string();
        if name.is_empty() {
            return Err(ResourcePoolError::EmptyName);
        }
        let raw_type = draft.pool_type.trim();
        let pool_type = if raw_type.is_empty() { "Node" } else { raw_type };
        if pool_type != "Node" && pool_type != "Kubernetes" {
            return Err(ResourcePoolError::BadType);
        }
        if draft.max_concurrency < 0 {
            return Err(ResourcePoolError::BadConcurrency);
        }
        let org_ids: Vec<String> = draft
            .org_ids
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !draft.all_org && org_ids.is_empty() {
            return Err(ResourcePoolError::EmptyOrgScope);
        }
        Ok(Self {
            name,
            enabled: draft.enabled,
            description: draft.description.trim().to_string(),
            max_concurrency: draft.max_concurrency,
            pool_type: pool_type.to_string(),
            all_org: draft.all_org,
            org_ids,
            server_url: draft.server_url.trim().to_string(),
            config: draft.config,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(name: &str) -> ResourcePoolDraft {
        ResourcePoolDraft { name: name.to_string(), enabled: true, all_org: true, ..Default::default() }
    }

    #[test]
    fn trims_and_accepts_name_with_default_type() {
        let p = NewResourcePool::new(draft("  本地池 ")).expect("ok");
        assert_eq!(p.name, "本地池");
        assert!(p.enabled);
        assert_eq!(p.pool_type, "Node"); // 空类型兜底 Node
    }

    #[test]
    fn rejects_blank_name() {
        assert_eq!(NewResourcePool::new(draft("   ")).unwrap_err(), ResourcePoolError::EmptyName);
    }

    #[test]
    fn rejects_bad_type() {
        let d = ResourcePoolDraft { pool_type: "Docker".into(), ..draft("p") };
        assert_eq!(NewResourcePool::new(d).unwrap_err(), ResourcePoolError::BadType);
    }

    #[test]
    fn rejects_negative_concurrency() {
        let d = ResourcePoolDraft { max_concurrency: -1, ..draft("p") };
        assert_eq!(NewResourcePool::new(d).unwrap_err(), ResourcePoolError::BadConcurrency);
    }

    #[test]
    fn specified_org_requires_at_least_one() {
        let d = ResourcePoolDraft { all_org: false, org_ids: vec!["  ".into()], ..draft("p") };
        assert_eq!(NewResourcePool::new(d).unwrap_err(), ResourcePoolError::EmptyOrgScope);
    }

    #[test]
    fn keeps_kubernetes_type_and_trims_orgs() {
        let d = ResourcePoolDraft {
            pool_type: "Kubernetes".into(),
            all_org: false,
            org_ids: vec![" org-1 ".into(), "".into(), "org-2".into()],
            ..draft("k8s")
        };
        let p = NewResourcePool::new(d).expect("ok");
        assert_eq!(p.pool_type, "Kubernetes");
        assert_eq!(p.org_ids, vec!["org-1".to_string(), "org-2".to_string()]);
    }
}
