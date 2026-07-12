use serde_json::Value;
use thiserror::Error;

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
        ResourcePoolDraft {
            name: name.to_string(),
            enabled: true,
            all_org: true,
            ..Default::default()
        }
    }

    #[test]
    fn trims_and_accepts_name_with_default_type() {
        let p = NewResourcePool::new(draft("  本地池 ")).expect("ok");
        assert_eq!(p.name, "本地池");
        assert!(p.enabled);
        assert_eq!(p.pool_type, "Node");
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
