//! 替身:`names_validated` 对外部用户故意报 `ProvenanceCheckFailed`,复现拦截行为。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::UserSource;
use crate::ports::{DirectoryError, UserDirectory};

#[derive(Default)]
struct State {
    users: BTreeMap<String, (String, UserSource)>,
    fail_direct: bool,
    validated_calls: usize,
    direct_calls: usize,
}

#[derive(Clone, Default)]
pub struct SpyDirectory {
    state: Arc<Mutex<State>>,
}

impl SpyDirectory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_user(self, id: &str, name: &str, source: UserSource) -> Self {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).users.insert(id.to_string(), (name.to_string(), source));
        self
    }

    pub fn failing_direct(self) -> Self {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).fail_direct = true;
        self
    }

    pub fn validated_calls(&self) -> usize {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).validated_calls
    }

    pub fn direct_calls(&self) -> usize {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).direct_calls
    }
}

#[async_trait]
impl UserDirectory for SpyDirectory {
    async fn names_validated(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.validated_calls += 1;
        let has_external =
            ids.iter().any(|id| state.users.get(id).map(|(_, s)| s.is_external()).unwrap_or(false));
        if has_external {
            return Err(DirectoryError::ProvenanceCheckFailed);
        }
        Ok(collect(&state.users, ids))
    }

    async fn names_direct(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.direct_calls += 1;
        if state.fail_direct {
            return Err(DirectoryError::Backend("db down".into()));
        }
        Ok(collect(&state.users, ids))
    }
}

fn collect(
    users: &BTreeMap<String, (String, UserSource)>,
    ids: &[String],
) -> BTreeMap<String, String> {
    ids.iter()
        .filter_map(|id| users.get(id).map(|(name, _)| (id.clone(), name.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn validated_path_blows_up_on_oidc() {
        let dir = SpyDirectory::new().with_user("u1", "Alice", UserSource::Oidc);
        assert_eq!(
            dir.names_validated(&ids(&["u1"])).await,
            Err(DirectoryError::ProvenanceCheckFailed)
        );
    }

    #[tokio::test]
    async fn direct_path_resolves_oidc() {
        let dir = SpyDirectory::new().with_user("u1", "Alice", UserSource::Oidc);
        assert_eq!(
            dir.names_direct(&ids(&["u1"])).await.expect("ok").get("u1"),
            Some(&"Alice".to_string())
        );
    }
}
